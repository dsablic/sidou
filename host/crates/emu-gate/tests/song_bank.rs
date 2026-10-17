// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bank-1 song selection and per-song MIDI setup, entirely inside fm1-emu.

mod common;

use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;

const NEW: u32 = 1;
const SAVE: u32 = 2;
const LOAD: u32 = 3;
const TIMEOUT_MS: u64 = 15_000;

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        skip_message("the song bank tests");
        return None;
    };
    Some(Firmware::load(&build))
}

fn byte(m: &mut Machine, fw: &Firmware, symbol: &str, offset: usize) -> u8 {
    m.soc.read8(fw.symbol(symbol) + offset as u32)
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

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < TIMEOUT_MS, "timed out: {what}");
        m.run_ms(1);
    }
}

fn idle(m: &mut Machine, fw: &Firmware) {
    until(m, "song I/O completes", |m| {
        byte(m, fw, "songs", ilp32::song_io::state) == SONG_IO_IDLE as u8
    });
}

fn boot(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    run_past_boot(&mut m, fw);
    idle(&mut m, fw);
    m
}

fn request(m: &mut Machine, fw: &Firmware, op: u32, requested_name: &str) {
    assert!(requested_name.len() <= STORE_NAME_MAX as usize);
    let hook = fw.symbol("song_io_test_hook");
    assert_eq!(m.soc.read32(hook), 0);
    for i in 0..=STORE_NAME_MAX {
        m.soc.write8(
            hook + 4 + i,
            requested_name
                .as_bytes()
                .get(i as usize)
                .copied()
                .unwrap_or(0),
        );
    }
    m.soc.write32(hook + 24, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "main consumes song I/O hook", |m| {
        m.soc.read32(hook) == 0
    });
    assert_eq!(m.soc.read32(hook + 24) as i32, 1, "operation {op}");
}

fn complete(m: &mut Machine, fw: &Firmware, op: u32, requested_name: &str) {
    request(m, fw, op, requested_name);
    idle(m, fw);
}

fn row_write(m: &mut Machine, fw: &Firmware, note: u8) {
    let hook = fw.symbol("song_test_hook");
    assert_eq!(m.soc.read32(hook), 0);
    m.soc.write32(hook + 4, 0);
    m.soc.write32(hook + 8, 0);
    for (i, value) in [note, 1, 0, 0].into_iter().enumerate() {
        m.soc.write8(hook + 12 + i as u32, value);
    }
    m.soc.write32(hook, 1);
    until(m, "main consumes row edit", |m| m.soc.read32(hook) == 0);
    m.run_ms(10);
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
                .write(m.soc.read8(address + i));
        }
    }
    song
}

fn two_songs(m: &mut Machine, fw: &Firmware) -> (Vec<u8>, Vec<u8>) {
    complete(m, fw, NEW, "");
    row_write(m, fw, GT2_FIRSTNOTE as u8 + 48);
    complete(m, fw, SAVE, "ZED");
    let zed = song(m, fw).write(true);
    complete(m, fw, NEW, "");
    row_write(m, fw, GT2_FIRSTNOTE as u8 + 55);
    complete(m, fw, SAVE, "ALPHA");
    let alpha = song(m, fw).write(true);
    assert_ne!(zed, alpha);
    (zed, alpha)
}

fn program(m: &mut Machine, index: u8) {
    m.midi(&[0xb0, 0, 0, 0xb0, 32, 1, 0xc0, index]);
}

fn selected(m: &mut Machine, fw: &Firmware, index: u8, expected_name: &str) {
    program(m, index);
    until(m, "bank-1 program loads the selected song", |m| {
        name(m, fw) == expected_name
            && byte(m, fw, "songs", ilp32::song_io::state) == SONG_IO_IDLE as u8
    });
    m.run_ms(20);
    assert_stopped(m, fw);
    assert_eq!(byte(m, fw, "songs", ilp32::song_io::dirty), 0);
    assert_eq!(
        byte(m, fw, "routing", ilp32::midi_route::song_req),
        MIDI_ROUTE_NO_REQUEST as u8,
        "main did not consume the router's song request"
    );
}

fn assert_stopped(m: &mut Machine, fw: &Firmware) {
    assert_eq!(
        byte(m, fw, "song_transport", ilp32::song_ctl::isr_playing),
        0
    );
    assert_eq!(byte(m, fw, "song_transport", ilp32::song_ctl::audition), 0);
}

fn check_passes(m: &Machine) {
    let worst = m.passes.iter().copied().max().expect("no measured passes");
    println!(
        "song bank: {} passes, worst {worst}/{POLL_INSTRUCTIONS}",
        m.passes.len()
    );
    assert!(worst <= POLL_INSTRUCTIONS, "song bank pass cost {worst}");
}

