// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;
use sidou_tests::song::Song;

struct Rig {
    song: Song,
    ctl: Box<song_ctl>,
    _shapes: Box<[gt2_shape; 2]>,
    _work: Box<song_work>,
    _stream: Box<gt2_stream>,
    io: Box<song_io>,
    store: crate::store::Rig,
    now: u32,
}

impl Rig {
    fn new() -> Self {
        let mut song = Song::device();
        let mut ctl = Box::<song_ctl>::default();
        let mut shapes = Box::<[gt2_shape; 2]>::default();
        let mut work = Box::<song_work>::default();
        let mut stream = Box::<gt2_stream>::default();
        let mut io = Box::<song_io>::default();
        let mut store = crate::store::fresh(81, 16);
        unsafe {
            let (a, b) = shapes.split_at_mut(1);
            song_ctl_init(&mut *ctl, song.ptr_mut(), &mut a[0], &mut b[0], &mut *work);
            song_io_init(
                &mut *io,
                &mut *ctl,
                &mut *store.st,
                std::ptr::null_mut(),
                (&mut *stream as *mut gt2_stream).cast(),
                std::ptr::null(),
            );
        }
        Self {
            song,
            ctl,
            _shapes: shapes,
            _work: work,
            _stream: stream,
            io,
            store,
            now: 0,
        }
    }

    fn step(&mut self) {
        self.now += 1;
        unsafe {
            song_ctl_poll(&mut *self.ctl, SONG_POLL_RING);
            song_io_poll(&mut *self.io, self.now);
        }
        self.store.poll();
    }

    fn finish(&mut self) {
        for _ in 0..100_000 {
            self.step();
            if self.io.state == SONG_IO_IDLE as u8 {
                return;
            }
        }
        panic!("song IO did not finish: state {}", self.io.state);
    }

    fn save(&mut self, name: &std::ffi::CStr) {
        self.io.dirty = 1;
        assert_eq!(
            unsafe { song_io_save(&mut *self.io, name.as_ptr(), self.now) },
            1
        );
        self.finish();
        assert_eq!(self.message(), "SONG SAVED");
    }

    fn message(&self) -> String {
        unsafe { std::ffi::CStr::from_ptr(self.io.message.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    fn typed_file(&mut self, name: &std::ffi::CStr, typ: u8, bytes: &[u8]) {
        self.store.maintain();
        assert_eq!(
            unsafe {
                store_file_begin(&mut *self.store.st, name.as_ptr(), typ, bytes.len() as u32)
            },
            STORE_OK
        );
        assert_eq!(
            self.store
                .file_stream(bytes, STORE_WINDOW_MAX as usize, None),
            STORE_OK
        );
    }
}

#[test]
fn program_lookup_walks_one_name_per_poll_and_counts_only_songs() {
    let mut r = Rig::new();
    let mut spec = sidou_tests::song::SngSpec::new();
    spec.strings[0][..6].copy_from_slice(b"MIDDLE");
    let bytes = spec.encode();
    for (name, typ) in [
        (c"ZULU", STORE_FILE_SONG as u8),
        (c"MIDDLE", STORE_FILE_SONG as u8),
        (c"B DUMP", 2),
        (c"ALPHA", STORE_FILE_SONG as u8),
        (c"A DUMP", 2),
    ] {
        r.typed_file(name, typ, &bytes);
    }
    let before = r.song.write(true);
    r.ctl.state = SONG_PLAYING as u8;
    assert_eq!(unsafe { song_io_program(&mut *r.io, 1) }, 1);
    assert_ne!(unsafe { song_io_busy(&*r.io) }, 0);
    assert_eq!(unsafe { song_io_saving(&*r.io) }, 0);
    for name in [c"A DUMP", c"ALPHA", c"B DUMP", c"MIDDLE"] {
        r.step();
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(r.io.file.name.as_ptr()) },
            name
        );
        assert_eq!(r.song.write(true), before);
        assert_eq!(r.ctl.state, SONG_PLAYING as u8);
        assert_eq!(r.ctl.cmd_head, r.ctl.cmd_tail);
    }
    assert_eq!(r.io.state, SONG_IO_STOP as u8);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.target.as_ptr()) },
        c"MIDDLE"
    );
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) }, c"");
}

