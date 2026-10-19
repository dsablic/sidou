// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU frames over the emulated TRS jack; no physical MIDI or USB device is opened.

mod common;

use common::{build_dir, mount_dump, Firmware, Machine, POLL_INSTRUCTIONS};
use fm1_core::Bus;
use sidou_sys::*;

const PACKET_TIMEOUT_MS: u64 = 1_000;
const PROGS: u32 = 0;
const ERASES: u32 = 4;
const HOLDS: u32 = 16;
const CHORD: [u8; 3] = [30, 34, 37];

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message("the SysEx emulator tests");
        return None;
    };
    Some(Firmware::load(&build))
}

fn byte(m: &mut Machine, base: u32, offset: usize) -> u8 {
    m.soc.read8(base + offset as u32)
}

fn until(m: &mut Machine, what: &str, mut done: impl FnMut(&mut Machine) -> bool) -> u64 {
    until_ms(m, what, PACKET_TIMEOUT_MS, &mut done)
}

fn until_ms(
    m: &mut Machine,
    what: &str,
    timeout: u64,
    mut done: impl FnMut(&mut Machine) -> bool,
) -> u64 {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < timeout, "timed out: {what}");
        m.run_ms(1);
    }
    m.ms() - start
}

fn ready(m: &mut Machine, fw: &Firmware, sequence: u8) -> u64 {
    let s = fw.symbol("sidou_sysex");
    let storage = fw.symbol("store_fm1_store");
    let started = m.ms();
    until(m, "TRS packet programmed and next sequence accepted", |m| {
        if m.ms() - started >= PACKET_TIMEOUT_MS {
            eprintln!(
                concat!(
                "SysEx state={} sequence={} wanted={} owner={} claimed={} running={} writing={} ",
                "filephase={} fill={} taken={} offset={}"),
                byte(m, s, ilp32::sysex::state),
                byte(m, s, ilp32::sysex::sequence),
                sequence,
                byte(m, s, ilp32::sysex::owner),
                byte(m, s, ilp32::sysex::claimed),
                m.soc.read8(fw.symbol("midi_sysex_running")),
                m.soc.read8(fw.symbol("midi_sysex_writing")),
                byte(
                    m,
                    storage,
                    ilp32::store::f + ilp32::store_file_writer::phase
                ),
                m.soc
                    .read16(storage + (ilp32::store::f + ilp32::store_file_writer::fill) as u32),
                m.soc
                    .read32(storage + (ilp32::store::f + ilp32::store_file_writer::taken) as u32),
                m.soc.read32(s + ilp32::sysex::offset as u32)
            );
        }
        // UPLOAD is private to sysex.c; sequence advances only after DATA is programmed.
        byte(m, s, ilp32::sysex::state) == 1
            && byte(m, s, ilp32::sysex::sequence) == sequence
            && m.soc
                .read16(storage + (ilp32::store::f + ilp32::store_file_writer::fill) as u32)
                == 0
    })
}

fn frame(command: u8, sequence: u8, data: &[u8]) -> Vec<u8> {
    let mut out = vec![0xf0, 0x7d, 0x53, 0x44, command, sequence];
    for group in data.chunks(7) {
        out.push(
            group
                .iter()
                .enumerate()
                .fold(0, |mask, (i, b)| mask | ((b >> 7) << i)),
        );
        out.extend(group.iter().map(|b| b & 127));
    }
    let sum = out[4..].iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    out.extend([sum.wrapping_neg() & 127, 0xf7]);
    out
}

fn send(m: &mut Machine, command: u8, sequence: u8, data: &[u8]) {
    let bytes = frame(command, sequence, data);
    let ms = (bytes.len() as u64 * 320).div_ceil(1000) + 8;
    m.midi(&bytes);
    m.run_ms(ms);
}

