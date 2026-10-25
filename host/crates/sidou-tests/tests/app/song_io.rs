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
        Self::on(crate::store::fresh(81, 16))
    }

    fn on(mut store: crate::store::Rig) -> Self {
        let mut song = Song::device();
        let mut ctl = Box::<song_ctl>::default();
        let mut shapes = Box::<[gt2_shape; 2]>::default();
        let mut work = Box::<song_work>::default();
        let mut stream = Box::<gt2_stream>::default();
        let mut io = Box::<song_io>::default();
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
fn an_external_dump_freeze_refuses_every_workspace_or_model_replacement() {
    let mut r = Rig::new();
    r.save(c"CURRENT");
    let before = r.song.write(true);
    unsafe { (*r.song.ptr_mut()).frozen = 1 };
    unsafe {
        assert_eq!(song_io_load(&mut *r.io, c"CURRENT".as_ptr(), 1), 0);
        assert_eq!(song_io_new(&mut *r.io, 1), 0);
        assert_eq!(song_io_save(&mut *r.io, c"OTHER".as_ptr(), r.now), 0);
        assert_eq!(song_io_delete(&mut *r.io, c"CURRENT".as_ptr(), 1), 0);
        assert_eq!(song_io_program(&mut *r.io, 0), 0);
        assert_eq!(song_io_about(&mut *r.io, 1), 0);
    }
    assert_eq!(r.io.state, SONG_IO_IDLE as u8);
    assert_eq!(r.song.write(true), before);
    assert_eq!(unsafe { (*r.song.ptr_mut()).frozen }, 1);
    unsafe { (*r.song.ptr_mut()).frozen = 0 };
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"CURRENT".as_ptr(), 1) },
        1
    );
    r.finish();
}

#[test]
fn an_external_workspace_lease_blocks_new_actions_even_after_io_becomes_idle() {
    let mut r = Rig::new();
    r.save(c"CURRENT");
    r.io.external = 1;
    assert_eq!(unsafe { song_io_busy(&*r.io) }, 0);
    assert_eq!(unsafe { (*r.song.ptr()).frozen }, 0);
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"CURRENT".as_ptr(), 1) },
        0
    );
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"NEXT".as_ptr(), r.now) },
        0
    );
    assert_eq!(unsafe { song_io_new(&mut *r.io, 1) }, 0);
    assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 0);
    assert_eq!(
        unsafe { song_io_delete(&mut *r.io, c"CURRENT".as_ptr(), 1) },
        0
    );
    r.io.external = 0;
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"CURRENT".as_ptr(), 1) },
        1
    );
    r.finish();
}

