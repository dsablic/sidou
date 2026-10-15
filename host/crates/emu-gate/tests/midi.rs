// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: MIDI into the live instrument through fm1-emu's TRS jack (UART1 at 31250
//! baud), watched on the audio output and on the device's SID state in emulated RAM.
//!
//! USB MIDI in is not tested here: fm1-emu does not emulate the USB controller (its registers
//! are plain storage, so the device sees no host and leaves USB off). The USB packet
//! conversion and the ring are host-tested (sidou-tests, `tests/app/midi_*.rs`). MIDI out is
//! checked where it leaves the main loop: the IN endpoint's ring in usb/usb.c, with the test
//! playing a host that configured the device (`usb.config`) but never reads (USB stays off,
//! so nothing drains the ring).

mod common;

use common::{build_dir, peak, pitch, power_above, record, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::{ilp32, live_freq_at, live_freq_table, SID_MODEL_8580};

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
    let base = fw.chips();
    let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

fn voice1(m: &mut Machine, fw: &Firmware) -> Voice1 {
    let s = active_sid(m, fw);
    let osc = s + ilp32::sid::osc as u32;
    let env = s + ilp32::sid::env as u32;
    Voice1 {
        control: m.soc.read8(osc + ilp32::sid_osc::control as u32),
        freq: m.soc.read32(osc + ilp32::sid_osc::freq as u32),
        env_gate: m.soc.read8(env + ilp32::sid_env::gate as u32),
    }
}

/// Oscillator v's frequency register (0..2) on the rendered chip.
fn osc_freq(m: &mut Machine, fw: &Firmware, v: u32) -> u32 {
    let s = active_sid(m, fw);
    let osc = s + (ilp32::sid::osc + v as usize * ilp32::sid_osc::SIZE) as u32;
    m.soc.read32(osc + ilp32::sid_osc::freq as u32)
}

fn cutoff(m: &mut Machine, fw: &Firmware) -> u32 {
    let s = active_sid(m, fw);
    m.soc.read32(s + ilp32::sid::fc as u32)
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

#[test]
fn an_nrpn_sets_the_cutoff_at_full_resolution() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    let fc_before = cutoff(&mut m, &fw);
    // NRPN 0/0 (docs/reference/midi.md): 1234 = 9 * 128 + 82, with running status.
    m.midi(&[0xB0, 99, 0, 98, 0, 6, 9, 38, 82]);
    m.run_ms(20);
    let fc_set = cutoff(&mut m, &fw);
    m.midi(&[0xB0, 96, 0]); // increment
    m.run_ms(10);
    let fc_inc = cutoff(&mut m, &fw);
    m.midi(&[97, 0, 97, 0]); // decrement twice
    m.run_ms(10);
    let fc_dec = cutoff(&mut m, &fw);
    m.midi(&[38, 0]); // an LSB alone: the current value's MSB
    m.run_ms(10);
    let fc_lsb = cutoff(&mut m, &fw);
    m.midi(&[99, 127, 98, 127, 6, 1]); // the null NRPN: data entry goes nowhere
    m.run_ms(10);
    let fc_null = cutoff(&mut m, &fw);
    let s = stats(&mut m, &fw);
    println!(
        "NRPN cutoff: {fc_before} -> {fc_set}, +1 {fc_inc}, -2 {fc_dec}, LSB 0 {fc_lsb}, \
         after the null NRPN {fc_null}; {s:?}"
    );
    assert_eq!(fc_before, SAW_LEAD_CUTOFF);
    assert_eq!(fc_set, 1234, "11 bits through MSB and LSB");
    assert_eq!(fc_inc, 1235);
    assert_eq!(fc_dec, 1233);
    assert_eq!(fc_lsb, 9 << 7);
    assert_eq!(fc_null, 9 << 7);
    assert_eq!((s.lost, s.restarts), (0, 0));
}

/// CC 25 (docs/reference/midi.md): the chip model, off below 64 (6581), on from 64 (8580).
const CC_MODEL: u8 = 25;

#[test]
fn a_chip_model_cc_flood_neither_stalls_the_main_loop_nor_loses_input() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("sid_init"));
    m.run_ms(BOOT_MS);
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(50);
    let boot_inits = m.watched_calls.len();
    // 201 toggles with running status at full speed, ending on the 8580: 128 ms of input.
    let mut stream = vec![0xB0];
    for i in 0..201u32 {
        stream.extend([CC_MODEL, if i % 2 == 0 { 64 } else { 63 }]);
    }
    let start = m.ms();
    m.midi(&stream);
    m.run_ms(300);
    let span_ms = m.ms() - start;
    let s = stats(&mut m, &fw);
    let inits = m.watched_calls.len() - boot_inits;
    let chip = active_sid(&mut m, &fw);
    let model = m.soc.read8(chip + ilp32::sid::model as u32);
    let v = voice1(&mut m, &fw);
    println!(
        "{} CC 25 toggles in {} bytes: {inits} chip re-inits in {span_ms} ms, model {model}; \
         voice 1 {v:?}; {s:?}",
        201,
        stream.len()
    );
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    assert_eq!((s.lost, s.restarts), (0, 0), "the MIDI ring overflowed");
    assert_eq!(s.entries, stream.len() as u32 + 3);
    assert!(
        s.max_wait_us < 5000.0,
        "a byte waited {:.0} us in the ring: the main loop stalled",
        s.max_wait_us
    );
    // live_tick applies the model at most once per 10 ms tick.
    assert!(
        inits as u64 <= span_ms / 10 + 1,
        "{inits} chip re-inits in {span_ms} ms"
    );
    assert_eq!(model, SID_MODEL_8580 as u8, "the last request did not win");
    assert_eq!(v.control & 1, 1, "the held note was released");
}