fn begin(m: &mut Machine, target: u8, instrument: u8, name: &str, data: &[u8]) {
    let mut metadata = vec![target, instrument];
    metadata.extend((0..16).map(|i| name.as_bytes().get(i).copied().unwrap_or(0)));
    metadata.push(1);
    metadata.extend((data.len() as u32).to_le_bytes());
    // SAFETY: data is valid for its length; the CRC routine does not retain it.
    metadata
        .extend(unsafe { store_crc32(0, data.as_ptr().cast(), data.len() as u32) }.to_le_bytes());
    send(m, 4, 0, &metadata);
}

fn upload(
    m: &mut Machine,
    fw: &Firmware,
    data: &[u8],
    mut after_packet: impl FnMut(&mut Machine, usize),
) -> u64 {
    ready(m, fw, 1);
    let mut sequence = 1;
    let mut worst = 0;
    for (i, packet) in data.chunks(SYSEX_PACKET_MAX as usize).enumerate() {
        send(m, 5, sequence, packet);
        sequence = (sequence + 1) & 127;
        worst = worst.max(ready(m, fw, sequence));
        after_packet(m, i);
    }
    let before_end = m.soc.read32(fw.symbol("uptime_ms"));
    send(m, 6, sequence, &[]);
    let s = fw.symbol("sidou_sysex");
    let timeout = if byte(m, s, ilp32::sysex::target) == 0 {
        PACKET_TIMEOUT_MS
    } else {
        15_000
    };
    until_ms(m, "END committed and import completed", timeout, |m| {
        byte(m, s, ilp32::sysex::state) == 0 && byte(m, s, ilp32::sysex::claimed) == 0
    });
    // store_fm1_state's input_ms follows the last non-real-time byte, including END.
    let last_input = m.soc.read32(fw.symbol("store_fm1_state") + 16);
    let now = m.soc.read32(fw.symbol("uptime_ms"));
    assert!(
        last_input.wrapping_sub(before_end) <= now.wrapping_sub(before_end),
        "quiet accounting did not observe END's MIDI activity"
    );
    worst
}

#[test]
fn a_current_song_put_finishes_the_normal_loader_before_releasing_the_transfer() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, true);
    let song = sidou_tests::song_embed::sng();
    put(&mut m, &fw, 2, 0, "", &song);
    assert_eq!(stored(&mut m, c"MIDI SONG"), Some(song));
    let io = fw.symbol("songs");
    assert_eq!(byte(&mut m, io, ilp32::song_io::state), SONG_IO_IDLE as u8);
    assert_eq!(byte(&mut m, io, ilp32::song_io::external), 0);
    let name: Vec<u8> = (0..9)
        .map(|i| m.soc.read8(io + ilp32::song_io::name as u32 + i))
        .collect();
    assert_eq!(name, b"MIDI SONG");
    assert_eq!(
        m.soc
            .read8(fw.symbol("__song_region") + ilp32::gt2_song::frozen as u32),
        0
    );
    let retained = stored(&mut m, c"MIDI SONG").unwrap();
    begin(&mut m, 2, 0, "", &[0x33; 32]);
    assert_eq!(
        byte(&mut m, fw.symbol("sidou_sysex"), ilp32::sysex::claimed),
        0
    );
    send(&mut m, 5, 1, &[0x33; 32]);
    send(&mut m, 6, 2, &[]);
    assert_eq!(stored(&mut m, c"MIDI SONG"), Some(retained));
    let toast = fw.symbol("ui") + ilp32::screen::toast as u32;
    let text: Vec<u8> = (0..13).map(|i| m.soc.read8(toast + i)).collect();
    assert_eq!(text, b"SAVE AS FIRST");
    check_passes(&m);
}

fn put(m: &mut Machine, fw: &Firmware, target: u8, instrument: u8, name: &str, data: &[u8]) {
    begin(m, target, instrument, name, data);
    upload(m, fw, data, |_, _| {});
}

