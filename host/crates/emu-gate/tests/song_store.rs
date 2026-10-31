// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::ffi::CString;

use common::panel::{
    press, wait_for_within, ALGORITHM, HOME, KEY0, PLAY, PRESETS, SAVE, SELECT, SEQ,
};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song};

const NEW: u32 = 1;
const SAVE_AS: u32 = 2;
const LOAD: u32 = 3;
const DELETE: u32 = 4;
const ABOUT_OPEN: u32 = 5;
const ABOUT_CLOSE: u32 = 6;
const OP_TIMEOUT_MS: u64 = 15_000;
const SONG_IO_HOOK_NAME: u32 = 4;
const SONG_IO_HOOK_RESULT: u32 = 24;
const SONG_HOOK_WRITE_ROW: u32 = 1;
const SONG_HOOK_PATTERN: u32 = 4;
const SONG_HOOK_ROW: u32 = 8;
const SONG_HOOK_CELL: u32 = 12;
const SPAN_ROWS_SHIFT: u32 = 16;
const SPAN_OFFSET_MASK: u32 = 0xffff;
const C4: u8 = 48;
const E4: u8 = 52;
const G4: u8 = 55;
const A4: u8 = 57;
const AUDIBLE_PEAK: i32 = 1000;
const MAX_SILENT_HALVES: usize = 4;
const TRUNCATED_BYTES: usize = 17;
const OVERSIZED_PATTERNS: u8 = 120;
const GT2_TITLE: u32 = 4;
const SERVICE_FORCE: u32 = 30;
const SERVICE_HOLDING: u32 = 31;
const SAVE_FAILS_WITHIN_MS: u64 = 500;
const SETTINGS_REC_CARDS: usize = 1;
const SETTINGS_REC_ROWS: usize = 106;
const SETTINGS_DEBOUNCE_MS: u64 = 6_000;
const RESET_STORED_WITHIN_MS: u64 = 3_000;
const GATE_ON: u8 = 0xff;
const GATE_RELEASED: u8 = 0xfe;
const AUDITION_WINDOW_MS: u64 = 300;
const AUDITION_SLACK_MS: u64 = 10;

fn gt2_note(semitone: u8, instrument: u8) -> [u8; 4] {
    [GT2_FIRSTNOTE as u8 + semitone, instrument, 0, 0]
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

fn ctl_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    m.soc.read8(fw.symbol("song_transport") + offset as u32)
}

fn ctl_word(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(fw.symbol("song_transport") + offset as u32)
}

fn until(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, OP_TIMEOUT_MS, what, done);
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
            hook + SONG_IO_HOOK_NAME + i,
            name.as_bytes().get(i as usize).copied().unwrap_or(0),
        );
    }
    m.soc.write32(hook + SONG_IO_HOOK_RESULT, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "main loop consumes song I/O request", |m| {
        m.soc.read32(hook) == 0
    });
    m.soc.read32(hook + SONG_IO_HOOK_RESULT) as i32
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
        "{what}: {} passes, worst {worst} instructions",
        m.passes.len()
    );
    assert_within_budget(
        &format!("{what}: the busiest pass"),
        worst,
        POLL_INSTRUCTIONS,
    );
}

fn powered_on(fw: &Firmware, flash: Option<&[u8]>) -> Machine {
    if let Some(flash) = flash {
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(flash);
        m
    } else {
        Machine::boot(&fw.image)
    }
}

fn past_boot_and_song_load(m: &mut Machine, fw: &Firmware) {
    watch_passes(m, fw);
    run_past_boot(m, fw);
    idle(m, fw);
}

fn boot(fw: &Firmware, flash: Option<&[u8]>) -> Machine {
    let mut m = powered_on(fw, flash);
    past_boot_and_song_load(&mut m, fw);
    m
}