#[test]
fn missing_program_keeps_the_song_playing_and_rejects_indices_without_truncation() {
    let mut r = Rig::new();
    r.save(c"CURRENT");
    let before = r.song.write(true);
    let revision = r.io.saved_revision;
    r.ctl.state = SONG_PLAYING as u8;
    for program in [1, 127, 256, u32::MAX] {
        assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 1);
        let accepted = unsafe { song_io_program(&mut *r.io, program) };
        if accepted != 0 {
            r.finish();
        }
        assert_eq!(r.io.state, SONG_IO_IDLE as u8);
        assert_eq!(r.song.write(true), before);
        assert_eq!(r.ctl.state, SONG_PLAYING as u8);
        assert_eq!(r.ctl.cmd_head, r.ctl.cmd_tail);
        assert_eq!(r.io.saved_revision, revision);
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) },
            c"CURRENT"
        );
        assert_eq!(r.message(), "SONG NOT FOUND");
    }
    let mut empty = Rig::new();
    assert_eq!(unsafe { song_io_program(&mut *empty.io, 0) }, 1);
    empty.finish();
    assert_eq!(empty.message(), "SONG NOT FOUND");
}

#[test]
fn program_refuses_unsaved_model_and_live_edits_even_during_lookup() {
    for source in 0..3 {
        let mut r = Rig::new();
        r.save(c"CURRENT");
        let mut generation = Box::new(0u32);
        r.io.hooks.ctx = (&mut *generation as *mut u32).cast();
        r.io.hooks.generation = Some(live_generation);
        match source {
            0 => r.io.dirty = 1,
            1 => unsafe { (*r.song.ptr_mut()).revision += 1 },
            _ => *generation = 1,
        }
        let before = r.song.write(true);
        assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 0);
        assert_eq!(r.io.state, SONG_IO_IDLE as u8);
        assert_eq!(r.song.write(true), before);
        assert!(matches!(
            r.message().as_str(),
            "SAVE YOUR SONG FIRST" | "DISCARD CHANGES?"
        ));
    }
    let mut r = Rig::new();
    r.save(c"CURRENT");
    assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 1);
    r.io.dirty = 1;
    let before = r.song.write(true);
    r.finish();
    assert_eq!(r.song.write(true), before);
    assert_ne!(unsafe { song_io_changed(&*r.io) }, 0);
    assert!(matches!(
        r.message().as_str(),
        "SAVE YOUR SONG FIRST" | "DISCARD CHANGES?"
    ));
}

#[test]
fn program_refuses_busy_and_about_without_replacing_their_cursors_or_targets() {
    let mut r = Rig::new();
    r.save(c"CURRENT");
    let before = r.song.write(true);
    for (state, about) in [
        (SONG_IO_WRITE, 0),
        (SONG_IO_READ, 0),
        (SONG_IO_STOP, 0),
        (SONG_IO_VERIFY, 0),
        (SONG_IO_IDLE, 1),
    ] {
        r.io.state = state as u8;
        r.io.about = about;
        let target = r.io.target;
        let name = r.io.name;
        let cursor = r.io.file.name;
        let action = r.io.action;
        assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 0);
        assert_eq!(r.io.state, state as u8);
        assert_eq!(r.io.about, about);
        assert_eq!(r.io.target, target);
        assert_eq!(r.io.name, name);
        assert_eq!(r.io.file.name, cursor);
        assert_eq!(r.io.action, action);
        assert_eq!(r.song.write(true), before);
    }
}

#[test]
fn latest_program_restarts_lookup_and_loads_stopped_through_the_normal_slices() {
    let mut r = Rig::new();
    let mut spec = sidou_tests::song::SngSpec::new();
    for name in [c"ZULU", c"ALPHA"] {
        spec.strings[0][..name.to_bytes().len()].copy_from_slice(name.to_bytes());
        r.typed_file(name, STORE_FILE_SONG as u8, &spec.encode());
    }
    let mut chip = Box::<sid>::default();
    unsafe {
        sid_init(&mut *chip, SID_MODEL_6581, SID_CLOCK_PAL, 44100);
        assert_eq!(song_ctl_play(&mut *r.ctl, 0, 0), 1);
    }
    for _ in 0..100_000 {
        unsafe {
            song_ctl_poll(&mut *r.ctl, SONG_POLL_RING | SONG_POLL_SLICE);
            song_ctl_commit(&mut *r.ctl);
            song_ctl_block(&mut *r.ctl, &mut *chip);
        }
        if r.ctl.isr_playing != 0 {
            break;
        }
    }
    assert_ne!(r.ctl.isr_playing, 0);
    assert_eq!(unsafe { song_io_program(&mut *r.io, 1) }, 1);
    r.step();
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.file.name.as_ptr()) },
        c"ALPHA"
    );
    assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 1);
    r.step();
    assert_eq!(r.io.state, SONG_IO_STOP as u8);
    assert_eq!(r.ctl.state, SONG_PLAYING as u8);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.target.as_ptr()) },
        c"ALPHA"
    );
    r.step();
    assert_eq!(r.io.state, SONG_IO_STOP as u8);
    unsafe {
        song_ctl_commit(&mut *r.ctl);
        song_ctl_block(&mut *r.ctl, &mut *chip);
    }
    r.step();
    assert_eq!(r.io.state, SONG_IO_VERIFY as u8);
    assert_eq!(unsafe { song_io_program(&mut *r.io, 1) }, 0);
    r.finish();
    assert_eq!(&r.song.write(true)[4..9], b"ALPHA");
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) },
        c"ALPHA"
    );
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 0);
    assert_eq!(r.ctl.isr_playing, 0);
    assert_ne!(r.ctl.state, SONG_PLAYING as u8);
    assert_eq!(r.ctl.cmd_head, r.ctl.cmd_tail);
}