fn stored(m: &mut Machine, name: &std::ffi::CStr) -> Option<Vec<u8>> {
    let mut flash = m.soc.flash.mem.clone();
    let st = mount_dump(&mut flash);
    let mut file = store_file::default();
    // SAFETY: st, file and the output buffer outlive each store call.
    unsafe {
        if store_file_open(&*st, name.as_ptr(), &mut file) != STORE_OK {
            return None;
        }
        let mut data = vec![0; file.length as usize];
        assert_eq!(
            store_file_read(&*st, &file, 0, data.as_mut_ptr().cast(), file.length),
            file.length as i32
        );
        Some(data)
    }
}

fn boot(fw: &Firmware, windows: bool) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    until(&mut m, "song I/O idle after boot", |m| {
        byte(m, fw.symbol("songs"), ilp32::song_io::state) == SONG_IO_IDLE as u8
    });
    if windows {
        m.soc.write32(fw.symbol("store_fm1_window"), 256);
    }
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    m
}

fn check_passes(m: &Machine) {
    let worst = m
        .passes
        .iter()
        .copied()
        .max()
        .expect("no measured main-loop passes");
    println!("SysEx: worst combined pass {worst}/{POLL_INSTRUCTIONS}");
    assert!(worst <= POLL_INSTRUCTIONS, "SysEx combined pass {worst}");
    assert_eq!(
        m.soc.periph.midi_dropped(),
        0,
        "TRS emulator queue dropped bytes"
    );
}

fn chord(m: &mut Machine) {
    for key in CHORD {
        m.key(key, true);
    }
    m.run_ms(50);
    assert!(m.soc.periph.audio.iter().rev().take(512).any(|&s| s != 0));
}

fn full_file() -> Vec<u8> {
    (0..STORE_FILE_MAX)
        .map(|i| (i * 31 + i / 256) as u8)
        .collect()
}

#[test]
fn a_trs_put_commits_a_song_byte_exact_and_bank_one_loads_it() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, true);
    let song = sidou_tests::song_embed::sng();
    put(&mut m, &fw, 0, 0, "MIDI TEST", &song);
    assert_eq!(stored(&mut m, c"MIDI TEST"), Some(song));
    m.midi(&[0xb0, 0, 0, 32, 1, 0xc0, 0]);
    let name = fw.symbol("songs") + ilp32::song_io::name as u32;
    until_ms(
        &mut m,
        "bank one loads the uploaded song stopped",
        15_000,
        |m| {
            (0..9)
                .map(|i| m.soc.read8(name + i))
                .eq(b"MIDI TEST".iter().copied())
                && byte(m, fw.symbol("songs"), ilp32::song_io::state) == SONG_IO_IDLE as u8
        },
    );
    let loaded: Vec<u8> = (0..9).map(|i| m.soc.read8(name + i)).collect();
    assert_eq!(loaded, b"MIDI TEST");
    let sc = fw.symbol("song_transport");
    assert_eq!(byte(&mut m, sc, ilp32::song_ctl::isr_playing), 0);
    assert_eq!(byte(&mut m, sc, ilp32::song_ctl::audition), 0);
    let events = m.soc.read32(sc + ilp32::song_ctl::ev_head as u32);
    m.key(PANEL_BTN_PLAY as u8, true);
    m.run_ms(30);
    m.key(PANEL_BTN_PLAY as u8, false);
    until(&mut m, "PLAY emits uploaded song events", |m| {
        byte(m, sc, ilp32::song_ctl::isr_playing) != 0
            && m.soc.read32(sc + ilp32::song_ctl::ev_head as u32) != events
    });
    let head = m.soc.read32(sc + ilp32::song_ctl::ev_head as u32);
    let flags = sc
        + ilp32::song_ctl::ev as u32
        + (head.wrapping_sub(1) % SONG_EVENT_RING) * ilp32::song_event::SIZE as u32
        + ilp32::song_event::flags as u32;
    assert_ne!(m.soc.read8(flags) & (GT2_EV_ROW | GT2_EV_NOTE) as u8, 0);
    check_passes(&m);
}