fn song(m: &mut Machine, fw: &Firmware) -> Song {
    let mut song = Song::device();
    let address = fw.symbol("__song_region");
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

fn stored_song_file(flash: &[u8], name: &str) -> Option<(store_file, Vec<u8>)> {
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

fn row_write(m: &mut Machine, fw: &Firmware, pattern: u32, row: u32, cell: [u8; 4]) {
    let hook = fw.symbol("song_test_hook");
    let base = fw.symbol("__song_region");
    let frozen = m.soc.read8(base + ilp32::gt2_song::frozen as u32) != 0;
    assert_eq!(m.soc.read32(hook), 0);
    m.soc.write32(hook + SONG_HOOK_PATTERN, pattern);
    m.soc.write32(hook + SONG_HOOK_ROW, row);
    for (i, byte) in cell.into_iter().enumerate() {
        m.soc.write8(hook + SONG_HOOK_CELL + i as u32, byte);
    }
    m.soc.write32(hook, SONG_HOOK_WRITE_ROW);
    until(m, "main loop consumes row write", |m| {
        m.soc.read32(hook) == 0
    });
    if frozen {
        assert_eq!(toast_text(m, fw), "SAVING");
        return;
    }
    until(m, "row write applies and releases", |m| {
        let span = m
            .soc
            .read32(base + ilp32::gt2_song::pattern as u32 + pattern * 4);
        let arena = m.soc.read32(base + ilp32::gt2_song::arena_offset as u32);
        let address = base + arena + (span & SPAN_OFFSET_MASK) * GT2_UNIT + row * GT2_ROW_BYTES;
        span >> SPAN_ROWS_SHIFT > row + 1
            && cell
                .into_iter()
                .enumerate()
                .all(|(i, value)| m.soc.read8(address + i as u32) == value)
            && m.soc.read16(base + ilp32::gt2_song::pending as u32) == 0
            && m.soc
                .read32(fw.symbol("song_transport") + ilp32::song_ctl::inflight_n as u32)
                == 0
    });
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
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    let c4 = gt2_note(C4, 1);
    let g4 = gt2_note(G4, 1);
    row_write(&mut m, &fw, 0, 0, c4);
    complete(&mut m, &fw, SAVE_AS, "ALPHA");
    let alpha = song(&mut m, &fw);
    row_write(&mut m, &fw, 0, 0, g4);
    complete(&mut m, &fw, SAVE_AS, "ZED");
    let zed = song(&mut m, &fw);
    for voice in 0..3 {
        assert_eq!(alpha.order(0, voice), zed.order(0, voice));
        assert_eq!(
            alpha.pattern(voice).unwrap().len(),
            zed.pattern(voice).unwrap().len()
        );
    }
    press(&mut m, SEQ);
    press(&mut m, SEQ);
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
    press(m, PLAY);
    m.run_ms(3_000);
    let play_block = ctl_word(m, fw, ilp32::song_ctl::play_block) as usize;
    assert!(play_block > 0, "PLAY did not reach the audio interrupt");
    let b_use = m.buffer_snapshots.iter().position(|s| s.bytes[0] != 0);
    let audio = &m.soc.periph.audio;
    assert!(
        audio.iter().any(|&v| i32::from(v).abs() > AUDIBLE_PEAK),
        "vacuous silent playback"
    );
    let rewrites: Vec<_> = b_use.map_or_else(
        || vec![None],
        |b| (b.saturating_sub(1)..=b + 1).map(Some).collect(),
    );
    let mut closest = i32::MAX;
    for silent in 0..=MAX_SILENT_HALVES {
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
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, SAVE_AS, "NIGHT");
    let expected = song(&mut m, &fw);
    let (file, bytes) = stored_song_file(&m.soc.flash.mem, "NIGHT").expect("SAVE stored no song");
    assert_eq!(
        bytes,
        expected.write(true),
        "streamed bytes differ from host serialization"
    );
    assert_eq!(name(&mut m, &fw), "NIGHT");
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::dirty), 0);
    m.run_ms(SETTLE_MS);
    complete(&mut m, &fw, SAVE_AS, "NIGHT");
    m.run_ms(300);
    let (again, unchanged) = stored_song_file(&m.soc.flash.mem, "NIGHT").unwrap();
    assert_eq!(
        again.fid, file.fid,
        "unchanged SAVE rewrote the file and consumed flash wear"
    );
    assert_eq!(unchanged, bytes);
    check_passes(&m, "save and unchanged save");
    let flash = m.soc.flash.mem.clone();
    let mut m = powered_on(&fw, Some(&flash));
    m.snapshot_bytes = vec![fw.symbol("b_on")];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    past_boot_and_song_load(&mut m, &fw);
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
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let before = song(&mut m, &fw).write(false);
    assert_eq!(request(&mut m, &fw, SAVE_AS, "FROZEN"), 1);
    until(&mut m, "save begins streaming", |m| {
        io_byte(m, &fw, ilp32::song_io::state) == SONG_IO_WRITE as u8
    });
    let frozen = song(&mut m, &fw);
    assert_eq!(frozen.raw().frozen, 1);
    assert_eq!(frozen.write(false), before);
    let expected = frozen.write(true);
    row_write(&mut m, &fw, 4, 2, gt2_note(A4, 2));
    assert_eq!(toast_text(&mut m, &fw), "SAVING");
    assert_eq!(
        song(&mut m, &fw).write(true),
        expected,
        "edit changed the frozen song"
    );
    idle(&mut m, &fw);
    assert_eq!(song(&mut m, &fw).raw().frozen, 0);
    assert_eq!(
        stored_song_file(&m.soc.flash.mem, "FROZEN").unwrap().1,
        expected
    );
    row_write(&mut m, &fw, 4, 2, gt2_note(A4, 2));
    assert_ne!(
        song(&mut m, &fw).write(false),
        before,
        "edits stayed frozen after SAVE"
    );
    check_passes(&m, "streaming save with a refused edit");
}