#[test]
fn unsaved_changes_require_confirmation() {
    let mut io = song_io {
        dirty: 1,
        ..Default::default()
    };
    assert_eq!(unsafe { song_io_new(&mut io, 0) }, 0);
    assert_eq!(io.state, SONG_IO_IDLE as u8);
    let message = unsafe { std::ffi::CStr::from_ptr(io.message.as_ptr()) };
    assert_eq!(message, c"DISCARD CHANGES?");
}

#[test]
fn saving_streams_with_backpressure_freezes_edits_and_skips_unchanged_saves() {
    let mut r = Rig::new();
    let before = r.song.write(true);
    r.io.dirty = 1;
    assert_eq!(unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), 0) }, 1);
    r.step();
    assert_eq!(r.io.state, SONG_IO_WRITE as u8);
    let mut edit = gt2_edit::default();
    let cell = [GT2_FIRSTNOTE as u8, 0, 0, 0];
    assert_eq!(
        unsafe { gt2_set_row(r.song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit) },
        GT2_ERR_SAVING,
    );
    r.finish();
    assert_eq!(r.store.file("NIGHT"), Some(before));
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 0);
    let seq = r.store.st.seq;
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(r.store.st.seq, seq, "unchanged save programs no records");
    assert_eq!(r.message(), "SONG SAVED");
}

unsafe extern "C" fn live_generation(ctx: *mut std::ffi::c_void) -> u32 {
    unsafe { *ctx.cast::<u32>() }
}

struct Capture {
    value: u16,
    io: *mut song_io,
    song: *mut gt2_song,
}

unsafe extern "C" fn snapshot_value(ctx: *mut std::ffi::c_void, storage: *mut u8) {
    let c = unsafe { &*ctx.cast::<Capture>() };
    unsafe { std::ptr::copy_nonoverlapping(c.value.to_le_bytes().as_ptr(), storage, 2) };
}

unsafe extern "C" fn captured_generation(ctx: *mut std::ffi::c_void) -> u32 {
    u32::from(unsafe { &*ctx.cast::<Capture>() }.value)
}

unsafe extern "C" fn prepare_captured(ctx: *mut std::ffi::c_void) -> i32 {
    let c = unsafe { &*ctx.cast::<Capture>() };
    let data = unsafe { &*c.io };
    let mut keys = song_keys {
        count: 1,
        preset: SONG_KEYS_PRESET_NONE as u8,
        ..Default::default()
    };
    keys.param[0] = u16::from_le_bytes([data.data[0], data.data[1]]);
    i32::from(unsafe { song_ext_set_keys(c.song, &keys) } == 0)
}

#[test]
fn a_busy_store_save_keeps_the_metadata_at_the_press_and_freezes_new_edits_at_once() {
    let mut r = Rig::new();
    let mut c = Box::new(Capture {
        value: 23,
        io: &mut *r.io,
        song: r.song.ptr_mut(),
    });
    r.io.hooks.ctx = (&mut *c as *mut Capture).cast();
    r.io.hooks.snapshot = Some(snapshot_value);
    r.io.hooks.prepare = Some(prepare_captured);
    r.io.hooks.generation = Some(captured_generation);
    assert_eq!(r.store.put(17, &[1; 256]), STORE_OK);
    assert_eq!(unsafe { song_io_save(&mut *r.io, c"PRESS".as_ptr(), 0) }, 1);
    c.value = 99;
    let mut edit = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_set_instr(r.song.ptr_mut(), 1, 0, 9, &mut edit) },
        GT2_ERR_SAVING
    );
    r.finish();
    let mut file = Song::device();
    file.read(&r.store.file("PRESS").unwrap(), 4).unwrap();
    let mut keys = song_keys::default();
    assert_eq!(unsafe { song_ext_get_keys(file.ptr(), &mut keys) }, 1);
    assert_eq!(
        keys.param[0], 23,
        "SAVE must not capture a later live value while waiting"
    );
    assert_eq!(
        unsafe { song_io_changed(&*r.io) },
        1,
        "the later change is still unsaved"
    );
    assert_eq!(r.song.raw().frozen, 0);
}