/// A 14-bit pitch bend: status E0, LSB, MSB.
fn bend(value: u16) -> [u8; 3] {
    [0xE0, (value & 0x7f) as u8, (value >> 7) as u8]
}

/// The frequency register live plays for a note bent by `value` at the default +-2 semitones.
fn bent(note: u8, value: u16) -> u32 {
    u32::from(unsafe { live_freq_at(note, (i32::from(value) - 8192) * 2) })
}

fn semitones(ratio: f64) -> f64 {
    12.0 * ratio.log2()
}

#[test]
fn a_pitch_bend_over_trs_shifts_the_measured_pitch() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(100);
    let f0 = pitch(&record(&mut m, 300));
    let mut steps = Vec::new();
    for value in [16383u16, 0, 8192] {
        m.midi(&bend(value));
        m.run_ms(30); // a byte per 320 us, then the next 10 ms tick
        let reg = voice1(&mut m, &fw).freq;
        let f = pitch(&record(&mut m, 300));
        steps.push((value, reg, f));
    }
    let s = stats(&mut m, &fw);
    println!("C4 {f0:.2} Hz; bends (value, register, Hz): {steps:?}; {s:?}");
    assert!((f0 / C4_HZ - 1.0).abs() < 0.005, "C4 plays {f0:.2} Hz");
    for (value, reg, f) in steps {
        assert_eq!(reg, bent(C4, value), "bend {value}");
        let want = 2.0 * (f64::from(value) - 8192.0) / 8192.0;
        let got = semitones(f / C4_HZ);
        assert!(
            (got - want).abs() < 0.05,
            "bend {value}: {f:.2} Hz is {got:+.3} semitones from C4, expected {want:+.3}"
        );
    }
    assert_eq!((s.lost, s.restarts), (0, 0));
}

