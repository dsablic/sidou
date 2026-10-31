// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::ffi::CString;

use common::panel::{open_midi_page, turn, wait_for_within, ALGORITHM, SELECT};
use common::*;
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::song::Song;

const NEW: u32 = 1;
const SAVE_AS: u32 = 2;
const LOAD: u32 = 3;
const TIMEOUT_MS: u64 = 15_000;
const SONG_IO_HOOK_NAME: u32 = 4;
const SONG_IO_HOOK_RESULT: u32 = 24;
const SONG_HOOK_WRITE_ROW: u32 = 1;
const SONG_HOOK_PATTERN: u32 = 4;
const SONG_HOOK_ROW: u32 = 8;
const SONG_HOOK_CELL: u32 = 12;
const SPAN_ROWS_SHIFT: u32 = 16;
const SPAN_OFFSET_MASK: u32 = 0xffff;
const C4: u8 = 48;
const G4: u8 = 55;
const C5: u8 = 60;
const MISSING_PROGRAM: u8 = 127;
const MIDI_MODE: i32 = 0;
const MIDI_OPTION: i32 = 2;
const MIDI_LEGATO: i32 = 3;
const MIDI_CHORD_2: i32 = 4;
const MIDI_CHORD_3: i32 = 5;
const MIDI_ROWS_PER_BEAT: i32 = 7;
const MODE_DOWN_TO_LIVE: i32 = -9;
const CC_AGE: u8 = 86;
const EDITED_AGE: u8 = 60;

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

fn until(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, TIMEOUT_MS, what, done);
}

fn idle(m: &mut Machine, fw: &Firmware) {
    until(m, "song I/O completes", |m| {
        byte(m, fw, "songs", ilp32::song_io::state) == SONG_IO_IDLE as u8
    });
}

fn past_boot_and_song_load(mut m: Machine, fw: &Firmware) -> Machine {
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    run_past_boot(&mut m, fw);
    idle(&mut m, fw);
    m
}

fn boot(fw: &Firmware) -> Machine {
    past_boot_and_song_load(Machine::boot(&fw.image), fw)
}

fn boot_with(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    past_boot_and_song_load(m, fw)
}

fn request(m: &mut Machine, fw: &Firmware, op: u32, requested_name: &str) {
    assert!(requested_name.len() <= STORE_NAME_MAX as usize);
    let hook = fw.symbol("song_io_test_hook");
    assert_eq!(m.soc.read32(hook), 0);
    for i in 0..=STORE_NAME_MAX {
        m.soc.write8(
            hook + SONG_IO_HOOK_NAME + i,
            requested_name
                .as_bytes()
                .get(i as usize)
                .copied()
                .unwrap_or(0),
        );
    }
    m.soc.write32(hook + SONG_IO_HOOK_RESULT, u32::MAX);
    m.soc.write32(hook, op);
    until(m, "main consumes song I/O hook", |m| {
        m.soc.read32(hook) == 0
    });
    assert_eq!(
        m.soc.read32(hook + SONG_IO_HOOK_RESULT) as i32,
        1,
        "operation {op}"
    );
}

fn complete(m: &mut Machine, fw: &Firmware, op: u32, requested_name: &str) {
    request(m, fw, op, requested_name);
    idle(m, fw);
}

fn first_row_write(m: &mut Machine, fw: &Firmware, semitone: u8) {
    let note = GT2_FIRSTNOTE as u8 + semitone;
    let hook = fw.symbol("song_test_hook");
    assert_eq!(m.soc.read32(hook), 0);
    m.soc.write32(hook + SONG_HOOK_PATTERN, 0);
    m.soc.write32(hook + SONG_HOOK_ROW, 0);
    for (i, value) in [note, 1, 0, 0].into_iter().enumerate() {
        m.soc.write8(hook + SONG_HOOK_CELL + i as u32, value);
    }
    m.soc.write32(hook, SONG_HOOK_WRITE_ROW);
    until(m, "main consumes row edit", |m| m.soc.read32(hook) == 0);
    let base = fw.symbol("__song_region");
    until(m, "row edit applies and releases", |m| {
        let span = m.soc.read32(base + ilp32::gt2_song::pattern as u32);
        let arena = m.soc.read32(base + ilp32::gt2_song::arena_offset as u32);
        let row = base + arena + (span & SPAN_OFFSET_MASK) * GT2_UNIT;
        span >> SPAN_ROWS_SHIFT > 1
            && [note, 1, 0, 0]
                .into_iter()
                .enumerate()
                .all(|(i, value)| m.soc.read8(row + i as u32) == value)
            && m.soc.read16(base + ilp32::gt2_song::pending as u32) == 0
            && m.soc
                .read32(fw.symbol("song_transport") + ilp32::song_ctl::inflight_n as u32)
                == 0
    });
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
    first_row_write(m, fw, C4);
    complete(m, fw, SAVE_AS, "ZED");
    let zed = song(m, fw).write(true);
    complete(m, fw, NEW, "");
    first_row_write(m, fw, G4);
    complete(m, fw, SAVE_AS, "ALPHA");
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
    println!("song bank: {} passes, worst {worst}", m.passes.len());
    assert_within_budget("the song bank's busiest pass", worst, POLL_INSTRUCTIONS);
}

