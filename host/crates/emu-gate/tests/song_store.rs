// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Task 8's song operations in fm1-emu, with flash retained across power cycles.
//! Requests go only to the firmware's RAM-only test hooks; no physical device is used.

mod common;

use std::ffi::{c_void, CString};

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song};

const NEW: u32 = 1;
const SAVE: u32 = 2;
const LOAD: u32 = 3;
const DELETE: u32 = 4;
const ABOUT_OPEN: u32 = 5;
const ABOUT_CLOSE: u32 = 6;
const OP_TIMEOUT_MS: u64 = 15_000;

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message("the song store tests");
        return None;
    };
    Some(Firmware::load(&build))
}

fn io_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("songs") + offset as u32)
}

fn text(m: &mut Machine, address: u32, length: u32) -> String {
    (0..length)
        .map(|i| m.soc.read8(address + i) as char)
        .take_while(|&c| c != '\0')
        .collect()
}

fn name(m: &mut Machine, fw: &Firmware) -> String {
    text(
        m,
        fw.symbol("songs") + ilp32::song_io::name as u32,
        STORE_NAME_MAX + 1,
    )
}

fn toast(m: &mut Machine, fw: &Firmware) -> String {
    text(
        m,
        fw.symbol("ui") + ilp32::screen::toast as u32,
        SCREEN_TOAST_CHARS,
    )
}

fn ctl_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("song_transport") + offset as u32)
}

fn ctl_word(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(fw.symbol("song_transport") + offset as u32)
}

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < OP_TIMEOUT_MS, "timed out: {what}");
        m.run_ms(1);
    }
}

fn idle(m: &mut Machine, fw: &Firmware) {
    let start = m.ms();
    while io_byte(m, fw, ilp32::song_io::state) != SONG_IO_IDLE as u8 {
        if m.ms() - start >= OP_TIMEOUT_MS {
            let stream = fw.symbol("song_stream");
            panic!(
                concat!(
                    "song I/O timed out: io={} stream={} loading={} ctl={} cmds={}/{} ",
                    "handback={} pc={} pos={} index={} pass={} action={} restoring={} prep={}"
                ),
                io_byte(m, fw, ilp32::song_io::state),
                m.soc.read32(stream + ilp32::gt2_stream::state as u32),
                m.soc.read8(stream + ilp32::gt2_stream::loading as u32),
                ctl_byte(m, fw, ilp32::song_ctl::state),
                ctl_word(m, fw, ilp32::song_ctl::cmd_head),
                ctl_word(m, fw, ilp32::song_ctl::cmd_tail),
                ctl_byte(m, fw, ilp32::song_ctl::handback),
                fw.function_at(m.cpu.pc),
                m.soc.read32(stream + ilp32::gt2_stream::pos as u32),
                m.soc.read32(stream + ilp32::gt2_stream::index as u32),
                m.soc.read8(stream + ilp32::gt2_stream::pass as u32),
                io_byte(m, fw, ilp32::song_io::action),
                io_byte(m, fw, ilp32::song_io::restoring),
                m.soc
                    .read8(fw.chips() + ilp32::reg_ring_chips::prep_pass as u32)
            );
        }
        m.run_ms(1);
    }
}

fn request(m: &mut Machine, fw: &Firmware, op: u32, name: &str) -> i32 {
    assert!(name.len() <= STORE_NAME_MAX as usize);
    let hook = fw.symbol("song_io_test_hook");
    assert_eq!(m.soc.read32(hook), 0, "previous request still pending");
    for i in 0..=STORE_NAME_MAX {
        m.soc.write8(
            hook + 4 + i,
            name.as_bytes().get(i as usize).copied().unwrap_or(0),
        );
    }
    // uint32_t op, char name[17], padding to int32_t result, in the firmware's hook.
    m.soc.write32(hook + 24, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "main loop consumes song I/O request", |m| {
        m.soc.read32(hook) == 0
    });
    m.soc.read32(hook + 24) as i32
}

fn complete(m: &mut Machine, fw: &Firmware, op: u32, name: &str) {
    assert_eq!(request(m, fw, op, name), 1, "operation {op} on {name}");
    idle(m, fw);
}

fn watch_passes(m: &mut Machine, fw: &Firmware) {
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
}