#[test]
fn saving_during_playback_forces_progress_even_with_a_settings_save() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    press(&mut m, PLAY);
    until(&mut m, "song plays", |m| {
        ctl_byte(m, &fw, ilp32::song_ctl::state) == SONG_PLAYING as u8
    });
    assert_eq!(request(&mut m, &fw, SAVE_AS, "PLAY SAVE"), 1);
    until(&mut m, "explicit song save forces held programs", |m| {
        io_byte(m, &fw, ilp32::song_io::forcing) != 0
    });
    press(&mut m, SAVE);
    idle(&mut m, &fw);
    assert!(stored_song_file(&m.soc.flash.mem, "PLAY SAVE").is_some());
    assert_eq!(
        ctl_byte(&mut m, &fw, ilp32::song_ctl::state),
        SONG_PLAYING as u8
    );
    assert_eq!(io_byte(&mut m, &fw, ilp32::song_io::forcing), 0);
    check_passes(&m, "song and settings saves during playback");
}

#[test]
fn refused_corrupt_and_oversized_files_restore_the_previous_saved_song() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, SAVE_AS, "NIGHT");
    let expected = song(&mut m, &fw).write(true);
    m.run_ms(SETTLE_MS);
    let mut flash = m.soc.flash.mem.clone();
    let plain = song(&mut m, &fw).write(false);
    put_file(
        &mut flash,
        "BROKEN",
        STORE_FILE_SONG,
        &plain[..plain.len() - TRUNCATED_BYTES],
    );
    let mut spec = SngSpec::new();
    spec.orders[0][0].0 = (0..OVERSIZED_PATTERNS).collect();
    spec.patterns = (0..OVERSIZED_PATTERNS)
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
    put_file(&mut flash, "TOO BIG", STORE_FILE_SONG, &large);
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
    put_file(
        &mut flash,
        "BAD SETUP",
        STORE_FILE_SONG,
        &bad_setup.write(true),
    );
    let mut m = boot(&fw, Some(&flash));
    for (file, reason) in [
        ("BROKEN", "SNG:"),
        ("TOO BIG", "TOO BIG"),
        ("BAD SETUP", "BAD CHIP SETUP"),
    ] {
        complete(&mut m, &fw, LOAD, file);
        let message = toast_text(&mut m, &fw);
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
    let (file, _) = stored_song_file(&m.soc.flash.mem, "NIGHT").unwrap();
    let at = STORE_LO
        + u32::from(file.sectors[0]) * STORE_SECTOR
        + STORE_HEADER
        + STORE_REC_HEADER
        + GT2_TITLE;
    m.soc.flash.mem[at as usize] ^= 1;
    complete(&mut m, &fw, LOAD, "NIGHT");
    assert!(toast_text(&mut m, &fw).contains("FILE CRC"));
    assert_eq!(song(&mut m, &fw).write(true), expected);
    check_passes(&m, "failed loads and previous-song reloads");
}

