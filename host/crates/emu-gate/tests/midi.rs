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

/// Past the LCD's reset delays and the first audio blocks. MIDI plays through the boot
/// animation (app/boot.h), so the tests that play only MIDI start inside it; the ones that
/// touch a panel control wait for it to go (`booted_for_the_panel`).
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

/// Booted until the boot view is gone (common::run_past_boot): a control during the boot
/// would skip it instead of doing what the test wants.
fn booted_for_the_panel(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
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

/// A TRS note-on while the boot's title card shows (app/boot.h: from about 0.4 s to 1.6 s)
/// plays at once and ends the boot: the status screen wipes in, with the note on it.
#[test]
fn a_trs_note_on_during_the_title_card_plays_and_ends_the_boot() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(600);
    let boot = fw.symbol("boot_view");
    assert_eq!(
        common::top_view(&mut m, &fw),
        boot,
        "the title card is not up at 600 ms"
    );
    m.midi(&[0x90, C4, 0x64]);
    let x = record(&mut m, 200);
    let gone = common::run_past_boot(&mut m, &fw);
    println!(
        "C4 over TRS during the title card: peak {}, the boot gone at {gone} ms",
        peak(&x)
    );
    assert!(
        peak(&x) > 1000.0,
        "the note did not play: peak {}",
        peak(&x)
    );
    // A frame to see the note, 8 to wipe: well before the short boot's own end (1.6 s).
    assert!(
        gone < 1200,
        "the note did not end the boot: it went on until {gone} ms"
    );
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("views_status"),
        "not the status screen"
    );
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
    let mut m = booted_for_the_panel(&fw);
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
    let mut m = booted_for_the_panel(&fw);
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

/// The instructions of the most expensive `midi_send` (one tick of MIDI out) over the next
/// `ms`, after `poke` set MIDI out's state in emulated RAM.
fn worst_send(m: &mut Machine, fw: &Firmware, poke: impl Fn(&mut Machine, u32, u32)) -> u64 {
    let base = fw.symbol("midi_tx");
    for p in 0..sidou_sys::LIVE_PARAM_COUNT {
        poke(m, base, p);
    }
    m.watched_calls.clear();
    m.run_ms(30);
    *m.watched_calls.iter().max().expect("midi_send ran")
}

#[test]
fn a_tick_of_midi_out_costs_little_at_worst() {
    let Some(fw) = fixture("the MIDI out test") else {
        return;
    };
    let mut m = booted(&fw);
    attach_host(&mut m, &fw);
    m.run_ms(50);
    m.watch_calls(fw.symbol("midi_send"));
    let idle = worst_send(&mut m, &fw, |_, _, _| {});
    // Every parameter pending with nothing new on the wire: each is looked up, encoded and
    // dropped in the one tick.
    let pending = |m: &mut Machine, base: u32, p: u32| {
        m.soc.write8(base + ilp32::midi_out::pending as u32 + p, 1);
    };
    let deduplicated = worst_send(&mut m, &fw, pending);
    // Every parameter pending and changed: a full tick of packets, the rest deferred.
    let changed = worst_send(&mut m, &fw, |m, base, p| {
        pending(m, base, p);
        let seen = m.soc.read16(base + ilp32::midi_out::seen as u32 + 2 * p);
        let max = unsafe { sidou_sys::live_param_max(p) } as u16;
        let wire = if seen == 0 { max } else { 0 };
        m.soc
            .write16(base + ilp32::midi_out::wire as u32 + 2 * p, wire);
    });
    let (packets, _, _) = out_counters(&mut m, &fw);
    let report = serde_json::json!({
        "midi_send_instructions_idle": idle,
        "midi_send_instructions_all_deduplicated": deduplicated,
        "midi_send_instructions_all_changed": changed,
        "packets": packets,
    });
    println!("{report}");
    assert!(
        packets >= sidou_sys::MIDI_OUT_TICK_PACKETS,
        "{packets} packets"
    );
    // 1.1k idle (the look at 36 parameters); at worst 3.1k, every parameter within one CC
    // step of the value on the wire (7.1k before the rows were cached).
    assert!(
        idle < 1_300 && deduplicated < 3_500 && changed < 3_500,
        "a tick of MIDI out: {idle} instructions idle, {deduplicated} with every parameter \
         deduplicated, {changed} with every parameter changed"
    );
}