#[test]
fn a_pool_drained_put_is_refused_before_any_program_or_claim() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, true);
    chord(&mut m);
    let storage = fw.symbol("store_fm1_store");
    // Debugger-style pool drain: SEC_CLEAN -> SEC_DIRTY, without touching flash.
    let states = storage + ilp32::store::sstate as u32;
    let mut drained = 0;
    for i in 0..STORE_SECTORS {
        let state = m.soc.read8(states + i);
        if state & 7 == 1 {
            m.soc.write8(states + i, state & !7);
            drained += 1;
        }
    }
    assert!(drained >= STORE_POOL_MIN);
    let before = m.soc.flash.mem.clone();
    let counts = fw.symbol("store_fm1_counts");
    let progs = m.soc.read32(counts + PROGS);
    let data = full_file();
    begin(&mut m, 0, 0, "REFUSED", &data);
    let s = fw.symbol("sidou_sysex");
    assert_eq!(byte(&mut m, s, ilp32::sysex::state), 0);
    assert_eq!(byte(&mut m, s, ilp32::sysex::claimed), 0);
    assert_eq!(
        byte(
            &mut m,
            storage,
            ilp32::store::f + ilp32::store_file_writer::phase
        ),
        0
    );
    let toast = fw.symbol("ui") + ilp32::screen::toast as u32;
    let message: Vec<u8> = (0..SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(toast + i))
        .take_while(|&b| b != 0)
        .collect();
    assert_eq!(message, b"STORE BUSY: RESEND");
    send(&mut m, 5, 1, &data[..256]);
    send(&mut m, 6, 2, &[]);
    assert_eq!(stored(&mut m, c"REFUSED"), None);
    assert_eq!(
        m.soc.read32(counts + PROGS),
        progs,
        "refusal programmed flash"
    );
    assert_eq!(m.soc.flash.mem, before, "refusal wrote flash");
    check_passes(&m);
}