/// The sync lead: voice 3 sets the pitch, voice 1 runs 2.75 times higher, hard-synced to it.
/// A bend moves both, so the ratio (the timbre) holds and the pitch follows the bend.
#[test]
fn the_sync_lead_bends_with_its_ratio_kept() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0xC0, 2, 0x90, C4, 0x64]);
    m.run_ms(100);
    let f0 = pitch(&record(&mut m, 300));
    let before = (osc_freq(&mut m, &fw, 0), osc_freq(&mut m, &fw, 2));
    m.midi(&bend(16383));
    m.run_ms(30);
    let after = (osc_freq(&mut m, &fw, 0), osc_freq(&mut m, &fw, 2));
    let f1 = pitch(&record(&mut m, 300));
    println!(
        "sync lead: voices 1 and 3 {before:?} -> {after:?}; {f0:.2} Hz -> {f1:.2} Hz, \
         {:+.3} semitones",
        semitones(f1 / f0)
    );
    assert_eq!(
        before.1,
        u32::from(unsafe { live_freq_table }[usize::from(C4)])
    );
    assert_eq!(after.1, bent(C4, 16383));
    for (slave, master) in [before, after] {
        assert_eq!(slave, (master * 704) >> 8, "the ratio is 2.75");
    }
    assert!(
        (semitones(f1 / f0) - 2.0).abs() < 0.05,
        "the sync lead bent {:+.3} semitones",
        semitones(f1 / f0)
    );
}

/// A bend sweep at the jack's full speed: every message is taken, none waits long, the
/// frequency is written at most once a tick, and the last value wins.
#[test]
fn a_pitch_bend_flood_neither_stalls_the_main_loop_nor_loses_input() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(1500); // past a USB retry (once a second while no host answers)
    let (isr_idle, main_idle) = rates(&mut m, 1000);
    // 1500 bends with running status, 0 up to 16383: 3001 bytes, 0.96 s of input.
    let mut stream = vec![0xE0];
    for i in 0..1500u32 {
        let v = (i * 16383 / 1499) as u16;
        stream.extend([(v & 0x7f) as u8, (v >> 7) as u8]);
    }
    let before = stats(&mut m, &fw).entries;
    m.midi(&stream);
    let (isr_busy, main_busy) = rates(&mut m, 900);
    m.run_ms(200);
    let s = stats(&mut m, &fw);
    let v = voice1(&mut m, &fw);
    let report = serde_json::json!({
        "bends": 1500,
        "isr_instructions_per_s_idle": isr_idle,
        "main_instructions_per_s_idle": main_idle,
        "isr_instructions_per_s_bend_flood": isr_busy,
        "main_instructions_per_s_bend_flood": main_busy,
        "max_wait_us": s.max_wait_us,
    });
    println!("{report}; voice 1 {v:?}; {s:?}");
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    assert_eq!((s.lost, s.restarts), (0, 0), "the MIDI ring overflowed");
    assert_eq!(s.entries - before, stream.len() as u32);
    assert!(
        s.max_wait_us < 2000.0,
        "a byte waited {:.0} us in the ring: the main loop stalled",
        s.max_wait_us
    );
    assert_eq!(v.freq, bent(C4, 16383), "the last bend did not win");
    assert_eq!(v.control & 1, 1, "the held note was released");
    assert!(
        main_busy < 5e6,
        "the main loop runs {:.2}M/s during the flood",
        main_busy / 1e6
    );
    assert!(
        isr_busy - isr_idle < 2e6,
        "the flood adds {:.2}M interrupt instructions/s",
        (isr_busy - isr_idle) / 1e6
    );
}

/// Program changes at the jack's full speed, with running status: one a byte (0.32 ms), each
/// writing every register (25). The main loop leaves the register ring room for its own worst
/// pass, so the MIDI drain takes only what fits after it; the ring must still keep up, or the
/// MIDI ring overflows, the parsers restart and the notes are released.
#[test]
fn a_program_change_flood_neither_stalls_the_main_loop_nor_loses_input() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted(&fw);
    // 1000 program changes alternating presets 0 and 1, ending on 1: 1001 bytes, 320 ms.
    let mut stream = vec![0xC0];
    for i in 0..1000u32 {
        stream.push((i % 2) as u8);
    }
    let before = stats(&mut m, &fw).entries;
    m.midi(&stream);
    m.run_ms(500);
    let s = stats(&mut m, &fw);
    let after = (voice1(&mut m, &fw).control, cutoff(&mut m, &fw));
    println!(
        "{} program changes in {} bytes: voice 1 control {:02x}, cutoff {}; {s:?}",
        1000,
        stream.len(),
        after.0,
        after.1
    );
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    assert_eq!((s.lost, s.restarts), (0, 0), "the MIDI ring overflowed");
    assert_eq!(s.entries - before, stream.len() as u32);
    assert!(
        s.max_wait_us < 10_000.0,
        "a byte waited {:.0} us in the ring: the main loop fell behind",
        s.max_wait_us
    );
    assert_eq!(
        after,
        (PULSE, PWM_BASS_CUTOFF),
        "the last program change did not win"
    );
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

