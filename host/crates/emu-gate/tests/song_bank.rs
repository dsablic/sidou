// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bank-1 song selection and per-song MIDI setup, entirely inside fm1-emu.

mod common;

use std::ffi::{c_void, CString};

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
                bytes.len() as u32,
            ),
            STORE_OK
        );
        let mut offset = 0;
        while offset < bytes.len() {
            let written = store_file_write(
                &mut *st,
                bytes.as_ptr().add(offset).cast(),
                (bytes.len() - offset) as u32,
            );
            assert!(written as usize <= bytes.len() - offset);
            offset += written as usize;
            store_poll(&mut *st);
        }
        assert_eq!(store_file_end(&mut *st), STORE_OK);
        while store_file_result(&*st) == STORE_BUSY {
            store_poll(&mut *st);
        }
        assert_eq!(store_file_result(&*st), STORE_OK);
    }
}

fn stored_song(flash: &[u8], name: &str) -> Song {
    let mut flash = flash.to_vec();
    let st = mount_dump(&mut flash);
    let name = CString::new(name).unwrap();
    let mut file = store_file::default();
    assert_eq!(
        unsafe { store_file_open(&*st, name.as_ptr(), &mut file) },
        STORE_OK
    );
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
    let mut song = Song::device();
    song.read(&bytes, SONG_MULTISPEED_MAX as u8).unwrap();
    song
}

fn assert_song_want(m: &mut Machine, fw: &Firmware, expected: &song_chip_setup) {
    let address = fw.symbol("keys_router") + ilp32::keys_route::song_want as u32;
    assert_eq!(
        m.soc.read8(address + ilp32::chip_setup::profile as u32),
        expected.profile,
        "stopped MIDI ownership retained the previous CHPA profile"
    );
    let controls: Vec<_> = (0..4)
        .map(|i| {
            m.soc
                .read8(address + ilp32::chip_setup::controls as u32 + i)
        })
        .collect();
    assert_eq!(
        controls,
        [expected.age, expected.dac, expected.thump, expected.bleed]
    );
    assert_eq!(
        m.soc.read8(address + ilp32::chip_setup::gate_reset as u32),
        u8::from(expected.envelope == 0)
    );
}

#[test]
fn stopped_multi_load_refreshes_chpa_and_cc_save_keeps_the_loaded_base() {
    let Some(fw) = fixture() else { return };
    let mut flash = Machine::boot(&fw.image).soc.flash.mem.clone();
    let alpha = song_chip_setup {
        model: 0,
        profile: SID_CHIP_6581_R3 as u8,
        age: 20,
        dac: 21,
        thump: 22,
        bleed: 23,
        envelope: 1,
        ..Default::default()
    };
    let beta = song_chip_setup {
        model: 0,
        profile: SID_CHIP_6581_R4AR as u8,
        age: 40,
        dac: 41,
        thump: 42,
        bleed: 43,
        envelope: 1,
        ..Default::default()
    };
    let mut midi = midi_route_setup::default();
    unsafe { midi_route_setup_default(&mut midi) };
    midi.mode = MIDI_MODE_MULTI as u8;
    midi.base = 0;
    for (name, chip) in [("ALPHA", &alpha), ("BETA", &beta)] {
        let mut fixture = Song::device();
        assert_eq!(
            unsafe { midi_route_setup_to_song(fixture.ptr_mut(), &midi) },
            0
        );
        assert_eq!(
            unsafe { song_ext_set_chip(fixture.ptr_mut(), sidou_tests::song::TAG_CHPA, chip) },
            0
        );
        put_file(&mut flash, name, &fixture.write(true));
    }
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(&flash);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    run_past_boot(&mut m, &fw);
    idle(&mut m, &fw);
    let mut first_setup = None;
    for (loaded_name, chip) in [("ALPHA", &alpha), ("BETA", &beta)] {
        complete(&mut m, &fw, LOAD, loaded_name);
        m.run_ms(100);
        assert_eq!(name(&mut m, &fw), loaded_name);
        assert_stopped(&mut m, &fw);
        assert_eq!(
            byte(&mut m, &fw, "keys_router", ilp32::keys_route::player),
            1
        );
        let loaded_setup = setup(&mut m, &fw);
        assert_eq!(
            loaded_setup[ilp32::midi_route_setup::mode],
            MIDI_MODE_MULTI as u8
        );
        assert_eq!(loaded_setup[ilp32::midi_route_setup::base], 0);
        if let Some(first) = &first_setup {
            assert_eq!(
                &loaded_setup, first,
                "second LOAD must not change MIDI setup"
            );
        } else {
            first_setup = Some(loaded_setup);
        }
        assert_song_want(&mut m, &fw, chip);
    }
    m.midi(&[0xb0, 86, 60]);
    until(&mut m, "CC86 edits stopped chip A's AGE", |m| {
        byte(
            m,
            &fw,
            "keys_router",
            ilp32::keys_route::song_want + ilp32::chip_setup::controls,
        ) == 60
    });
    let expected = song_chip_setup { age: 60, ..beta };
    assert_song_want(&mut m, &fw, &expected);
    complete(&mut m, &fw, SAVE, "BETA");
    let saved = stored_song(&m.soc.flash.mem, "BETA");
    let mut chip = song_chip_setup::default();
    assert_eq!(
        unsafe { song_ext_get_chip(saved.ptr(), sidou_tests::song::TAG_CHPA, &mut chip) },
        1
    );
    assert_eq!(
        [
            chip.model,
            chip.profile,
            chip.age,
            chip.dac,
            chip.thump,
            chip.bleed,
            chip.envelope
        ],
        [
            expected.model,
            expected.profile,
            expected.age,
            expected.dac,
            expected.thump,
            expected.bleed,
            expected.envelope,
        ],
        "SAVE used a stale CHPA base instead of BETA with only AGE edited"
    );
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}