#[test]
fn bank_one_loads_name_order_stopped_and_missing_index_preserves_song() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw);
    let (zed, alpha) = two_songs(&mut m, &fw);
    selected(&mut m, &fw, 1, "ZED");
    assert_eq!(song(&mut m, &fw).write(true), zed);
    selected(&mut m, &fw, 0, "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), alpha);
    program(&mut m, 127);
    until(&mut m, "missing song is reported", |m| {
        toast(m, &fw) == "SONG NOT FOUND"
    });
    idle(&mut m, &fw);
    assert_eq!(name(&mut m, &fw), "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), alpha);
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}

#[test]
fn bank_one_refuses_unsaved_row_edits_without_discarding_them() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw);
    let (_, alpha) = two_songs(&mut m, &fw);
    row_write(&mut m, &fw, GT2_FIRSTNOTE as u8 + 60);
    let edited = song(&mut m, &fw).write(true);
    assert_ne!(edited, alpha, "row hook did not make an unsaved edit");
    program(&mut m, 1);
    until(&mut m, "unsaved bank selection is refused", |m| {
        toast(m, &fw) == "SAVE YOUR SONG FIRST"
    });
    m.run_ms(100);
    assert_eq!(name(&mut m, &fw), "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), edited);
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}

#[test]
fn bank_one_refuses_a_program_while_save_is_busy() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw);
    let (_, alpha) = two_songs(&mut m, &fw);
    request(&mut m, &fw, SAVE, "BUSY");
    assert_ne!(
        byte(&mut m, &fw, "songs", ilp32::song_io::state),
        SONG_IO_IDLE as u8
    );
    program(&mut m, 1);
    until(&mut m, "busy bank selection is refused", |m| {
        toast(m, &fw) == "SAVING"
    });
    idle(&mut m, &fw);
    m.run_ms(100);
    assert_eq!(
        name(&mut m, &fw),
        "BUSY",
        "busy program replaced the SAVE target"
    );
    assert_eq!(song(&mut m, &fw).write(true), alpha);
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

fn turn(m: &mut Machine, encoder: u8, clicks: i32) {
    m.turn(encoder, clicks);
    m.run_ms(10 * u64::from(clicks.unsigned_abs()) + 20);
}

fn open_midi_page(m: &mut Machine) {
    press(m, PANEL_BTN_HOME as u8);
    m.run_ms(700);
    turn(m, PANEL_ENC_SELECT as u8, -(VIEWS_MENU_ITEMS as i32));
    turn(m, PANEL_ENC_SELECT as u8, VIEWS_MENU_MIDI as i32);
    turn(m, PANEL_ENC_ALGORITHM as u8, 1);
    m.run_ms(700);
}

fn setup(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let address = fw.symbol("routing") + ilp32::midi_route::setup as u32;
    (0..ilp32::midi_route_setup::SIZE)
        .map(|i| m.soc.read8(address + i as u32))
        .collect()
}

#[test]
fn save_and_load_restore_midi_chunk_and_sync_rows() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw);
    complete(&mut m, &fw, NEW, "");
    open_midi_page(&mut m);
    let select = PANEL_ENC_SELECT as u8;
    let algorithm = PANEL_ENC_ALGORITHM as u8;
    turn(&mut m, algorithm, -9);
    turn(&mut m, select, 2);
    turn(&mut m, algorithm, 1);
    turn(&mut m, select, 1);
    turn(&mut m, algorithm, 1);
    turn(&mut m, select, 1);
    turn(&mut m, algorithm, 1);
    turn(&mut m, select, 1);
    turn(&mut m, algorithm, 2);
    turn(&mut m, select, 2);
    turn(&mut m, algorithm, 2);
    let expected = setup(&mut m, &fw);
    assert_eq!(
        expected[ilp32::midi_route_setup::mode],
        MIDI_MODE_LIVE as u8
    );
    assert_eq!(expected[ilp32::midi_route_setup::base], 0);
    assert_eq!(
        expected[ilp32::midi_route_setup::option],
        MIDI_WHOLE_CHORD as u8
    );
    assert_eq!(expected[ilp32::midi_route_setup::legato], 1);
    complete(&mut m, &fw, SAVE, "MIDI SETUP");
    let saved = song(&mut m, &fw);
    let mut chunk = song_midi::default();
    assert_eq!(
        unsafe { song_ext_get_midi(saved.ptr(), &mut chunk) },
        1,
        "SAVE omitted MIDI chunk"
    );
    assert_eq!(
        chunk.rows_per_beat,
        expected[ilp32::midi_route_setup::rows_per_beat]
    );
    turn(&mut m, algorithm, -3);
    turn(&mut m, select, -4);
    turn(&mut m, algorithm, -1);
    assert_ne!(setup(&mut m, &fw), expected);
    complete(&mut m, &fw, LOAD, "MIDI SETUP");
    assert_eq!(
        setup(&mut m, &fw),
        expected,
        "LOAD did not restore the saved MIDI setup"
    );
    assert_eq!(
        byte(&mut m, &fw, "syncing", ilp32::midi_sync::rows_per_beat),
        expected[ilp32::midi_route_setup::rows_per_beat],
        "clock rows did not follow the loaded MIDI setup"
    );
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}