// --- MIDI out ---

/// usb/usb.c's MIDI out ring, `midi_out_q[MQ]`, and its write index.
const MQ: u32 = 64;
/// Felucca's ids: the FX button, K1 and K4.
const FX_BUTTON: u8 = 2;
const K1: u8 = 2;
const K4: u8 = 5;

/// A host has configured the device: `usb.config`, the second byte of usb.c's `usb` (after
/// `up`). USB itself stays off in fm1-emu, so the ring is never sent.
fn attach_host(m: &mut Machine, fw: &Firmware) {
    m.soc.write8(fw.symbol("usb") + 1, 1);
}

/// The packets queued for the host since write index `from`, and the write index now.
fn queued(m: &mut Machine, fw: &Firmware, from: u32) -> (Vec<u32>, u32) {
    let (q, w) = (fw.symbol("midi_out_q"), m.soc.read32(fw.symbol("mo_w")));
    assert!(w - from <= MQ, "the ring wrapped");
    let p = (from..w).map(|i| m.soc.read32(q + 4 * (i % MQ))).collect();
    (p, w)
}

/// MIDI out's counters (app/midi_out.h): packets, deferred, dropped.
fn out_counters(m: &mut Machine, fw: &Firmware) -> (u32, u32, u32) {
    let base = fw.symbol("midi_tx");
    (
        m.soc.read32(base + ilp32::midi_out::packets as u32),
        m.soc.read32(base + ilp32::midi_out::deferred as u32),
        m.soc.read32(base + ilp32::midi_out::dropped as u32),
    )
}

fn cc_packet(cc: u8, value: u8) -> u32 {
    u32::from_le_bytes([0x0B, 0xB0, cc, value])
}