/// USB-MIDI packets straight into platform/fm1/midi.c's MIDI in ring (`in_ring`), as usb.c's
/// poll pushes them: fm1-emu has no USB controller. Returns how many fit.
fn usb_in(m: &mut Machine, fw: &Firmware, packets: &[u32]) -> usize {
    use ilp32::{midi_ring as r, midi_ring_entry as e};
    let ring = fw.symbol("in_ring");
    let size = sidou_sys::MIDI_RING_SIZE;
    let head = m.soc.read32(ring + r::head as u32);
    let tail = m.soc.read32(ring + r::tail as u32);
    let epoch = m.soc.read8(ring + r::epoch as u32);
    let n = packets.len().min((size - (head - tail)) as usize);
    for (i, &p) in packets[..n].iter().enumerate() {
        let at = ring + r::e as u32 + ((head + i as u32) % size) * e::SIZE as u32;
        m.soc.write32(at + e::stamp as u32, 0);
        m.soc.write32(at + e::data as u32, p);
        m.soc
            .write8(at + e::source as u32, sidou_sys::MIDI_SRC_USB as u8);
        m.soc.write8(at + e::epoch as u32, epoch);
    }
    m.soc.write32(ring + r::head as u32, head + n as u32);
    n
}

#[test]
fn a_usb_cc_burst_costs_one_look_a_drain_pass() {
    let Some(fw) = fixture("the MIDI out test") else {
        return;
    };
    let mut m = booted(&fw);
    attach_host(&mut m, &fw);
    m.run_ms(1500); // past a USB retry
    m.watch_calls(fw.symbol("midi_drain"));
    // 16 CCs in one pass: the cutoff, the resonance and the attack, alternating.
    let burst: Vec<u32> = (0..16u8)
        .map(|i| cc_packet([CC_CUTOFF, 71, 73][usize::from(i % 3)], i * 8))
        .collect();
    let mut passes = Vec::new();
    for _ in 0..20 {
        assert_eq!(usb_in(&mut m, &fw, &burst), burst.len());
        m.watched_calls.clear();
        m.run_ms(5);
        passes.push(*m.watched_calls.iter().max().expect("midi_drain ran"));
    }
    let entries = stats(&mut m, &fw).entries;
    let worst_pass = *passes.iter().max().unwrap();
    // Dense DAW automation: 16 CCs every millisecond (16k a second, USB is not
    // baud-limited) for a second.
    let (isr0, main0) = (m.isr_steps, m.main_steps);
    for _ in 0..1000 {
        usb_in(&mut m, &fw, &burst);
        m.run_ms(1);
    }
    let main_flood = (m.main_steps - main0) as f64;
    let isr_flood = (m.isr_steps - isr0) as f64;
    let (echoed, _) = queued(&mut m, &fw, 0);
    let report = serde_json::json!({
        "midi_drain_instructions_16_ccs": worst_pass,
        "main_instructions_per_s_usb_16k_cc": main_flood,
        "isr_instructions_per_s_usb_16k_cc": isr_flood,
        "entries": entries,
    });
    println!("{report}");
    assert_eq!(entries, 20 * 16);
    assert!(echoed.is_empty(), "MIDI in echoed: {echoed:08x?}");
    // MIDI out's two looks (2.2k) and 16 CCs onto live (0.4k each): 8.2k. With a look before
    // and after every CC it was 31.5k.
    assert!(
        worst_pass < 9_000,
        "a drain pass of 16 CCs costs {worst_pass} instructions"
    );
    // 10.5M/s, past the 5M/s of the other floods: the CCs themselves cost 6M/s at this rate
    // (MIDI out 2.2M/s). USB holds what the ring has no room for in the endpoint and the host
    // retries, so input this dense slows down rather than stalling the main loop. 33.7M/s
    // with the looks around every CC.
    assert!(
        main_flood < 12e6,
        "the main loop runs {:.2}M/s with 16k CCs a second over USB",
        main_flood / 1e6
    );
}
