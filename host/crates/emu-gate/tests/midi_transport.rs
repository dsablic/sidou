// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: MIDI clock and transport for the song (M2e Task 2, app/midi_sync.h) over TRS
//! in fm1-emu: SIDOU's first song started, stopped, continued and sought from a 120 BPM clock,
//! its rows against the clock's 16ths, every main-loop pass within the poll allowance.
//! The device's own TRS poll (2 kHz) gives the clock its jitter: the bytes go in on time.
//! One case goes over USB instead (common::usb_host): song position and continue reach SIDOU's
//! parser through usb.c's raw MIDI-in hook (platform/fm1/VENDORED.md), not its filter.

mod common;

use fm1_core::Bus;

use common::usb_host::UsbHost;
use common::{build_dir, Firmware, Machine};
use sidou_sys::ilp32;
use sidou_tests::song::Song;
use sidou_tests::song_embed;
use sidou_tests::transport::Transport;

const BPM: f64 = 120.0;
/// Emulated CPU cycles a sample (240 MHz, 24 MHz / 544 samples a second).
const CYCLES_PER_SAMPLE: f64 = 240e6 / (24e6 / 544.0);
const FS: f64 = 24e6 / 544.0;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m.run_ms(300);
    m
}

/// A host on the cable that has the device configured (address 7, configuration 1).
fn booted_on_usb(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.usb = Some(UsbHost::default());
    common::run_past_boot(&mut m, fw);
    m.usb_frames(20);
    let setup = |m: &mut Machine, pkt: [u8; 8]| {
        m.usb.as_mut().unwrap().setup(&mut m.soc, pkt);
        m.usb_frames(3);
    };
    m.usb.as_mut().unwrap().bus_reset();
    m.usb_frames(3);
    setup(&mut m, [0x00, 0x05, 0x07, 0, 0, 0, 0, 0]);
    setup(&mut m, [0x00, 0x09, 0x01, 0, 0, 0, 0, 0]);
    let usb = fw.symbol("usb");
    assert_eq!(m.soc.read8(usb + 1), 1, "not configured");
    m.usb_frames(300);
    m
}

/// One MIDI message as a USB-MIDI event packet on EP1 OUT (cable 0), retried while the
/// device NAKs.
fn usb_midi(m: &mut Machine, bytes: &[u8]) {
    let cin = match bytes[0] {
        0xF2 => 0x3,
        0xF3 => 0x2,
        _ => 0xF,
    };
    let mut pkt = [cin, 0, 0, 0];
    pkt[1..=bytes.len()].copy_from_slice(bytes);
    loop {
        let usb = m.usb.as_mut().expect("a host");
        if usb.out1(&mut m.soc, &pkt) {
            break;
        }
        m.usb_frames(1);
    }
}

/// A MIDI message over TRS, or over USB when a host is on the cable.
fn midi(m: &mut Machine, bytes: &[u8]) {
    if m.usb.is_some() {
        usb_midi(m, bytes);
    } else {
        m.midi(bytes);
    }
}

/// The first song's calls a row, from the host's song control (the same song and player).
fn row_calls() -> u32 {
    let mut song = Song::device();
    song.read(&song_embed::sng(), 4).expect("reads");
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    assert!(t.run_until(400, |t| t.calls() > 0));
    let tempo = t.sc.player.chn[0].tempo;
    assert!(tempo >= 2, "no funktempo in the first song");
    u32::from(tempo) + 1
}

fn song_field(m: &mut Machine, fw: &Firmware, offset: usize) -> u32 {
    m.soc.read32(fw.symbol("song_transport") + offset as u32)
}

/// Runs `ms` milliseconds a millisecond at a time with the clock's bytes at their times
/// (the cycle each went in at, for the comparison), recording each call's (number, sample).
struct Run {
    clocks: Vec<u64>,
    calls: Vec<(u32, u32)>,
}

fn run_clocked(
    m: &mut Machine,
    fw: &Firmware,
    ms: u64,
    mut extra: impl FnMut(&mut Machine, u64) -> bool,
) -> Run {
    let per = 60.0 / BPM / 24.0 * m.soc.periph.hz as f64; // cycles a clock
    let start = m.soc.periph.cycles;
    let mut next = start as f64;
    let mut run = Run {
        clocks: Vec::new(),
        calls: Vec::new(),
    };
    let mut last = song_field(m, fw, ilp32::song_ctl::calls);
    for t in 0..ms {
        while (next as u64) <= m.soc.periph.cycles {
            midi(m, &[0xF8]);
            run.clocks.push(m.soc.periph.cycles);
            next += per;
        }
        if extra(m, t) {
            // a transport byte went in: the clock's next one after it
        }
        if m.usb.is_some() {
            m.usb_frames(1);
        } else {
            m.run_ms(1);
        }
        let calls = song_field(m, fw, ilp32::song_ctl::calls);
        if calls != last {
            let s = song_field(m, fw, ilp32::song_ctl::call_sample);
            run.calls.push((calls - 1, s));
            last = calls;
        }
    }
    let _ = start;
    run
}