#[test]
fn new_template_save_as_and_delete_keep_the_right_identity() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
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
    assert_eq!(
        &instruments[SONG_IO_KIT as usize - 1][GT2_INS_NAME as usize..][..4],
        b"KICK",
        "the kit is not there"
    );
    complete(&mut m, &fw, SAVE_AS, "TEMPLATE");
    let original = stored_song_file(&m.soc.flash.mem, "TEMPLATE").unwrap().1;
    complete(&mut m, &fw, SAVE_AS, "NIGHT-SONG-00001");
    assert_eq!(name(&mut m, &fw), "NIGHT-SONG-00001");
    assert_eq!(
        stored_song_file(&m.soc.flash.mem, "NIGHT-SONG-00001")
            .unwrap()
            .1,
        original
    );
    complete(&mut m, &fw, DELETE, "TEMPLATE");
    m.run_ms(100);
    assert!(stored_song_file(&m.soc.flash.mem, "TEMPLATE").is_none());
    assert_eq!(
        name(&mut m, &fw),
        "NIGHT-SONG-00001",
        "deleting another song changed the loaded song"
    );
    complete(&mut m, &fw, DELETE, "NIGHT-SONG-00001");
    m.run_ms(100);
    assert!(stored_song_file(&m.soc.flash.mem, "NIGHT-SONG-00001").is_none());
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
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    row_write(&mut m, &fw, 0, 0, gt2_note(C4, 1));
    complete(&mut m, &fw, SAVE_AS, "ORIGINAL");
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
        .any(|&v| i32::from(v).abs() > AUDIBLE_PEAK));
    complete(&mut m, &fw, ABOUT_CLOSE, "");
    assert_stopped(&mut m, &fw);
    assert_eq!(name(&mut m, &fw), "ORIGINAL");
    assert_eq!(
        song(&mut m, &fw).write(true),
        original,
        "About did not restore the saved song"
    );
    row_write(&mut m, &fw, 0, 1, gt2_note(E4, 1));
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

#[test]
fn save_on_an_unknown_flash_part_fails_at_once_and_holds_nothing() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    let store = fw.symbol("store_fm1_store");
    m.soc.write8(
        store + ilp32::store::state as u32,
        STORE_STATE_UNMOUNTED as u8,
    );
    let flash = m.soc.flash.mem.clone();
    let start = m.ms();
    assert_eq!(request(&mut m, &fw, SAVE_AS, "NO CHIP"), 1);
    idle(&mut m, &fw);
    assert!(
        m.ms() - start < SAVE_FAILS_WITHIN_MS,
        "SAVE took {} ms",
        m.ms() - start
    );
    assert_eq!(toast_text(&mut m, &fw), "SONG: SAVE FAILED");
    assert_eq!(song(&mut m, &fw).raw().frozen, 0);
    m.run_ms(3 * SETTINGS_FORCE_MS as u64);
    let service = fw.symbol("store_fm1_state");
    assert_eq!(
        m.soc.read8(service + SERVICE_FORCE),
        0,
        "no forced save left on"
    );
    assert_eq!(
        m.soc.read8(service + SERVICE_HOLDING),
        0,
        "the audio is not held"
    );
    assert!(m.soc.flash.mem == flash, "nothing written");
    check_passes(&m, "a save on an unknown flash part");
}