fn cutoff_cc(value: u32) -> u8 {
    unsafe {
        sidou_sys::midi_map_to_cc(
            sidou_sys::midi_map_for_param(sidou_sys::LIVE_PARAM_CUTOFF),
            value,
        )
    }
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

#[test]
fn the_panel_goes_out_over_usb_and_midi_in_is_not_echoed() {
    let Some(fw) = fixture("the MIDI out test") else {
        return;
    };
    let mut m = booted(&fw);
    // No host: a knob's change is dropped, nothing is queued.
    m.turn(K1, 1);
    m.run_ms(50);
    assert_eq!(queued(&mut m, &fw, 0), (vec![], 0));
    assert_eq!(out_counters(&mut m, &fw), (0, 0, 1));

    attach_host(&mut m, &fw);
    m.run_ms(50);
    let (idle, w) = queued(&mut m, &fw, 0);
    assert!(idle.is_empty(), "nothing changed: nothing to send");
    // K1: CC 74, at most one a tick, ending on the latest value (fm1-emu delivers the three
    // detents over a few key-scan frames).
    m.turn(K1, 3);
    m.run_ms(50);
    let c = cutoff(&mut m, &fw);
    assert_eq!(c, SAW_LEAD_CUTOFF + 4 * 16);
    let (k1, w) = queued(&mut m, &fw, w);
    assert!((1..=3).contains(&k1.len()), "{k1:08x?}");
    assert_eq!(k1.last(), Some(&cc_packet(CC_CUTOFF, cutoff_cc(c))));
    let values: Vec<u8> = k1.iter().map(|&x| (x >> 24) as u8).collect();
    assert!(values.windows(2).all(|v| v[0] < v[1]), "{values:?}");
    assert!(k1
        .iter()
        .all(|&x| x & 0x00ff_ffff == cc_packet(CC_CUTOFF, 0)));
    // The cutoff from TRS MIDI: applied, not echoed.
    m.midi(&[0xB0, CC_CUTOFF, 10]);
    m.run_ms(50);
    assert_eq!(cutoff(&mut m, &fw), (10 * 2047 + 63) / 127);
    let (p, w) = queued(&mut m, &fw, w);
    assert!(p.is_empty(), "MIDI in echoed: {p:08x?}");
    // The FX button: the chip model on CC 25. K4: a program change, without the parameters
    // the preset loads.
    press(&mut m, FX_BUTTON);
    let (p, w) = queued(&mut m, &fw, w);
    assert_eq!(p, [cc_packet(25, 127)]);
    m.turn(K4, 1);
    m.run_ms(50);
    let (p, w) = queued(&mut m, &fw, w);
    assert_eq!(p, [u32::from_le_bytes([0x0C, 0xC0, 1, 0])]);
    let counters = out_counters(&mut m, &fw);
    println!("MIDI out (packets, deferred, dropped): {counters:?}, ring at {w}; K1 {k1:08x?}");
    assert_eq!(counters, (w, 0, 1));
}

#[test]
fn midi_out_costs_little_and_waits_for_no_one() {
    let Some(fw) = fixture("the MIDI out test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(1500); // past a USB retry (once a second while no host answers)
    let (_, main_no_host) = rates(&mut m, 2000);
    attach_host(&mut m, &fw);
    let (_, main_idle) = rates(&mut m, 2000);
    // A CC at full TRS speed for 2 s, 1042 a second: MIDI out looks before and after each
    // (midi_rx_watch), and echoes none.
    let mut stream = Vec::new();
    for i in 0..2084u32 {
        stream.extend([0xB0, CC_CUTOFF, (i % 128) as u8]);
    }
    m.midi(&stream);
    let (_, main_cc_flood) = rates(&mut m, 2000);
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    let (echoed, _) = queued(&mut m, &fw, 0);
    assert!(echoed.is_empty(), "MIDI in echoed: {echoed:08x?}");
    // K1 turned every tick for 2 s, back and forth: a CC a tick. The host never reads, so the
    // ring fills after 64 and the rest waits, one pending value, never the main loop.
    let start = m.main_steps;
    for i in 0..200 {
        m.turn(K1, if i % 40 < 20 { 2 } else { -2 });
        m.run_ms(10);
    }
    let main_turning = (m.main_steps - start) as f64 / 2.0;
    let (packets, deferred, dropped) = out_counters(&mut m, &fw);
    let (p, w) = queued(&mut m, &fw, 0);
    // A note plays meanwhile: the main loop never stalled.
    m.midi(&[0x90, C4, 0x64]);
    m.run_ms(50);
    let v = voice1(&mut m, &fw);
    let report = serde_json::json!({
        "main_instructions_per_s_no_host": main_no_host,
        "main_instructions_per_s_host_idle": main_idle,
        "main_instructions_per_s_trs_cc_flood": main_cc_flood,
        "main_instructions_per_s_k1_every_tick": main_turning,
        "packets": packets,
        "deferred": deferred,
        "dropped": dropped,
        "ring_write_index": w,
    });
    println!("{report}");
    assert_eq!(w, MQ, "the ring is full and stays so");
    assert!(
        p.iter()
            .all(|&x| x & 0xffff == 0xB00B && (x >> 16) as u8 == CC_CUTOFF),
        "only CC 74"
    );
    assert_eq!(packets, MQ);
    assert!(deferred > 100, "{deferred} ticks with no room");
    assert_eq!(v.env_gate, 1, "the note did not start");
    assert!(
        main_idle < 5e6 && main_turning < 5e6 && main_cc_flood < 5e6,
        "the main loop runs {:.2}M/s with a host, {:.2}M/s with K1 turning, {:.2}M/s in a \
         CC flood",
        main_idle / 1e6,
        main_turning / 1e6,
        main_cc_flood / 1e6
    );
}