/// Each row's error from its 16th, less the run's mean (the constant delay, the jack's byte
/// time, the emulator's audio offset), in ms, for rows from `first_row` on. The 16th of row k
/// is the clock 6k after the clock `clock0` (the start's).
fn row_deviation(run: &Run, row: u32, clock0: usize, first_row: u32) -> (f64, usize) {
    let mut errs = Vec::new();
    for &(call, sample) in &run.calls {
        if call % row != 0 || call / row < first_row {
            continue;
        }
        let i = clock0 + (call / row * 6) as usize;
        if i >= run.clocks.len() {
            continue;
        }
        let at = run.clocks[i] as f64 / CYCLES_PER_SAMPLE;
        errs.push(f64::from(sample) - at);
    }
    // the clock's samples and the player's differ by a constant (wraps cancel too)
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let worst = errs.iter().fold(0.0f64, |a, &e| a.max((e - mean).abs()));
    (worst / FS * 1e3, errs.len())
}

#[test]
fn rows_land_on_a_120_bpm_clock_over_trs() {
    let Some(fw) = fixture("the MIDI transport test") else {
        return;
    };
    let row = row_calls();
    let mut m = booted(&fw);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let beat_ms = (60_000.0 / BPM) as u64;
    // two beats of clock, then a start just before a clock, then eight beats
    let mut start_clock = None;
    let run = run_clocked(&mut m, &fw, beat_ms * 10, |m, t| {
        if t == 2 * beat_ms - 5 {
            m.midi(&[0xFA]);
            start_clock = Some(());
            return true;
        }
        false
    });
    assert!(start_clock.is_some());
    // the start went in 5 ms before the clock at 2 beats: clock 48 is the song's first
    let clock0 = 48;
    assert!(run.calls.len() > 100, "{} calls", run.calls.len());
    assert_eq!(run.calls[0].0, 0, "from the song's start");
    let (worst, n) = row_deviation(&run, row, clock0, 2 * 4);
    let pass = m.passes.iter().copied().max().unwrap_or(0);
    println!(
        "{n} rows from two beats after the start: worst 16th phase error {worst:.3} ms; busiest pass {pass}"
    );
    assert!(n > 20);
    assert!(worst <= 1.0, "a row {worst:.3} ms off its 16th");
    assert!(pass <= common::POLL_INSTRUCTIONS);
}

#[test]
fn stop_song_position_and_continue_over_trs() {
    let Some(fw) = fixture("the MIDI transport test") else {
        return;
    };
    let mut m = booted(&fw);
    stop_song_position_and_continue(&mut m, &fw);
}

/// The same over USB: song position (a system common message) and the real-time bytes go
/// through usb.c's raw MIDI-in hook to SIDOU's parser, as on TRS.
#[test]
fn stop_song_position_and_continue_over_usb() {
    let Some(fw) = fixture("the MIDI transport test") else {
        return;
    };
    let mut m = booted_on_usb(&fw);
    stop_song_position_and_continue(&mut m, &fw);
}

fn stop_song_position_and_continue(m: &mut Machine, fw: &Firmware) {
    let row = row_calls();
    let beat_ms = (60_000.0 / BPM) as u64;
    let run = run_clocked(m, fw, beat_ms * 4, |m, t| {
        if t == beat_ms - 5 {
            midi(m, &[0xFA]);
        }
        if t == 3 * beat_ms - 5 {
            midi(m, &[0xFC]);
        }
        false
    });
    let stopped_at = run.calls.last().unwrap().0;
    assert!(
        stopped_at > 0 && stopped_at < 3 * 4 * row,
        "stopped at call {stopped_at}"
    );
    // bar 3: 32 16ths, then continue
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let run = run_clocked(m, fw, beat_ms * 4, |m, t| {
        if t == 10 {
            midi(m, &[0xF2, 32, 0]);
        }
        if t == beat_ms - 5 {
            midi(m, &[0xFB]);
        }
        false
    });
    let first = run.calls.first().expect("it played on").0;
    assert_eq!(first, 32 * row, "the continue's first call: row 32");
    let pass = m.passes.iter().copied().max().unwrap_or(0);
    println!("after SPP to bar 3: first call {first}; busiest pass {pass} (the seek's)");
    assert!(pass <= common::POLL_INSTRUCTIONS);
}