fn check_passes(m: &Machine, what: &str) {
    let worst = m
        .passes
        .iter()
        .copied()
        .max()
        .expect("no main-loop passes measured");
    println!(
        "{what}: {} passes, worst {worst}/{POLL_INSTRUCTIONS} instructions",
        m.passes.len()
    );
    assert!(
        worst <= POLL_INSTRUCTIONS,
        "{what}: main-loop pass cost {worst}"
    );
}

fn boot(fw: &Firmware, flash: Option<&[u8]>) -> Machine {
    let mut m = if let Some(flash) = flash {
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(flash);
        m
    } else {
        Machine::boot(&fw.image)
    };
    watch_passes(&mut m, fw);
    run_past_boot(&mut m, fw);
    idle(&mut m, fw);
    m
}

fn song(m: &mut Machine, fw: &Firmware) -> Song {
    let mut song = Song::device();
    let address = fw.symbol("__song_region");
    // The model region contains offsets, not native pointers; both builds use this layout.
    assert_eq!(ilp32::gt2_song::SIZE, size_of::<gt2_song>());
    for i in 0..SONG_REGION_BYTES {
        unsafe {
            song.ptr_mut()
                .cast::<u8>()
                .add(i as usize)
                .write(m.soc.read8(address + i))
        };
    }
    song
}

fn stored_file(flash: &[u8], name: &str) -> Option<(store_file, Vec<u8>)> {
    let mut flash = flash.to_vec();
    let st = mount_dump(&mut flash);
    let name = CString::new(name).unwrap();
    let mut file = store_file::default();
    let result = unsafe { store_file_open(&*st, name.as_ptr(), &mut file) };
    if result == STORE_NOT_FOUND {
        return None;
    }
    assert_eq!(result, STORE_OK);
    assert_eq!(file.type_, STORE_FILE_SONG as u8);
    let mut bytes = vec![0; file.length as usize];
    assert_eq!(
        unsafe { store_file_read(&*st, &file, 0, bytes.as_mut_ptr().cast(), file.length) },
        file.length as i32
    );
    assert_eq!(
        unsafe { store_crc32(0, bytes.as_ptr().cast(), file.length) },
        file.crc
    );
    Some((file, bytes))
}

fn press_play(m: &mut Machine) {
    m.key(PANEL_BTN_PLAY as u8, true);
    m.run_ms(30);
    m.key(PANEL_BTN_PLAY as u8, false);
}

fn row_write(m: &mut Machine, fw: &Firmware, pattern: u32, row: u32, cell: [u8; 4]) {
    let hook = fw.symbol("song_test_hook");
    assert_eq!(m.soc.read32(hook), 0);
    m.soc.write32(hook + 4, pattern);
    m.soc.write32(hook + 8, row);
    for (i, byte) in cell.into_iter().enumerate() {
        m.soc.write8(hook + 12 + i as u32, byte);
    }
    m.soc.write32(hook, 1);
    until(m, "main loop consumes row write", |m| {
        m.soc.read32(hook) == 0
    });
    m.run_ms(10);
}

fn assert_stopped(m: &mut Machine, fw: &Firmware) {
    assert_eq!(ctl_byte(m, fw, ilp32::song_ctl::isr_playing), 0);
    assert_eq!(ctl_byte(m, fw, ilp32::song_ctl::audition), 0);
}

fn tracker_first_note(m: &mut Machine, fw: &Firmware) -> String {
    let grid = fw.symbol("cells");
    let row = TRACKER_VIEW_TOP + TRACKER_VIEW_CONTEXT * TRACKER_VIEW_ROW_CELLS;
    (0..3)
        .map(|i| {
            let cell = row * GRID_COLS + TRACKER_VIEW_NOTE_COL + 2 * i;
            assert_eq!(
                u32::from(m.soc.read8(grid + ilp32::grid::ch as u32 + cell)),
                GRID_BIG_FIRST,
                "TRACKER note is not drawn at 2x"
            );
            m.soc.read8(grid + ilp32::grid::src as u32 + cell) as char
        })
        .collect()
}