#[test]
fn settings_reset_keeps_the_saved_song_and_stores_the_factory_settings() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, SAVE_AS, "NIGHT");
    let (_, bytes) = stored_song_file(&m.soc.flash.mem, "NIGHT").expect("SAVE stored no song");
    let cards = |m: &mut Machine| m.soc.read8(fw.symbol("ui") + ilp32::views::cards as u32);
    let rows = |m: &mut Machine| {
        m.soc
            .read8(fw.symbol("ui") + (ilp32::views::song + ilp32::song_view::scroll) as u32)
    };
    press(&mut m, HOME);
    m.run_ms(700);
    settings_item(&mut m, VIEWS_SETTINGS_CARDS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    m.turn(
        SELECT,
        -((VIEWS_SETTINGS_CARDS - VIEWS_SETTINGS_ROWS) as i32),
    );
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    assert_eq!((cards(&mut m), rows(&mut m)), (0, 1));
    m.run_ms(SETTINGS_DEBOUNCE_MS);
    let stored = stored_value(&m.soc.flash.mem, SETTINGS_KEY).expect("settings stored");
    assert_eq!(
        (stored[SETTINGS_REC_CARDS], stored[SETTINGS_REC_ROWS]),
        (0, 1)
    );
    m.turn(SELECT, (VIEWS_SETTINGS_RESET - VIEWS_SETTINGS_ROWS) as i32);
    m.run_ms(150);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    press(&mut m, HOME);
    m.run_ms(500);
    assert_eq!((cards(&mut m), rows(&mut m)), (0, 1), "HOME cancelled");
    assert_eq!(
        stored_value(&m.soc.flash.mem, SETTINGS_KEY).as_ref(),
        Some(&stored),
        "nothing written"
    );
    settings_item(&mut m, VIEWS_SETTINGS_RESET);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    let before = stored_value(&m.soc.flash.mem, SETTINGS_KEY).expect("settings stored");
    press(&mut m, SAVE);
    assert_eq!((cards(&mut m), rows(&mut m)), (1, 0), "factory at once");
    let start = m.ms();
    while stored_value(&m.soc.flash.mem, SETTINGS_KEY).as_ref() == Some(&before) {
        m.run_ms(10);
        assert!(
            m.ms() < start + RESET_STORED_WITHIN_MS,
            "RESET waited for the debounce"
        );
    }
    println!("reset stored after {} ms", m.ms() - start);
    m.run_ms(500);
    let rec = stored_value(&m.soc.flash.mem, SETTINGS_KEY).unwrap();
    assert_eq!(
        (rec[SETTINGS_REC_CARDS], rec[SETTINGS_REC_ROWS]),
        (1, 0),
        "CARDS ONCE, ROWS BAR stored"
    );
    let (_, kept) = stored_song_file(&m.soc.flash.mem, "NIGHT").expect("RESET erased the song");
    assert_eq!(kept, bytes, "the song is untouched");
    let flash = m.soc.flash.mem.clone();
    let mut m = boot(&fw, Some(&flash));
    assert_eq!((cards(&mut m), rows(&mut m)), (1, 0));
    assert_eq!(name(&mut m, &fw), "NIGHT");
    assert_eq!(song(&mut m, &fw).write(true), bytes);
}

fn zero_crossings_and_rms(audio: &[i16]) -> (usize, f64) {
    let zc = audio
        .windows(2)
        .filter(|w| (w[0] < 0) != (w[1] < 0))
        .count();
    let sum: f64 = audio.iter().map(|&x| f64::from(x).powi(2)).sum();
    (zc, (sum / audio.len().max(1) as f64).sqrt())
}