#[test]
fn a_trs_instrument_put_relocates_tables_into_the_current_song() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, true);
    let ctl = fw.symbol("song_transport");
    m.key(PANEL_BTN_PLAY as u8, true);
    m.run_ms(30);
    m.key(PANEL_BTN_PLAY as u8, false);
    m.run_ms(30);
    until_ms(
        &mut m,
        "the demo plays before instrument replacement",
        15_000,
        |m| byte(m, ctl, ilp32::song_ctl::isr_playing) != 0,
    );
    m.key(PANEL_BTN_PLAY as u8, true);
    m.run_ms(30);
    m.key(PANEL_BTN_PLAY as u8, false);
    until_ms(
        &mut m,
        "the demo stops before instrument replacement",
        15_000,
        |m| {
            byte(m, ctl, ilp32::song_ctl::state) == SONG_STOPPED as u8
                && byte(m, ctl, ilp32::song_ctl::isr_playing) == 0
                && byte(m, ctl, ilp32::song_ctl::handback) == 0
        },
    );
    let mut song = sidou_tests::song::Song::device();
    // SAFETY: the song owns its region; no player is reading this host fixture.
    unsafe {
        let s = &mut *song.ptr_mut();
        for table in 0..GT2_TABLES as usize {
            s.ltable[table][0] = 0xff;
        }
        gt2_saved_recount(s);
    }
    let region = fw.symbol("__song_region");
    for (i, &b) in song.bytes().iter().enumerate() {
        m.soc.write8(region + i as u32, b);
    }
    let mut data = b"GTI5".to_vec();
    let mut instrument = [0; GT2_INSTR_BYTES as usize];
    instrument[0] = 0x12;
    instrument[2..6].copy_from_slice(&[10, 20, 30, 40]);
    data.extend(instrument);
    data.extend([3, 0xf1, 0x21, 0xff, 41, 0x80, 11]);
    data.extend([2, 0x88, 0xff, 0, 20]);
    data.extend([2, 0x90, 0xff, 0, 0]);
    data.extend([2, 0, 1, 9, 2]);
    begin(&mut m, 1, 2, "", &data);
    upload(&mut m, &fw, &data, |m, _| {
        m.midi(&[0xf2, 4, 0, 0xfb]);
        m.run_ms(5);
    });
    assert_eq!(
        byte(
            &mut m,
            fw.symbol("song_transport"),
            ilp32::song_ctl::isr_playing
        ),
        0
    );
    let instr = region + ilp32::gt2_song::instr as u32 + GT2_INSTR_BYTES;
    let actual: Vec<u8> = (0..GT2_INSTR_BYTES)
        .map(|i| m.soc.read8(instr + i))
        .collect();
    instrument[2..6].copy_from_slice(&[2, 2, 2, 2]);
    assert_eq!(actual, instrument);
    let left: [&[u8]; 4] = [&[0xf1, 0x21, 0xff], &[0x88, 0xff], &[0x90, 0xff], &[0, 1]];
    let right: [&[u8]; 4] = [&[3, 0x80, 3], &[0, 2], &[0, 0], &[9, 2]];
    for table in 0..GT2_TABLES as usize {
        for (field, expected) in [
            (ilp32::gt2_song::ltable, left[table]),
            (ilp32::gt2_song::rtable, right[table]),
        ] {
            let base = region + field as u32 + table as u32 * GT2_TABLE_ROWS;
            assert_eq!(
                m.soc.read8(base),
                if field == ilp32::gt2_song::ltable {
                    0xff
                } else {
                    0
                }
            );
            let actual: Vec<u8> = (0..expected.len())
                .map(|i| m.soc.read8(base + 1 + i as u32))
                .collect();
            assert_eq!(actual, expected, "table {table} field {field}");
        }
    }
    assert_eq!(byte(&mut m, region, ilp32::gt2_song::frozen), 0);
    assert_eq!(
        byte(
            &mut m,
            fw.symbol("song_transport"),
            ilp32::song_ctl::isr_playing
        ),
        0
    );
    assert_eq!(byte(&mut m, ctl, ilp32::song_ctl::state), SONG_IDLE as u8);
    assert_eq!(byte(&mut m, ctl, ilp32::song_ctl::shapes_used), 0);
    check_passes(&m);
}

#[test]
fn a_full_size_trs_put_with_a_chord_never_erases_or_holds() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, true);
    chord(&mut m);
    let counts = fw.symbol("store_fm1_counts");
    let erases = m.soc.read32(counts + ERASES);
    let holds = m.soc.read32(counts + HOLDS);
    let progs = m.soc.read32(counts + PROGS);
    let data = full_file();
    begin(&mut m, 0, 0, "FULL MIDI", &data);
    let worst = upload(&mut m, &fw, &data, |m, _| {
        assert_eq!(m.soc.read32(counts + ERASES), erases);
        assert_eq!(m.soc.read32(counts + HOLDS), holds);
    });
    assert_eq!(stored(&mut m, c"FULL MIDI"), Some(data));
    assert_eq!(m.soc.read32(counts + ERASES), erases);
    assert_eq!(m.soc.read32(counts + HOLDS), holds);
    assert!(m.soc.read32(counts + PROGS) > progs);
    println!("64 KiB windowed TRS: worst storage backpressure {worst}/{PACKET_TIMEOUT_MS} ms");
    check_passes(&m);
}