#[test]
fn tracker_refreshes_same_shaped_song_rows_after_load_and_bank_recall() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    let c4 = [GT2_FIRSTNOTE as u8 + 48, 1, 0, 0];
    let g4 = [GT2_FIRSTNOTE as u8 + 55, 1, 0, 0];
    row_write(&mut m, &fw, 0, 0, c4);
    complete(&mut m, &fw, SAVE, "ALPHA");
    let alpha = song(&mut m, &fw);
    row_write(&mut m, &fw, 0, 0, g4);
    complete(&mut m, &fw, SAVE, "ZED");
    let zed = song(&mut m, &fw);
    for voice in 0..3 {
        assert_eq!(alpha.order(0, voice), zed.order(0, voice));
        assert_eq!(
            alpha.pattern(voice).unwrap().len(),
            zed.pattern(voice).unwrap().len()
        );
    }
    press(&mut m, PANEL_BTN_SEQ as u8);
    m.run_ms(SETTLE_MS);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("tracker_view_view"));
    assert_eq!(tracker_first_note(&mut m, &fw), "G-4");

    complete(&mut m, &fw, LOAD, "ALPHA");
    m.run_ms(SETTLE_MS);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("tracker_view_view"));
    assert_eq!(song(&mut m, &fw).pattern(0).unwrap()[0], c4);
    let after_load = tracker_first_note(&mut m, &fw);

    m.midi(&[0xb0, 0, 0, 0xb0, 32, 1, 0xc0, 1]);
    until(&mut m, "bank-1 recalls ZED without leaving TRACKER", |m| {
        assert_eq!(top_view(m, &fw), fw.symbol("tracker_view_view"));
        name(m, &fw) == "ZED" && io_byte(m, &fw, ilp32::song_io::state) == SONG_IO_IDLE as u8
    });
    m.run_ms(SETTLE_MS);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("tracker_view_view"));
    assert_eq!(song(&mut m, &fw).pattern(0).unwrap()[0], g4);
    let after_bank = tracker_first_note(&mut m, &fw);
    assert_eq!(
        (after_load.as_str(), after_bank.as_str()),
        ("C-4", "G-4"),
        "TRACKER kept rows cached from a different same-shaped song"
    );
    assert_stopped(&mut m, &fw);
    check_passes(&m, "same-shaped song loads while TRACKER stays open");
}

fn assert_bit_exact(m: &mut Machine, fw: &Firmware, expected: &Song) {
    assert!(
        !m.buffer_snapshots.is_empty(),
        "capture audio block indices from power-on"
    );
    press_play(m);
    m.run_ms(3_000);
    let play_block = ctl_word(m, fw, ilp32::song_ctl::play_block) as usize;
    assert!(play_block > 0, "PLAY did not reach the audio interrupt");
    let b_use = m.buffer_snapshots.iter().position(|s| s.bytes[0] != 0);
    let audio = &m.soc.periph.audio;
    assert!(
        audio.iter().any(|&v| i32::from(v).abs() > 1000),
        "vacuous silent playback"
    );
    let rewrites: Vec<_> = b_use.map_or_else(
        || vec![None],
        |b| (b.saturating_sub(1)..=b + 1).map(Some).collect(),
    );
    let mut closest = i32::MAX;
    for silent in 0..=4 {
        let skip = 2 * silent * HALF_FRAMES;
        if audio[..skip].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[skip..];
        for &rewrite in &rewrites {
            let host = render_host_song(
                played.len() / 2,
                &SongPlay {
                    song: expected,
                    play_block: play_block - 1,
                    ccs: &[],
                    ticks: &[],
                    edits: &[],
                    stop_block: None,
                    b_use_block: b_use,
                    b_rewrite_block: rewrite,
                },
            );
            let difference = max_diff(played, &host);
            closest = closest.min(difference);
            if difference == 0 {
                return;
            }
        }
    }
    panic!("reloaded song differs from host playback: closest {closest} LSB");
}

#[test]
fn saved_song_survives_reboot_stopped_and_plays_bit_exact() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, SAVE, "NIGHT");
    let expected = song(&mut m, &fw);
    let (file, bytes) = stored_file(&m.soc.flash.mem, "NIGHT").expect("SAVE stored no song");
    assert_eq!(
        bytes,
        expected.write(true),
        "streamed bytes differ from host serialization"
    );
    assert_eq!(name(&mut m, &fw), "NIGHT");
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::dirty), 0);
    m.run_ms(SETTLE_MS);
    complete(&mut m, &fw, SAVE, "NIGHT");
    m.run_ms(300);
    let (again, unchanged) = stored_file(&m.soc.flash.mem, "NIGHT").unwrap();
    assert_eq!(
        again.fid, file.fid,
        "unchanged SAVE rewrote the file and consumed flash wear"
    );
    assert_eq!(unchanged, bytes);
    check_passes(&m, "save and unchanged save");
    let flash = m.soc.flash.mem.clone();
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    m.snapshot_bytes = vec![fw.symbol("b_on")];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    watch_passes(&mut m, &fw);
    run_past_boot(&mut m, &fw);
    idle(&mut m, &fw);
    assert_eq!(
        name(&mut m, &fw),
        "NIGHT",
        "last song was not loaded at boot"
    );
    assert_stopped(&mut m, &fw);
    assert_eq!(song(&mut m, &fw).write(true), bytes);
    assert_bit_exact(&mut m, &fw, &expected);
    check_passes(&m, "boot load and playback");
}