#[test]
fn bank_one_loads_name_order_stopped_and_missing_index_preserves_song() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut m = boot(&fw);
    let (zed, alpha) = two_songs(&mut m, &fw);
    selected(&mut m, &fw, 1, "ZED");
    assert_eq!(song(&mut m, &fw).write(true), zed);
    selected(&mut m, &fw, 0, "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), alpha);
    program(&mut m, MISSING_PROGRAM);
    until(&mut m, "missing song is reported", |m| {
        toast_text(m, &fw) == "SONG NOT FOUND"
    });
    idle(&mut m, &fw);
    assert_eq!(name(&mut m, &fw), "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), alpha);
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}

#[test]
fn bank_one_refuses_unsaved_row_edits_without_discarding_them() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut m = boot(&fw);
    let (_, alpha) = two_songs(&mut m, &fw);
    first_row_write(&mut m, &fw, C5);
    let edited = song(&mut m, &fw).write(true);
    assert_ne!(edited, alpha, "row hook did not make an unsaved edit");
    program(&mut m, 1);
    until(&mut m, "unsaved bank selection is refused", |m| {
        toast_text(m, &fw) == "SAVE YOUR SONG FIRST"
    });
    m.run_ms(100);
    assert_eq!(name(&mut m, &fw), "ALPHA");
    assert_eq!(song(&mut m, &fw).write(true), edited);
    assert_stopped(&mut m, &fw);
    check_passes(&m);
}