#[test]
fn a_production_full_size_trs_put_holds_every_program_but_never_erases_or_stays_muted() {
    let Some(fw) = fixture() else { return };
    let mut m = boot(&fw, false);
    chord(&mut m);
    assert_eq!(m.soc.read32(fw.symbol("store_fm1_window")), 0);
    let data = full_file();
    let counts = fw.symbol("store_fm1_counts");
    let progs = m.soc.read32(counts + PROGS);
    let holds = m.soc.read32(counts + HOLDS);
    let audio0 = m.soc.periph.audio.len();
    let want = fw.symbol("hold_want");
    let faded = fw.symbol("hold_faded");
    assert_eq!(m.soc.read8(faded), 0, "audio already held before BEGIN");
    m.snapshot_bytes = vec![faded];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 4 * common::HALF_FRAMES as u32)));
    begin(&mut m, 0, 0, "HELD MIDI", &data);
    ready(&mut m, &fw, 1);
    // TRS's accepted BEGIN is the equivalent of USB's initial ACK: no later erase is allowed.
    let erases = m.soc.read32(counts + ERASES);
    let worst = upload(&mut m, &fw, &data, |m, i| {
        assert_eq!(
            m.soc.read32(counts + ERASES),
            erases,
            "erase after packet {i}"
        );
        until(m, "audio released between TRS packets", |m| {
            m.soc.read8(want) == 0 && m.soc.read8(faded) == 0
        });
        // Two DMA halves must play after release, not a whole second of sender delay.
        m.run_ms(6);
        assert!(
            m.soc
                .periph
                .audio
                .iter()
                .rev()
                .take(2 * common::HALF_FRAMES)
                .any(|&s| s != 0),
            "held chord stayed muted between packets {i} and {}",
            i + 1
        );
        if i == 0 {
            m.snapshot_buffer(None);
        }
    });
    until(&mut m, "audio released after END", |m| {
        m.soc.read8(want) == 0 && m.soc.read8(faded) == 0
    });
    m.run_ms(6);
    assert!(
        m.soc
            .periph
            .audio
            .iter()
            .rev()
            .take(2 * common::HALF_FRAMES)
            .any(|&s| s != 0),
        "held chord stayed muted after END"
    );
    assert_eq!(stored(&mut m, c"HELD MIDI"), Some(data));
    assert_eq!(m.soc.read32(counts + ERASES), erases);
    let programs = m.soc.read32(counts + PROGS) - progs;
    let forced_holds = m.soc.read32(counts + HOLDS) - holds;
    assert!(programs > 0, "no file programs ran");
    assert_eq!(
        programs, forced_holds,
        "every production program must be held"
    );
    // Same explicit-hold silence assertion as store.rs::no_erase_while_it_plays.
    let mut run = 0;
    let longest = m.soc.periph.audio[audio0..].iter().fold(0, |best, &s| {
        run = if s == 0 { run + 1 } else { 0 };
        best.max(run)
    });
    assert!(
        longest >= 2 * 2 * common::HALF_FRAMES,
        "the hold's silence: {longest} samples"
    );
    let mut previous = 0;
    let (mut down, mut up) = (0, 0);
    for snapshot in &m.buffer_snapshots {
        let current = snapshot.bytes[0];
        if current != previous {
            let (halves, remainder) = snapshot.words.as_chunks::<{ 2 * common::HALF_FRAMES }>();
            assert!(remainder.is_empty());
            let ramp = halves.iter().any(|half| {
                half.iter().any(|&s| s != 0)
                    && if current != 0 {
                        half[half.len() - 2..] == [0, 0]
                    } else {
                        half[..2] == [0, 0]
                    }
            });
            assert!(
                ramp,
                "hold transition {previous}->{current} has no audio ramp endpoint"
            );
            if current != 0 {
                down += 1;
            } else {
                up += 1;
            }
        }
        previous = current;
    }
    assert!(down > 0 && up > 0, "first DATA never faded out and back in");
    println!(
        "64 KiB production TRS: {programs} held programs, worst DATA backpressure \
         {worst}/{PACKET_TIMEOUT_MS} ms, {down}/{up} first-packet ramps"
    );
    check_passes(&m);
}