#[test]
fn save_freezes_edits_and_keeps_the_snapshot_at_save() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    let before = song(&mut m, &fw).write(false);
    assert_eq!(request(&mut m, &fw, SAVE, "FROZEN"), 1);
    until(&mut m, "save begins streaming", |m| {
        io_byte(m, &fw, ilp32::song_io::state) == SONG_IO_WRITE as u8
    });
    let frozen = song(&mut m, &fw);
    assert_eq!(frozen.raw().frozen, 1);
    assert_eq!(frozen.write(false), before);
    let expected = frozen.write(true);
    row_write(&mut m, &fw, 4, 2, [0x60 + 4 * 12 + 9, 2, 0, 0]);
    assert_eq!(toast(&mut m, &fw), "SAVING");
    assert_eq!(
        song(&mut m, &fw).write(true),
        expected,
        "edit changed the frozen song"
    );
    idle(&mut m, &fw);
    assert_eq!(song(&mut m, &fw).raw().frozen, 0);
    assert_eq!(stored_file(&m.soc.flash.mem, "FROZEN").unwrap().1, expected);
    row_write(&mut m, &fw, 4, 2, [0x60 + 4 * 12 + 9, 2, 0, 0]);
    assert_ne!(
        song(&mut m, &fw).write(false),
        before,
        "edits stayed frozen after SAVE"
    );
    check_passes(&m, "streaming save with a refused edit");
}

#[test]
fn saving_during_playback_forces_progress_even_with_a_settings_save() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    press(&mut m, 12);
    until(&mut m, "song plays", |m| {
        ctl_byte(m, &fw, ilp32::song_ctl::state) == SONG_PLAYING as u8
    });
    assert_eq!(request(&mut m, &fw, SAVE, "PLAY SAVE"), 1);
    until(&mut m, "explicit song save forces held programs", |m| {
        io_byte(m, &fw, ilp32::song_io::forcing) != 0
    });
    // LIVE's SAVE owns a separate settings transaction and force permission.
    press(&mut m, 9);
    idle(&mut m, &fw);
    assert!(stored_file(&m.soc.flash.mem, "PLAY SAVE").is_some());
    assert_eq!(
        ctl_byte(&mut m, &fw, ilp32::song_ctl::state),
        SONG_PLAYING as u8
    );
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::forcing), 0);
    check_passes(&m, "song and settings saves during playback");
}

unsafe extern "C" fn copy_map(ctx: *mut c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn copy_prog(ctx: *mut c_void, off: u32, data: *const c_void, n: u32) -> i32 {
    for i in 0..n as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) &= *data.cast::<u8>().add(i) };
    }
    STORE_OK
}

unsafe extern "C" fn copy_erase(ctx: *mut c_void, off: u32) -> i32 {
    for i in 0..STORE_SECTOR as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) = 0xff };
    }
    STORE_OK
}