#[test]
fn live_changes_during_a_save_stay_dirty_and_preflight_refusal_keeps_unsaved_edits() {
    let mut r = Rig::new();
    let mut generation = Box::new(0u32);
    r.io.hooks.ctx = (&mut *generation as *mut u32).cast();
    r.io.hooks.generation = Some(live_generation);
    r.io.dirty = 1;
    assert_eq!(unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), 0) }, 1);
    r.step();
    *generation = 1;
    r.finish();
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 1);
    assert_eq!(unsafe { song_io_new(&mut *r.io, 0) }, 0);
    assert_eq!(r.store.file_now("BROKEN", b"GTS5"), STORE_OK);
    let mut edit = gt2_edit::default();
    let cell = [GT2_FIRSTNOTE as u8, 0, 0, 0];
    unsafe {
        assert_eq!(
            gt2_set_row(r.song.ptr_mut(), 0, 0, cell.as_ptr(), &mut edit),
            GT2_OK
        );
        gt2_edit_commit(r.song.ptr_mut(), &edit);
    }
    let before = r.song.write(true);
    r.ctl.shapes_used = 1;
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"BROKEN".as_ptr(), 1) },
        1
    );
    r.finish();
    assert_eq!(
        r.song.write(true),
        before,
        "preflight failure must not discard edits"
    );
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 1);
    assert_eq!(
        r.ctl.shapes_used, 1,
        "preflight keeps the stopped song's runtime setup"
    );
}

unsafe extern "C" fn builtin_read(
    ctx: *mut std::ffi::c_void,
    off: u32,
    buf: *mut u8,
    n: u32,
) -> u32 {
    let bytes = unsafe { &*ctx.cast::<Vec<u8>>() };
    let at = off as usize;
    let len = (n as usize).min(bytes.len().saturating_sub(at));
    if len != 0 {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr().add(at), buf, len) };
    }
    len as u32
}

#[test]
fn a_failed_about_restore_releases_ownership_and_marks_the_remaining_tune_unsaved() {
    let mut r = Rig::new();
    r.save(c"NIGHT");
    let mut builtin = Box::new(sidou_tests::song::SngSpec::new().encode());
    r.io.hooks.ctx = (&mut *builtin as *mut Vec<u8>).cast();
    r.io.hooks.builtin = Some(builtin_read);
    assert_eq!(unsafe { song_io_about(&mut *r.io, 1) }, 1);
    r.finish();
    assert_ne!(r.io.about, 0);
    assert_eq!(r.store.file_now("NIGHT", b"GTS5"), STORE_OK);
    assert_eq!(unsafe { song_io_about(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(r.io.about, 0);
    assert_eq!(r.io.about_close, 0);
    assert_ne!(r.io.dirty, 0);
    assert_eq!(r.message(), "SONG: RELOAD FAILED");
    assert_eq!(unsafe { song_io_new(&mut *r.io, 1) }, 1);
}

#[test]
fn a_corrupt_load_reloads_the_previous_file_and_new_uses_the_three_instrument_template() {
    let mut r = Rig::new();
    r.save(c"NIGHT");
    let before = r.song.write(true);
    assert_eq!(r.store.file_now("BROKEN", b"GTS5"), STORE_OK);
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"BROKEN".as_ptr(), 1) },
        1
    );
    r.finish();
    assert_eq!(r.song.write(true), before);
    assert!(r.message().starts_with("SNG:"), "{}", r.message());
    assert_eq!(unsafe { song_io_new(&mut *r.io, 1) }, 1);
    r.finish();
    assert_eq!(r.song.raw().subtunes, 1);
    for p in 0..3 {
        assert_eq!(unsafe { gt2_order_entries(r.song.ptr(), 0, p) }, 1);
        assert_eq!(unsafe { gt2_pattern_rows(r.song.ptr(), p) }, 65);
    }
    assert_eq!(unsafe { gt2_saved_instruments(r.song.ptr()) }, 3);
    assert_eq!(r.song.raw().instr[0][GT2_INS_FIRSTWAVE as usize], 0x41);
    assert_eq!(r.song.raw().instr[2][GT2_INS_FIRSTWAVE as usize], 0x81);
    assert_ne!(r.io.dirty, 0);
}