#[test]
fn a_new_song_plays_and_auditions_its_lead_at_pitch() {
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    press(&mut m, SEQ);
    m.run_ms(SETTLE_MS);
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("step_view_view"),
        "STEP is up"
    );
    let before = m.soc.periph.audio.len();
    press(&mut m, KEY0);
    let lead_f0 = pitch(&record(&mut m, 400));
    let (zc, rms) = zero_crossings_and_rms(&m.soc.periph.audio[before..]);
    println!("audition: {zc} zero crossings, rms {rms:.0}");
    assert!(
        zc > 40 && rms > 200.0,
        "the audition is silent: {zc} crossings, rms {rms:.0}"
    );
    m.run_ms(600);
    m.turn(PRESETS, 1);
    m.run_ms(100);
    let an_empty_step = KEY0 + 2;
    press(&mut m, an_empty_step);
    let rec = record(&mut m, 400);
    println!(
        "rms {:.0}",
        (rec.iter().map(|x| x * x).sum::<f32>() / rec.len() as f32).sqrt()
    );
    let bass_f0 = pitch(&rec);
    println!("lead {lead_f0:.1} Hz, bass {bass_f0:.1} Hz");
    assert!(
        (bass_f0 * 2.0 / lead_f0 - 1.0).abs() < 0.03,
        "the bass is not an octave under the lead: {bass_f0:.1} vs {lead_f0:.1} Hz"
    );
    m.turn(PRESETS, -1);
    m.run_ms(100);
    for k in [4u8, 8, 12] {
        press(&mut m, KEY0 + k);
        m.run_ms(400);
    }
    let before = m.soc.periph.audio.len();
    press(&mut m, PLAY);
    m.run_ms(3000);
    let (zc, rms) = zero_crossings_and_rms(&m.soc.periph.audio[before..]);
    println!("play: {zc} zero crossings, rms {rms:.0}");
    assert!(
        zc > 200 && rms > 300.0,
        "PLAY is silent: {zc} crossings, rms {rms:.0}"
    );
    press(&mut m, PLAY);
    m.run_ms(500);
    check_passes(&m, "a NEW song's steps played");
}

#[test]
fn a_tap_after_presets_auditions_the_new_instrument_for_its_hold() {
    const TAP_MS: u64 = 40;
    let Some(fw) = firmware("the song store tests") else {
        return;
    };
    let mut m = boot(&fw, None);
    complete(&mut m, &fw, NEW, "");
    press(&mut m, SEQ);
    m.run_ms(SETTLE_MS);
    let ctl = fw.symbol("song_transport");
    let chn = ctl + ilp32::song_ctl::player as u32 + ilp32::gt2_player::chn as u32;
    let key = KEY0;
    press(&mut m, key);
    m.run_ms(400);
    let mut k = 1u8;
    for (turn, instr) in [(2, 3u8), (1, 4), (1, 5), (2, 7), (-3, 4)] {
        m.turn(PRESETS, turn);
        m.run_ms(400);
        for tap in 0..2 {
            let before = m.soc.periph.audio.len();
            let (mut start, mut end) = (None, None);
            m.key(key + k, true);
            for ms in 0..AUDITION_WINDOW_MS {
                if ms == TAP_MS {
                    m.key(key + k, false);
                }
                let i = m.soc.read8(chn + ilp32::gt2_channel::instr as u32);
                let gate = m.soc.read8(chn + ilp32::gt2_channel::gate as u32);
                if start.is_none() && i == instr && gate == GATE_ON {
                    start = Some(ms);
                }
                if start.is_some() && end.is_none() && gate == GATE_RELEASED {
                    end = Some(ms);
                }
                m.run_ms(1);
            }
            m.run_ms(200);
            let what = format!("instrument {instr:02X}, tap {tap}");
            let start = start.unwrap_or_else(|| panic!("{what}: the audition never started"));
            let end = end.unwrap_or_else(|| panic!("{what}: the audition was never released"));
            let (zc, rms) = zero_crossings_and_rms(&m.soc.periph.audio[before..]);
            println!("{what}: {start}..{end} ms, {zc} crossings, rms {rms:.0}");
            assert!(
                end - start + AUDITION_SLACK_MS >= TAP_MS,
                "{what}: sounded {} ms of a {TAP_MS} ms tap",
                end - start
            );
            assert!(rms > 50.0, "{what}: the audition is silent (rms {rms:.0})");
            k += 1;
        }
    }
    check_passes(&m, "taps after PRESETS");
}