fn put_file(flash: &mut [u8], name: &str, bytes: &[u8]) {
    let hooks = store_flash {
        map: Some(copy_map),
        prog: Some(copy_prog),
        erase: Some(copy_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st = Box::<store>::default();
    let name = CString::new(name).unwrap();
    unsafe {
        store_init(&mut *st, &hooks, STORE_WINDOW_MAX);
        store_mount(&mut *st);
        store_quiet(&mut *st, 1);
        while store_poll(&mut *st) != 0 {}
        assert_eq!(
            store_file_begin(
                &mut *st,
                name.as_ptr(),
                STORE_FILE_SONG as u8,
                bytes.len() as u32
            ),
            STORE_OK
        );
        let mut offset = 0;
        while offset < bytes.len() {
            offset += store_file_write(
                &mut *st,
                bytes.as_ptr().add(offset).cast(),
                (bytes.len() - offset) as u32,
            ) as usize;
            store_poll(&mut *st);
        }
        assert_eq!(store_file_end(&mut *st), STORE_OK);
        while store_file_result(&*st) == STORE_BUSY {
            store_poll(&mut *st);
        }
        assert_eq!(store_file_result(&*st), STORE_OK);
    }
}

#[test]
fn refused_corrupt_and_oversized_files_restore_the_previous_saved_song() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, SAVE, "NIGHT");
    let expected = song(&mut m, &fw).write(true);
    m.run_ms(SETTLE_MS);
    let mut flash = m.soc.flash.mem.clone();
    let plain = song(&mut m, &fw).write(false);
    // Truncation after valid headers forces failure after the reader has overwritten RAM.
    put_file(&mut flash, "BROKEN", &plain[..plain.len() - 17]);
    let mut spec = SngSpec::new();
    spec.orders[0][0].0 = (0..120).collect();
    spec.patterns = (0..120)
        .map(|_| {
            (
                vec![[GT2_FIRSTNOTE as u8, 0, 0, 0]; GT2_MAX_ROWS as usize],
                [0xff, 0, 0, 0],
            )
        })
        .collect();
    let large = spec.encode();
    assert!(large.len() <= STORE_FILE_MAX as usize);
    Song::host()
        .read(&large, SONG_MULTISPEED_MAX as u8)
        .expect("oversized fixture is valid");
    let refusal = Song::device()
        .read(&large, SONG_MULTISPEED_MAX as u8)
        .unwrap_err();
    assert!(refusal.msg.contains("TOO BIG"), "{}", refusal.msg);
    put_file(&mut flash, "TOO BIG", &large);
    let mut bad_setup = Song::device();
    bad_setup
        .read(&expected, SONG_MULTISPEED_MAX as u8)
        .unwrap();
    let chip = song_chip_setup {
        profile: 255,
        ..Default::default()
    };
    assert_eq!(
        unsafe { song_ext_set_chip(bad_setup.ptr_mut(), sidou_tests::song::TAG_CHPA, &chip) },
        0
    );
    put_file(&mut flash, "BAD SETUP", &bad_setup.write(true));
    let mut m = boot(&fw, Some(&flash));
    for (file, reason) in [
        ("BROKEN", "SNG:"),
        ("TOO BIG", "TOO BIG"),
        ("BAD SETUP", "BAD CHIP SETUP"),
    ] {
        complete(&mut m, &fw, LOAD, file);
        let message = toast(&mut m, &fw);
        assert!(
            message.contains(reason),
            "{file}: refusal toast {message:?}"
        );
        if file == "TOO BIG" {
            assert!(
                message.bytes().any(|b| b.is_ascii_digit()),
                "oversized refusal omitted its size: {message:?}"
            );
        }
        assert_eq!(
            name(&mut m, &fw),
            "NIGHT",
            "failed load replaced the previous identity"
        );
        assert_eq!(
            song(&mut m, &fw).write(true),
            expected,
            "{file}: previous song was not restored"
        );
        assert_stopped(&mut m, &fw);
    }
    complete(&mut m, &fw, ABOUT_OPEN, "");
    until(&mut m, "About still plays after a refused load", |m| {
        ctl_byte(m, &fw, ilp32::song_ctl::state) == SONG_PLAYING as u8
    });
    complete(&mut m, &fw, ABOUT_CLOSE, "");
    assert_stopped(&mut m, &fw);
    assert_eq!(song(&mut m, &fw).write(true), expected);
    // A valid-looking GT2 title flipped after mount is still store corruption, not an edit.
    let (file, _) = stored_file(&m.soc.flash.mem, "NIGHT").unwrap();
    let at =
        STORE_LO + u32::from(file.sectors[0]) * STORE_SECTOR + STORE_HEADER + STORE_REC_HEADER + 4;
    m.soc.flash.mem[at as usize] ^= 1;
    complete(&mut m, &fw, LOAD, "NIGHT");
    assert!(toast(&mut m, &fw).contains("FILE CRC"));
    assert_eq!(song(&mut m, &fw).write(true), expected);
    check_passes(&m, "failed loads and previous-song reloads");
}