#[test]
fn bank_one_refuses_a_program_while_save_is_busy() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut m = boot(&fw);
    let (_, alpha) = two_songs(&mut m, &fw);
    request(&mut m, &fw, SAVE_AS, "BUSY");
    assert_ne!(
        byte(&mut m, &fw, "songs", ilp32::song_io::state),
        SONG_IO_IDLE as u8
    );
    program(&mut m, 1);
    until(&mut m, "busy bank selection is refused", |m| {
        toast_text(m, &fw) == "SAVING"
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

fn edit_midi_items(m: &mut Machine, mut cursor: i32, edits: &[(i32, i32)]) -> i32 {
    for &(item, clicks) in edits {
        if item != cursor {
            turn(m, SELECT, item - cursor);
            cursor = item;
        }
        turn(m, ALGORITHM, clicks);
    }
    cursor
}

fn setup(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let address = fw.symbol("routing") + ilp32::midi_route::setup as u32;
    (0..ilp32::midi_route_setup::SIZE)
        .map(|i| m.soc.read8(address + i as u32))
        .collect()
}

#[test]
fn save_and_load_restore_midi_chunk_and_sync_rows() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut m = boot(&fw);
    complete(&mut m, &fw, NEW, "");
    open_midi_page(&mut m);
    let cursor = edit_midi_items(
        &mut m,
        MIDI_MODE,
        &[
            (MIDI_MODE, MODE_DOWN_TO_LIVE),
            (MIDI_OPTION, 1),
            (MIDI_LEGATO, 1),
            (MIDI_CHORD_2, 1),
            (MIDI_CHORD_3, 2),
            (MIDI_ROWS_PER_BEAT, 2),
        ],
    );
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
    complete(&mut m, &fw, SAVE_AS, "MIDI SETUP");
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
    edit_midi_items(
        &mut m,
        cursor,
        &[(MIDI_ROWS_PER_BEAT, -3), (MIDI_LEGATO, -1)],
    );
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
fn refused_chip_setup_restores_plain_song_and_runtime_metadata() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut flash = settled_flash(&fw.image);
    let plain = sidou_tests::song::SngSpec::new().encode();
    put_file(&mut flash, "PLAIN", STORE_FILE_SONG, &plain);
    let mut candidate = Song::device();
    candidate.read(&plain, SONG_MULTISPEED_MAX as u8).unwrap();
    let bad_chip = song_chip_setup {
        model: 1,
        profile: SID_CHIP_6581_R3 as u8,
        envelope: 1,
        ..Default::default()
    };
    let mut midi = midi_route_setup::default();
    unsafe { midi_route_setup_default(&mut midi) };
    midi.mode = MIDI_MODE_MULTI as u8;
    midi.base = 7;
    midi.rows_per_beat = 9;
    let mut keys = song_keys {
        count: LIVE_PARAM_COUNT as u8,
        preset: 1,
        ..Default::default()
    };
    keys.param[LIVE_PARAM_CUTOFF as usize] = 1379;
    let slots = song_slots {
        count: 1,
        slot: [[1, 23, 24, 25, 26]; SONG_SLOTS_MAX as usize],
    };
    unsafe {
        assert_eq!(midi_route_setup_to_song(candidate.ptr_mut(), &midi), 0);
        assert_eq!(song_ext_set_keys(candidate.ptr_mut(), &keys), 0);
        assert_eq!(song_ext_set_slots(candidate.ptr_mut(), &slots), 0);
        assert_eq!(
            song_ext_set_chip(candidate.ptr_mut(), sidou_tests::song::TAG_CHPA, &bad_chip),
            0
        );
    }
    put_file(
        &mut flash,
        "BAD CHIP",
        STORE_FILE_SONG,
        &candidate.write(true),
    );
    let valid_chip = song_chip_setup {
        model: 0,
        ..bad_chip
    };
    assert_eq!(
        unsafe {
            song_ext_set_chip(
                candidate.ptr_mut(),
                sidou_tests::song::TAG_CHPA,
                &valid_chip,
            )
        },
        0
    );
    let padding = vec![1; (SONG_EXT_BYTES - candidate.raw().ext_used - SONG_CHUNK_HEADER) as usize];
    assert_eq!(
        unsafe {
            song_ext_set(
                candidate.ptr_mut(),
                sidou_tests::song::tag(b"TEST"),
                padding.as_ptr(),
                padding.len() as u32,
            )
        },
        0
    );
    put_file(
        &mut flash,
        "FULL SETUP",
        STORE_FILE_SONG,
        &candidate.write(true),
    );
    let mut m = boot_with(&fw, &flash);
    complete(&mut m, &fw, LOAD, "PLAIN");
    let bend_range = byte(&mut m, &fw, "instrument", ilp32::live::bend_range);
    m.midi(&[
        0xb0, 101, 0, 0xb0, 100, 0, 0xb0, 6, bend_range, 0x90, 60, 100,
    ]);
    let held_keys = |m: &mut Machine| {
        (0..LIVE_VOICES as usize)
            .map(|voice| {
                byte(
                    m,
                    &fw,
                    "instrument",
                    ilp32::live::voice + voice * ilp32::live_voice::SIZE + ilp32::live_voice::gate,
                )
            })
            .collect::<Vec<_>>()
    };
    until(&mut m, "live key and RPN bend ownership are active", |m| {
        byte(m, &fw, "instrument", ilp32::live::bend_range_rpn) == 1 && held_keys(m).contains(&1)
    });
    let before_held = held_keys(&mut m);
    let before_song = song(&mut m, &fw).write(true);
    let before_midi = setup(&mut m, &fw);
    let runtime_keys = |m: &mut Machine| {
        [
            (ilp32::live::preset, 1),
            (ilp32::live::cutoff, 2),
            (ilp32::live::slot, (LIVE_SLOTS * LIVE_SLOT_FIELDS) as usize),
        ]
        .into_iter()
        .flat_map(|(offset, count)| offset..offset + count)
        .map(|offset| m.soc.read8(fw.symbol("instrument") + offset as u32))
        .collect::<Vec<_>>()
    };
    let before_keys = runtime_keys(&mut m);
    for (candidate, reason) in [
        ("BAD CHIP", "SNG: BAD CHIP SETUP"),
        ("FULL SETUP", "SNG: SAVED FORM TOO BIG"),
    ] {
        complete(&mut m, &fw, LOAD, candidate);
        assert_eq!(toast_text(&mut m, &fw), reason);
        assert_eq!(
            held_keys(&mut m),
            before_held,
            "refusal released held live keys"
        );
        assert_eq!(
            byte(&mut m, &fw, "instrument", ilp32::live::bend_range_rpn),
            1,
            "refusal cleared RPN bend ownership"
        );
        assert_eq!(name(&mut m, &fw), "PLAIN");
        assert_eq!(setup(&mut m, &fw), before_midi, "refused MIDI leaked");
        assert_eq!(
            runtime_keys(&mut m),
            before_keys,
            "refused KEYS/SLOT leaked"
        );
        assert_eq!(song(&mut m, &fw).write(true), before_song);
        assert_eq!(byte(&mut m, &fw, "songs", ilp32::song_io::dirty), 0);
        assert_stopped(&mut m, &fw);
    }
    check_passes(&m);
}

#[test]
fn stopped_multi_load_refreshes_chpa_and_cc_save_keeps_the_loaded_base() {
    let Some(fw) = firmware("the song bank tests") else {
        return;
    };
    let mut flash = settled_flash(&fw.image);
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
        put_file(&mut flash, name, STORE_FILE_SONG, &fixture.write(true));
    }
    let mut m = boot_with(&fw, &flash);
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
    m.midi(&[0xb0, CC_AGE, EDITED_AGE]);
    until(&mut m, "CC86 edits stopped chip A's AGE", |m| {
        byte(
            m,
            &fw,
            "keys_router",
            ilp32::keys_route::song_want + ilp32::chip_setup::controls,
        ) == EDITED_AGE
    });
    let expected = song_chip_setup {
        age: EDITED_AGE,
        ..beta
    };
    assert_song_want(&mut m, &fw, &expected);
    complete(&mut m, &fw, SAVE_AS, "BETA");
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
