// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: MIDI into the live instrument through fm1-emu's TRS jack (UART1 at 31250
//! baud), watched on the audio output and on the device's SID state in emulated RAM.
//!
//! USB MIDI is not tested here: fm1-emu does not emulate the USB controller (its registers
//! are plain storage, so the device sees no host and leaves USB off). The USB packet
//! conversion and the ring are host-tested (sidou-tests, `tests/app/midi_*.rs`).

mod common;

use std::mem::{offset_of, size_of};

use common::{build_dir, peak, pitch, power_above, record, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::{live_freq_table, reg_ring_chips, sid, sid_env, sid_osc};

/// Past the LCD's reset delays and the first audio blocks.
const BOOT_MS: u64 = 500;
const C4: u8 = 60;
const C4_HZ: f64 = 261.626;
const CC_CUTOFF: u8 = 74;
/// app/live.c's presets 0 (SAW LEAD) and 1 (PWM BASS): waveform bits and cutoff.
const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const PWM_BASS_CUTOFF: u32 = 700;
const SAW_LEAD_CUTOFF: u32 = 1200;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.run_ms(BOOT_MS);
    m
}

/// The rendered chip's state, read from emulated RAM (platform/fm1/audio.c `chips`). `sid`
/// and `reg_ring_chips` have no pointers, so the host build's layout is the device's.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Voice1 {
    control: u8,
    freq: u32,
    env_gate: u8,
}

fn active_sid(m: &mut Machine, fw: &Firmware) -> u32 {
    let base = fw.symbol("chips");
    let active = m
        .soc
        .read8(base + offset_of!(reg_ring_chips, active) as u32);
    base + (offset_of!(reg_ring_chips, chip) + usize::from(active) * size_of::<sid>()) as u32
}

fn voice1(m: &mut Machine, fw: &Firmware) -> Voice1 {
    let s = active_sid(m, fw);
    let osc = s + offset_of!(sid, osc) as u32;
    let env = s + offset_of!(sid, env) as u32;
    Voice1 {
        control: m.soc.read8(osc + offset_of!(sid_osc, control) as u32),
        freq: m.soc.read32(osc + offset_of!(sid_osc, freq) as u32),
        env_gate: m.soc.read8(env + offset_of!(sid_env, gate) as u32),
    }
}

fn cutoff(m: &mut Machine, fw: &Firmware) -> u32 {
    let s = active_sid(m, fw);
    m.soc.read32(s + offset_of!(sid, fc) as u32)
}

/// platform/fm1/midi.c `midi_stats`: six uint32_t.
#[derive(Debug)]
struct Stats {
    entries: u32,
    lost: u32,
    restarts: u32,
    max_wait_us: f64,
    usb_polls: u32,
    uart_polls: u32,
}

fn stats(m: &mut Machine, fw: &Firmware) -> Stats {
    let base = fw.symbol("midi_stats");
    let mut w = [0u32; 6];
    for (i, v) in w.iter_mut().enumerate() {
        *v = m.soc.read32(base + 4 * i as u32);
    }
    Stats {
        entries: w[0],
        lost: w[1],
        restarts: w[2],
        max_wait_us: f64::from(w[3]) / 24.0,
        usb_polls: w[4],
        uart_polls: w[5],
    }
}

#[test]
fn a_trs_note_on_plays_c4_and_the_note_off_releases_it() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    let quiet = record(&mut m, 100);
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(100);
    let held = voice1(&mut m, &fw);
    let x = record(&mut m, 300);
    let f0 = pitch(&x);
    m.midi(&[0x80, C4, 0x00]);
    m.run_ms(20);
    let released = voice1(&mut m, &fw);
    let tail = record(&mut m, 30);
    m.run_ms(800);
    let after = record(&mut m, 100);
    let s = stats(&mut m, &fw);
    println!(
        "C4 over TRS: {f0:.2} Hz, peak {} held, {} just after the note-off, {} 0.8 s later; \
         voice 1 {held:?} -> {released:?}; {s:?}",
        peak(&x),
        peak(&tail),
        peak(&after)
    );
    assert!(peak(&quiet) < 500.0, "sound before the note-on");
    assert_eq!(held.control & 1, 1, "voice 1 is not gated");
    assert_eq!(
        held.freq,
        u32::from(unsafe { live_freq_table }[usize::from(C4)])
    );
    assert!(peak(&x) > 2000.0, "the note is silent");
    assert!(
        (f0 / C4_HZ - 1.0).abs() < 0.005,
        "C4 plays {f0:.2} Hz, expected {C4_HZ} Hz"
    );
    assert_eq!(
        (released.control & 1, released.env_gate),
        (0, 0),
        "the note-off left the gate on"
    );
    assert!(
        peak(&tail) > 500.0,
        "the note stopped dead instead of releasing"
    );
    assert!(peak(&after) < 500.0, "the note still sounds after release");
    assert_eq!((s.entries, s.lost, s.restarts), (6, 0, 0));
    assert!(
        s.max_wait_us < 2000.0,
        "a byte waited {:.0} us in the ring",
        s.max_wait_us
    );
}