#[test]
fn new_template_save_as_and_delete_keep_the_right_identity() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    assert_stopped(&mut m, &fw);
    assert_eq!(name(&mut m, &fw), "");
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::dirty), 1);
    let template = song(&mut m, &fw);
    assert_eq!((template.raw().subtunes, template.raw().channels), (1, 3));
    for channel in 0..3 {
        assert_eq!(template.order(0, channel), Some((vec![channel as u8], 0)));
        let rows = template.pattern(channel).unwrap();
        assert_eq!(rows.len(), GT2_DEFAULT_ROWS as usize + 1);
        assert!(rows[..GT2_DEFAULT_ROWS as usize]
            .iter()
            .all(|r| *r == [GT2_REST as u8, 0, 0, 0]));
        assert_eq!(rows.last(), Some(&[GT2_ENDPATT as u8, 0, 0, 0]));
    }
    let instruments = &template.raw().instr;
    assert_ne!(
        instruments[0][GT2_INS_PULSE as usize], 0,
        "lead has no pulse table"
    );
    assert_ne!(
        instruments[0][GT2_INS_WAVE as usize],
        instruments[1][GT2_INS_WAVE as usize]
    );
    assert_ne!(
        instruments[2][GT2_INS_FIRSTWAVE as usize] & 0x80,
        0,
        "drum is not noise"
    );
    complete(&mut m, &fw, SAVE, "TEMPLATE");
    let original = stored_file(&m.soc.flash.mem, "TEMPLATE").unwrap().1;
    complete(&mut m, &fw, SAVE, "NIGHT-SONG-00001");
    assert_eq!(name(&mut m, &fw), "NIGHT-SONG-00001");
    assert_eq!(
        stored_file(&m.soc.flash.mem, "NIGHT-SONG-00001").unwrap().1,
        original
    );
    complete(&mut m, &fw, DELETE, "TEMPLATE");
    m.run_ms(100);
    assert!(stored_file(&m.soc.flash.mem, "TEMPLATE").is_none());
    assert_eq!(
        name(&mut m, &fw),
        "NIGHT-SONG-00001",
        "deleting another song changed the loaded song"
    );
    complete(&mut m, &fw, DELETE, "NIGHT-SONG-00001");
    m.run_ms(100);
    assert!(stored_file(&m.soc.flash.mem, "NIGHT-SONG-00001").is_none());
    assert_eq!(name(&mut m, &fw), "");
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::dirty), 1);
    assert_eq!(
        song(&mut m, &fw).write(true),
        original,
        "DELETE destroyed the in-RAM song"
    );
    check_passes(&m, "NEW, SAVE AS and DELETE");
}

#[test]
fn about_music_restores_the_saved_song_and_leaves_unsaved_edits_silent() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    row_write(&mut m, &fw, 0, 0, [GT2_FIRSTNOTE as u8 + 48, 1, 0, 0]);
    complete(&mut m, &fw, SAVE, "ORIGINAL");
    let original = song(&mut m, &fw).write(true);
    complete(&mut m, &fw, ABOUT_OPEN, "");
    until(&mut m, "About music starts", |m| {
        ctl_byte(m, &fw, ilp32::song_ctl::isr_playing) != 0
    });
    assert_ne!(
        song(&mut m, &fw).write(false),
        song_from_bytes(&original).write(false)
    );
    let from = m.soc.periph.audio.len();
    m.run_ms(1_000);
    assert!(m.soc.periph.audio[from..]
        .iter()
        .any(|&v| i32::from(v).abs() > 1000));
    complete(&mut m, &fw, ABOUT_CLOSE, "");
    assert_stopped(&mut m, &fw);
    assert_eq!(name(&mut m, &fw), "ORIGINAL");
    assert_eq!(
        song(&mut m, &fw).write(true),
        original,
        "About did not restore the saved song"
    );
    row_write(&mut m, &fw, 0, 1, [GT2_FIRSTNOTE as u8 + 52, 1, 0, 0]);
    let edited = song(&mut m, &fw).write(true);
    assert_ne!(edited, original);
    let play_block = ctl_word(&mut m, &fw, ilp32::song_ctl::play_block);
    assert_eq!(
        request(&mut m, &fw, ABOUT_OPEN, ""),
        0,
        "About accepted unsaved changes"
    );
    m.run_ms(500);
    assert_stopped(&mut m, &fw);
    assert_eq!(
        ctl_word(&mut m, &fw, ilp32::song_ctl::play_block),
        play_block
    );
    complete(&mut m, &fw, ABOUT_CLOSE, "");
    assert_eq!(
        song(&mut m, &fw).write(true),
        edited,
        "silent About touched unsaved edits"
    );
    check_passes(&m, "About music, restoration and unsaved refusal");
}

fn song_from_bytes(bytes: &[u8]) -> Song {
    let mut song = Song::device();
    song.read(bytes, SONG_MULTISPEED_MAX as u8).unwrap();
    song
}