#[test]
fn internal_midi_staging_songs_do_not_take_a_bank_program_number() {
    let mut r = Rig::new();
    let bytes = sidou_tests::song::SngSpec::new().encode();
    r.typed_file(c"MIDI SONG", STORE_FILE_SONG as u8, &bytes);
    r.typed_file(c"NAMED", STORE_FILE_SONG as u8, &bytes);
    assert_eq!(unsafe { song_io_program(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) },
        c"NAMED"
    );
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

/// What song_io_returning says to each read's loaded hook, in order.
struct Returning {
    io: *const song_io,
    seen: Vec<i32>,
    builtin: Vec<u8>,
}

unsafe extern "C" fn returning_loaded(ctx: *mut std::ffi::c_void) -> i32 {
    let r = unsafe { &mut *ctx.cast::<Returning>() };
    r.seen.push(unsafe { song_io_returning(r.io) });
    1
}

unsafe extern "C" fn returning_builtin(
    ctx: *mut std::ffi::c_void,
    off: u32,
    buf: *mut u8,
    n: u32,
) -> u32 {
    let r = unsafe { &mut *ctx.cast::<Returning>() };
    unsafe { builtin_read((&mut r.builtin as *mut Vec<u8>).cast(), off, buf, n) }
}

/// Ergonomics review E27 (M2 review 5.10): ABOUT's tune and the song coming back at its close
/// are returning reads (the song views keep their places); a load, NEW and a lost file's
/// built-in stand-in are not.
#[test]
fn abouts_tune_and_its_restore_are_returning_reads_and_a_load_is_not() {
    let mut r = Rig::new();
    r.save(c"NIGHT");
    let mut ret = Box::new(Returning {
        io: &*r.io,
        seen: Vec::new(),
        builtin: sidou_tests::song::SngSpec::new().encode(),
    });
    r.io.hooks.ctx = (&mut *ret as *mut Returning).cast();
    r.io.hooks.loaded = Some(returning_loaded);
    r.io.hooks.builtin = Some(returning_builtin);
    assert_eq!(unsafe { song_io_load(&mut *r.io, c"NIGHT".as_ptr(), 1) }, 1);
    r.finish();
    assert_eq!(unsafe { song_io_about(&mut *r.io, 1) }, 1);
    r.finish();
    assert_eq!(unsafe { song_io_about(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(r.io.about, 0);
    assert_eq!(ret.seen, [0, 1, 1], "load, ABOUT's tune, the song back");
    assert_eq!(unsafe { song_io_new(&mut *r.io, 1) }, 1);
    r.finish();
    // ABOUT with the file gone meanwhile: its close falls back to the built-in, a new song.
    r.save(c"NIGHT");
    assert_eq!(unsafe { song_io_about(&mut *r.io, 1) }, 1);
    r.finish();
    assert_eq!(r.store.file_delete("NIGHT"), STORE_OK);
    r.store.settle();
    assert_eq!(unsafe { song_io_about(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(ret.seen[3..], [0, 1, 0], "NEW, ABOUT's tune, the stand-in");
}

/// LOAD's built-in song (SIDOU LOOP, in the firmware image): loaded like a file, it is the
/// song, unnamed and unchanged, nothing remembered for the next power-on (which plays the
/// built-in anyway) and no stored file it would replace: a SAVE writes a store copy. Unsaved
/// edits ask first, as a file's load does; a failed read gives the previous file back.
#[test]
fn the_builtin_song_loads_like_a_file_and_saving_it_makes_a_store_copy() {
    let mut r = Rig::new();
    r.save(c"NIGHT");
    let night = r.song.write(true);
    let builtin = Box::new(sidou_tests::song_embed::sng());
    r.io.hooks.ctx = (&*builtin as *const Vec<u8>).cast_mut().cast();
    r.io.hooks.builtin = Some(builtin_read);
    r.io.dirty = 1;
    assert_eq!(unsafe { song_io_load_builtin(&mut *r.io, 0) }, 0);
    assert_eq!(r.message(), "DISCARD CHANGES?");
    assert_eq!(unsafe { song_io_load_builtin(&mut *r.io, 1) }, 1);
    r.finish();
    assert_ne!(r.song.write(true), night, "the built-in is the song");
    assert_eq!(r.io.name[0], 0, "unnamed");
    assert_eq!(r.io.fid, 0, "no stored file");
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 0);
    assert_eq!(r.io.builtin, 1);
    // SAVE AS: a store copy, the built-in left as it is.
    r.save(c"MY LOOP");
    let mut f = store_file::default();
    assert_eq!(
        unsafe { store_file_open(&*r.store.st, c"MY LOOP".as_ptr(), &mut f) },
        STORE_OK
    );
    // A built-in that fails to read: the previous file comes back.
    assert_eq!(unsafe { song_io_load(&mut *r.io, c"NIGHT".as_ptr(), 1) }, 1);
    r.finish();
    let broken: Box<Vec<u8>> = Box::new(b"GTS5".to_vec());
    r.io.hooks.ctx = (&*broken as *const Vec<u8>).cast_mut().cast();
    assert_eq!(unsafe { song_io_load_builtin(&mut *r.io, 1) }, 1);
    r.finish();
    assert_eq!(r.song.write(true), night);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) },
        c"NIGHT"
    );
}

#[test]
fn a_corrupt_load_reloads_the_previous_file_and_new_uses_the_template_with_the_kit() {
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
    // the lead and the bass, the SID drum kit (core/song/gt2_kit.h, tests/kit.rs), then the
    // digi drums, which the DIGI chunk marks (tests/digi.rs)
    assert_eq!(
        unsafe { gt2_saved_instruments(r.song.ptr()) },
        SONG_IO_DIGI - 1 + SONG_DIGI_KINDS - 1
    );
    assert_eq!(
        r.song.ext_find(sidou_tests::song::tag(b"DIGI")),
        Some(vec![1, 4, 11, 1, 12, 2, 13, 3, 14, 4])
    );
    assert_eq!(r.song.raw().instr[0][GT2_INS_FIRSTWAVE as usize], 0x41);
    assert_eq!(r.song.raw().instr[1][GT2_INS_FIRSTWAVE as usize], 0x41);
    assert_eq!(
        &r.song.raw().instr[2][GT2_INS_NAME as usize..][..4],
        b"KICK"
    );
    assert_ne!(r.io.dirty, 0);
}

struct LoadRuntime {
    value: u16,
    song: *mut gt2_song,
    snapshots: u32,
    restores: u32,
    builtin: Vec<u8>,
}

unsafe extern "C" fn snapshot_runtime(ctx: *mut std::ffi::c_void, storage: *mut u8) {
    let runtime = unsafe { &mut *ctx.cast::<LoadRuntime>() };
    runtime.snapshots += 1;
    unsafe { std::ptr::copy_nonoverlapping(runtime.value.to_le_bytes().as_ptr(), storage, 2) };
}

unsafe extern "C" fn restore_runtime(ctx: *mut std::ffi::c_void, storage: *const u8) {
    let runtime = unsafe { &mut *ctx.cast::<LoadRuntime>() };
    runtime.restores += 1;
    runtime.value = unsafe { u16::from_le_bytes([*storage, *storage.add(1)]) };
}

unsafe extern "C" fn loaded_runtime(ctx: *mut std::ffi::c_void) -> i32 {
    let runtime = unsafe { &mut *ctx.cast::<LoadRuntime>() };
    let mut keys = song_keys::default();
    if unsafe { song_ext_get_keys(runtime.song, &mut keys) } != 0 {
        runtime.value = keys.param[0];
    }
    1
}

unsafe extern "C" fn prepare_runtime(ctx: *mut std::ffi::c_void) -> i32 {
    i32::from(unsafe { &*ctx.cast::<LoadRuntime>() }.value != 99)
}

unsafe extern "C" fn runtime_generation(ctx: *mut std::ffi::c_void) -> u32 {
    u32::from(unsafe { &*ctx.cast::<LoadRuntime>() }.value)
}

unsafe extern "C" fn preflight_runtime(ctx: *mut std::ffi::c_void) -> i32 {
    let runtime = unsafe { &*ctx.cast::<LoadRuntime>() };
    let mut keys = song_keys::default();
    if unsafe { song_ext_get_keys(runtime.song, &mut keys) } != 0 && keys.param[0] == 99 {
        return 0;
    }
    unsafe { loaded_runtime(ctx) }
}

unsafe extern "C" fn runtime_builtin(
    ctx: *mut std::ffi::c_void,
    off: u32,
    buf: *mut u8,
    n: u32,
) -> u32 {
    let runtime = unsafe { &mut *ctx.cast::<LoadRuntime>() };
    unsafe { builtin_read((&mut runtime.builtin as *mut Vec<u8>).cast(), off, buf, n) }
}

#[test]
fn load_refusal_restores_only_applied_runtime_and_keeps_unsaved_metadata_dirty() {
    for reject_before_runtime in [false, true] {
        for unsaved_source in 0..3 {
            let unsaved_metadata = unsaved_source == 1;
            let mut r = Rig::new();
            r.save(c"PLAIN");
            let before = r.song.write(true);
            let mut candidate = Song::device();
            candidate.read(&before, SONG_MULTISPEED_MAX as u8).unwrap();
            let mut keys = song_keys {
                count: 1,
                preset: SONG_KEYS_PRESET_NONE as u8,
                ..Default::default()
            };
            keys.param[0] = 99;
            assert_eq!(unsafe { song_ext_set_keys(candidate.ptr_mut(), &keys) }, 0);
            r.typed_file(c"TOO BIG", STORE_FILE_SONG as u8, &candidate.write(true));
            let mut runtime = Box::new(LoadRuntime {
                value: 23,
                song: r.song.ptr_mut(),
                snapshots: 0,
                restores: 0,
                builtin: Vec::new(),
            });
            r.io.hooks.ctx = (&mut *runtime as *mut LoadRuntime).cast();
            r.io.hooks.snapshot = Some(snapshot_runtime);
            r.io.hooks.restore = Some(restore_runtime);
            r.io.hooks.loaded = if reject_before_runtime {
                Some(preflight_runtime)
            } else {
                Some(loaded_runtime)
            };
            r.io.hooks.prepare = Some(prepare_runtime);
            r.io.hooks.generation = Some(runtime_generation);
            r.io.saved_live = 23;
            runtime.value += u16::from(unsaved_metadata);
            let before_runtime = runtime.value;
            if unsaved_source == 2 {
                let mut edit = gt2_edit::default();
                let row = [GT2_FIRSTNOTE as u8, 1, 0, 0];
                unsafe {
                    assert_eq!(
                        gt2_set_row(r.song.ptr_mut(), 0, 0, row.as_ptr(), &mut edit),
                        GT2_OK
                    );
                    gt2_edit_commit(r.song.ptr_mut(), &edit);
                }
            }
            assert_eq!(r.io.dirty, 0, "metadata dirtiness starts generation-only");
            assert_eq!(
                unsafe { song_io_changed(&*r.io) },
                i32::from(unsaved_source != 0)
            );
            assert_eq!(
                unsafe { song_io_load(&mut *r.io, c"TOO BIG".as_ptr(), 1) },
                1
            );
            r.finish();
            assert_eq!(
                runtime.value, before_runtime,
                "missing KEYS inherited refused runtime"
            );
            assert_eq!(runtime.snapshots, 1);
            assert_eq!(runtime.restores, u32::from(!reject_before_runtime));
            assert_eq!(r.song.write(true), before);
            assert_eq!(r.message(), "SNG: SAVED FORM TOO BIG");
            assert_eq!(
                unsafe { song_io_changed(&*r.io) },
                i32::from(unsaved_metadata),
                "retained unsaved metadata was incorrectly marked clean"
            );
            assert_eq!(
                r.io.snapshot, 0,
                "read backup is not a frozen SAVE snapshot"
            );
            assert_eq!(
                unsafe { std::ffi::CStr::from_ptr(r.io.name.as_ptr()) },
                c"PLAIN"
            );
            assert_eq!(unsafe { song_io_load(&mut *r.io, c"PLAIN".as_ptr(), 1) }, 1);
            r.finish();
            assert_eq!(
                unsafe { song_io_changed(&*r.io) },
                0,
                "accepted LOAD clears dirtiness"
            );
        }
    }
}

#[test]
fn about_keeps_the_original_runtime_snapshot_until_close_and_restore_verification() {
    let mut r = Rig::new();
    r.save(c"PLAIN");
    let before = r.song.write(true);
    let mut demo = Song::device();
    demo.read(&before, SONG_MULTISPEED_MAX as u8).unwrap();
    let mut keys = song_keys {
        count: 1,
        ..Default::default()
    };
    keys.param[0] = 77;
    assert_eq!(unsafe { song_ext_set_keys(demo.ptr_mut(), &keys) }, 0);
    let mut runtime = Box::new(LoadRuntime {
        value: 23,
        song: r.song.ptr_mut(),
        snapshots: 0,
        restores: 0,
        builtin: demo.write(true),
    });
    r.io.hooks.ctx = (&mut *runtime as *mut LoadRuntime).cast();
    r.io.hooks.snapshot = Some(snapshot_runtime);
    r.io.hooks.restore = Some(restore_runtime);
    r.io.hooks.loaded = Some(loaded_runtime);
    r.io.hooks.prepare = Some(prepare_runtime);
    r.io.hooks.builtin = Some(runtime_builtin);
    assert_eq!(unsafe { song_io_about(&mut *r.io, 1) }, 1);
    r.finish();
    assert_eq!(runtime.value, 77);
    assert_eq!(runtime.restores, 0);
    assert_eq!(unsafe { song_io_about(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(runtime.value, 23);
    assert_eq!(runtime.snapshots, 1);
    assert_eq!(runtime.restores, 1);
    assert_eq!(r.song.write(true), before);
    assert_eq!(r.io.about, 0);
    assert_eq!(r.io.dirty, 0);
}

/// SAVE on a store that was never mounted (an unknown flash part, review 1.1) says it failed
/// at once and gives the song back: before, it waited on STORE_BUSY for ever, frozen.
#[test]
fn a_save_on_an_unmounted_store_fails_at_once_and_unfreezes_the_song() {
    let mut r = Rig::on(crate::store::Rig::new(crate::store::Sim::new(5), 16));
    r.io.dirty = 1;
    assert_eq!(unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), 0) }, 1);
    for _ in 0..3 {
        r.step();
    }
    assert_eq!(r.io.state, SONG_IO_IDLE as u8);
    assert_eq!(r.message(), "SONG: SAVE FAILED");
    assert_eq!(unsafe { (*r.song.ptr()).frozen }, 0, "edits go on");
    assert_eq!(r.io.dirty, 1, "still unsaved");
    assert_eq!(r.store.sim.ops, 0);
}

/// An unchanged SAVE is skipped only while the file it would save to is still the one the
/// song was loaded or saved as (review 1.3): after a SysEx DEL of it, or a PUT that replaced
/// it, SAVE writes the song again.
#[test]
fn an_unchanged_save_writes_again_once_its_file_was_deleted_or_replaced() {
    let mut r = Rig::new();
    let song = r.song.write(true);
    r.save(c"NIGHT");
    assert_eq!(r.store.file("NIGHT"), Some(song.clone()));
    // A SysEx DEL of the saved file.
    r.store.maintain();
    assert_eq!(r.store.file_delete("NIGHT"), STORE_OK);
    r.store.settle();
    assert_eq!(
        unsafe { song_io_changed(&*r.io) },
        0,
        "the song itself is unchanged"
    );
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(r.message(), "SONG SAVED");
    assert_eq!(r.store.file("NIGHT"), Some(song.clone()), "written again");
    // A SysEx PUT of other bytes under its name.
    let other = sidou_tests::song::SngSpec::new().encode();
    r.typed_file(c"NIGHT", STORE_FILE_SONG as u8, &other);
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(
        r.store.file("NIGHT"),
        Some(song.clone()),
        "the song, not the host's"
    );
    // Unchanged and untouched since: skipped, as before.
    let seq = r.store.st.seq;
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"NIGHT".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(r.message(), "SONG SAVED");
    assert_eq!(r.store.st.seq, seq, "nothing written");
}

/// The same for a song loaded from the store: the load's file is the one an unchanged SAVE
/// skips for, and a PUT that replaced it meanwhile makes SAVE write the loaded song back.
#[test]
fn an_unchanged_save_after_a_load_writes_once_the_loaded_file_was_replaced() {
    let mut r = Rig::new();
    let mut spec = sidou_tests::song::SngSpec::new();
    spec.strings[0][..4].copy_from_slice(b"LOAD");
    let loaded = spec.encode();
    r.typed_file(c"FOO", STORE_FILE_SONG as u8, &loaded);
    assert_eq!(unsafe { song_io_load(&mut *r.io, c"FOO".as_ptr(), 1) }, 1);
    r.finish();
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 0);
    let song = r.song.write(true);
    let seq = r.store.st.seq;
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"FOO".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(r.store.st.seq, seq, "unchanged since its load: skipped");
    let other = sidou_tests::song::SngSpec::new().encode();
    r.typed_file(c"FOO", STORE_FILE_SONG as u8, &other);
    assert_eq!(
        unsafe { song_io_save(&mut *r.io, c"FOO".as_ptr(), r.now) },
        1
    );
    r.finish();
    assert_eq!(r.message(), "SONG SAVED");
    assert_eq!(r.store.file("FOO"), Some(song));
}

/// A .sid tune takes the song's memory (song_io_away, M3 Task 9): every action is refused, the
/// song counts as unchanged and nothing is read; when the tune is done the song comes back from
/// its file, by name, as it was saved (the memory was overwritten meanwhile), its edits gone.
#[test]
fn away_refuses_everything_and_back_reads_the_song_from_its_file() {
    let mut r = Rig::new();
    r.save(c"NIGHT");
    let saved = r.song.write(true);
    r.io.dirty = 1; // an unsaved edit, lost with the model
    assert_eq!(unsafe { song_io_away(&mut *r.io, 1) }, 1);
    assert_eq!(
        unsafe { song_io_changed(&*r.io) },
        0,
        "away, nothing to lose"
    );
    unsafe {
        assert_eq!(song_io_load(&mut *r.io, c"NIGHT".as_ptr(), 1), 0);
        assert_eq!(r.message(), "SID PLAYING");
        assert_eq!(song_io_new(&mut *r.io, 1), 0);
        assert_eq!(song_io_save(&mut *r.io, c"OTHER".as_ptr(), r.now), 0);
        assert_eq!(song_io_delete(&mut *r.io, c"NIGHT".as_ptr(), 1), 0);
        assert_eq!(song_io_program(&mut *r.io, 0), 0);
        assert_eq!(song_io_about(&mut *r.io, 1), 0);
    }
    // The tune overwrites the model: the song's region is the 6502's memory.
    unsafe {
        let s = r.song.ptr_mut();
        let bytes = (*s).region_bytes;
        std::ptr::write_bytes(s.cast::<u8>(), 0xA5, bytes as usize);
        assert!(!gt2_song_init_wiped(s.cast(), bytes).is_null());
    }
    for _ in 0..100 {
        r.step();
    }
    assert_eq!(r.io.state, SONG_IO_IDLE as u8, "nothing runs while away");
    assert_eq!(unsafe { song_io_away(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(r.song.write(true), saved, "the song back from its file");
    assert_eq!(unsafe { song_io_changed(&*r.io) }, 0);
    assert_eq!(
        unsafe { song_io_load(&mut *r.io, c"NIGHT".as_ptr(), 1) },
        1,
        "and its actions back"
    );
    r.finish();
}

/// NEW's song, never saved, comes back as NEW's empty song.
#[test]
fn away_from_an_unsaved_new_song_comes_back_new() {
    let mut r = Rig::new();
    assert_eq!(unsafe { song_io_new(&mut *r.io, 1) }, 1);
    r.finish();
    let fresh = r.song.write(true);
    assert_eq!(unsafe { song_io_away(&mut *r.io, 1) }, 1);
    unsafe {
        let s = r.song.ptr_mut();
        let bytes = (*s).region_bytes;
        std::ptr::write_bytes(s.cast::<u8>(), 0x5A, bytes as usize);
        gt2_song_init_wiped(s.cast(), bytes);
    }
    assert_eq!(unsafe { song_io_away(&mut *r.io, 0) }, 1);
    r.finish();
    assert_eq!(r.song.write(true), fresh);
    assert_eq!(r.io.name[0], 0, "unnamed, as NEW leaves it");
}