#[test]
fn cc74_moves_the_cutoff_and_takes_the_highs_out() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(200);
    let fc_before = cutoff(&mut m, &fw);
    let before = power_above(&record(&mut m, 200), 2000.0);
    m.midi(&[0xB0, CC_CUTOFF, 0]);
    m.run_ms(100);
    let fc_low = cutoff(&mut m, &fw);
    let after = power_above(&record(&mut m, 200), 2000.0);
    m.midi(&[0xB0, CC_CUTOFF, 127]);
    m.run_ms(20);
    let fc_high = cutoff(&mut m, &fw);
    println!(
        "CC 74: cutoff {fc_before} -> {fc_low} -> {fc_high}; power above 2 kHz {before:.4} -> {after:.4}"
    );
    assert_eq!(fc_before, SAW_LEAD_CUTOFF);
    assert_eq!(fc_low, 0);
    assert_eq!(fc_high, 2047);
    assert!(
        after < before / 4.0,
        "CC 74 at 0 left the highs: {before:.4} -> {after:.4} of the power above 2 kHz"
    );
}

#[test]
fn program_change_switches_the_preset() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    let saw = (voice1(&mut m, &fw).control, cutoff(&mut m, &fw));
    m.midi(&[0xC0, 1]);
    m.run_ms(20);
    let pulse = (voice1(&mut m, &fw).control, cutoff(&mut m, &fw));
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(100);
    let f0 = pitch(&record(&mut m, 300));
    m.midi(&[0x80, C4, 0, 0xC0, 0]);
    m.run_ms(20);
    let back = (voice1(&mut m, &fw).control, cutoff(&mut m, &fw));
    println!(
        "program change: preset 0 {saw:02x?}, 1 {pulse:02x?}, 0 {back:02x?}; C4 on preset 1 {f0:.2} Hz"
    );
    assert_eq!(saw, (SAW, SAW_LEAD_CUTOFF));
    assert_eq!(pulse, (PULSE, PWM_BASS_CUTOFF));
    assert!(
        (f0 / C4_HZ - 1.0).abs() < 0.005,
        "C4 on PWM BASS plays {f0:.2} Hz"
    );
    assert_eq!(back, (SAW, SAW_LEAD_CUTOFF));
}

/// Instructions per second of emulated time over `ms`: in interrupts and in the main loop.
fn rates(m: &mut Machine, ms: u64) -> (f64, f64) {
    let (isr, main) = (m.isr_steps, m.main_steps);
    m.run_ms(ms);
    let per_s = 1000.0 / ms as f64;
    (
        (m.isr_steps - isr) as f64 * per_s,
        (m.main_steps - main) as f64 * per_s,
    )
}

/// Needs no firmware symbol, so it also measures a build from before MIDI input for the
/// difference.
#[test]
fn midi_input_costs_little() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(1500); // past a USB retry (once a second while no host answers)
    let (isr_idle, main_idle) = rates(&mut m, 2000);
    // The jack at full speed for 2 s: a note-on and its note-off every 1.92 ms.
    let mut stream = Vec::new();
    for i in 0..1042u32 {
        let note = 48 + (i % 24) as u8;
        stream.extend([0x90, note, 0x64, 0x80, note, 0x00]);
    }
    m.midi(&stream);
    let (isr_busy, main_busy) = rates(&mut m, 2000);
    let report = serde_json::json!({
        "isr_instructions_per_s_idle": isr_idle,
        "main_instructions_per_s_idle": main_idle,
        "isr_instructions_per_s_trs_full_speed": isr_busy,
        "main_instructions_per_s_trs_full_speed": main_busy,
        "midi_bytes_left_in_the_jack": m.soc.periph.midi_queued(),
    });
    println!("{report}");
    assert!(
        main_idle < 5e6 && main_busy < 5e6,
        "the main loop runs {:.2}M/s idle, {:.2}M/s with TRS MIDI at full speed",
        main_idle / 1e6,
        main_busy / 1e6
    );
    assert!(
        isr_busy - isr_idle < 2e6,
        "TRS MIDI at full speed adds {:.2}M interrupt instructions/s",
        (isr_busy - isr_idle) / 1e6
    );
}

#[test]
fn a_full_speed_stream_loses_nothing() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    let mut stream = Vec::new();
    for i in 0..500u32 {
        let note = 48 + (i % 24) as u8;
        stream.extend([0x90, note, 0x64, 0x80, note, 0x00]);
    }
    m.midi(&stream);
    m.run_ms(1200);
    let s = stats(&mut m, &fw);
    let v = voice1(&mut m, &fw);
    println!("{} bytes at 31250 baud: {s:?}; voice 1 {v:?}", stream.len());
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    assert_eq!(s.entries, stream.len() as u32);
    assert_eq!((s.lost, s.restarts), (0, 0));
    assert!(s.uart_polls > 2000, "{} UART polls in 1.7 s", s.uart_polls);
    assert_eq!(s.usb_polls, 0, "USB polled with no host (fm1-emu has none)");
    assert_eq!(v.control & 1, 0, "a note is left on");
    assert!(
        s.max_wait_us < 2000.0,
        "a byte waited {:.0} us in the ring",
        s.max_wait_us
    );
}
