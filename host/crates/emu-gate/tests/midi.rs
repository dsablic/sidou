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
/// plays at once and ends the boot: LIVE wipes in, with the note on it.
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
        fw.symbol("live_view_view"),
        "not LIVE"
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

/// `rates` with the jack at its full speed throughout (`Machine::midi_paced`).
fn paced_rates(m: &mut Machine, ms: u64, message: impl FnMut(u32) -> Vec<u8>) -> (f64, f64) {
    let (isr, main) = (m.isr_steps, m.main_steps);
    m.midi_paced(ms, message);
    let per_s = 1000.0 / ms as f64;
    (
        (m.isr_steps - isr) as f64 * per_s,
        (m.main_steps - main) as f64 * per_s,
    )
}

/// After `midi_paced`: nothing dropped and bytes still waiting, so the jack never stopped.
fn jack_busy_throughout(m: &Machine) {
    assert_eq!(m.soc.periph.midi_dropped(), 0, "the jack dropped bytes");
    assert!(
        m.soc.periph.midi_queued() > 0,
        "the jack ran dry: the window was not at full speed"
    );
}

/// The jack's full speed in `paced_rates`: a note-on and its note-off every 1.92 ms, across
/// two octaves.
fn note_on_off(i: u32) -> Vec<u8> {
    let note = 48 + (i % 24) as u8;
    vec![0x90, note, 0x64, 0x80, note, 0x00]
}

/// What MIDI input adds, measured on the status screen: LIVE, the root, draws every note it
/// plays (bounded in `live_under_a_full_speed_midi_flood_stays_within_its_budget` below and
/// in screen.rs), the status screen only its note names. The increment, busy minus idle, is
/// the drain, the parser and `live`.
#[test]
fn midi_input_costs_little() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted_for_the_panel(&fw);
    common::open_status(&mut m, &fw);
    m.run_ms(1500); // past a USB retry (once a second while no host answers)
    let (isr_idle, main_idle) = rates(&mut m, 2000);
    let (isr_busy, main_busy) = paced_rates(&mut m, 2000, note_on_off);
    let report = serde_json::json!({
        "isr_instructions_per_s_idle": isr_idle,
        "main_instructions_per_s_idle": main_idle,
        "isr_instructions_per_s_trs_full_speed": isr_busy,
        "main_instructions_per_s_trs_full_speed": main_busy,
        "midi_bytes_left_in_the_jack": m.soc.periph.midi_queued(),
    });
    println!("{report}");
    jack_busy_throughout(&m);
    assert!(
        main_busy - main_idle < MIDI_MAIN_INCREMENT,
        "TRS MIDI at full speed adds {:.2}M main-loop instructions/s ({:.2}M/s idle)",
        (main_busy - main_idle) / 1e6,
        main_idle / 1e6
    );
    assert!(
        isr_busy - isr_idle < 2e6,
        "TRS MIDI at full speed adds {:.2}M interrupt instructions/s",
        (isr_busy - isr_idle) / 1e6
    );
}

/// The main-loop instructions a second MIDI input at the jack's full speed may add: 1.33M
/// at M1e Task 4 (2.11M/s idle on the status screen, 3.44M/s busy), 0.55 % of 240 MHz.
const MIDI_MAIN_INCREMENT: f64 = 1.5e6;

/// LIVE, the power-on root, with the jack at its full speed: every note moves a voice's
/// scope, envelope, note name and meter. The main loop's whole rate, LIVE and MIDI together.
#[test]
fn live_under_a_full_speed_midi_flood_stays_within_its_budget() {
    let Some(fw) = fixture("the MIDI test") else {
        return;
    };
    let mut m = booted_for_the_panel(&fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    m.run_ms(1500); // past a USB retry (once a second while no host answers)
    let (_, main_idle) = rates(&mut m, 2000);
    let (_, main_busy) = paced_rates(&mut m, 2000, note_on_off);
    println!(
        "{}",
        serde_json::json!({
            "main_instructions_per_s_idle": main_idle,
            "main_instructions_per_s_trs_full_speed": main_busy,
        })
    );
    jack_busy_throughout(&m);
    assert!(
        main_busy < LIVE_MIDI_FLOOD_MAIN,
        "LIVE with TRS MIDI at full speed: the main loop runs {:.2}M/s",
        main_busy / 1e6
    );
}

/// The main loop's instructions a second on LIVE with the jack at its full speed: 5.73M at
/// M1e Task 4 (2.27M/s idle). Over the 5M/s the status screen was held to before LIVE: the
/// scopes, envelope bars, note names and meters of a note-on and a note-off every 1.92 ms
/// cost about 84k instructions a frame (LIVE's compose, the tap's reading half way between
/// frames, the runs), against 14k at rest, on top of MIDI's own 1.3M/s. 6.3M/s is 2.6 % of
/// 240 MHz; what matters for playing is bounded elsewhere: the longest ui_poll (screen.rs
/// LIVE_WORST_POLL, within the latency gate's poll allowance) and no byte lost or late
/// (`a_full_speed_stream_loses_nothing`, the flood tests above).
const LIVE_MIDI_FLOOD_MAIN: f64 = 6.3e6;

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
    // The FX button: the chip model on CC 25, and the profile that follows it on CC 85 (the
    // 8580 R5). K4: a program change, without the parameters the preset loads.
    press(&mut m, FX_BUTTON);
    let (p, w) = queued(&mut m, &fw, w);
    let r5 = unsafe {
        sidou_sys::midi_map_to_cc(
            sidou_sys::midi_map_for_param(sidou_sys::LIVE_PARAM_PROFILE),
            sidou_sys::SID_CHIP_8580_R5,
        )
    };
    assert_eq!(p, [cc_packet(25, 127), cc_packet(85, r5)]);
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
    // (midi_rx_watch), and echoes none. 4.54M/s, 2.3k instructions a CC (5.37M/s when each
    // look read the 37 parameters through live_param_get; this window was half idle before
    // the stream was paced, since fm1-emu drops a backlog past 3125 bytes).
    let (_, main_cc_flood) = paced_rates(&mut m, 2000, |i| vec![0xB0, CC_CUTOFF, (i % 128) as u8]);
    jack_busy_throughout(&m);
    m.run_ms(1000);
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

/// MIDI out's tick, instructions per parameter: idle (the look) and with every parameter
/// back at its value on the wire (a_tick_of_midi_out_costs_little_at_worst).
const IDLE_PER_PARAM: f64 = 22.0;
const DEDUPLICATED_PER_PARAM: f64 = 46.0;

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
    // Every parameter pending with nothing new on the wire: each is dropped in the one tick
    // (back at its value on the wire, before it is looked up or encoded).
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
    // The same with every parameter bound by MIDI learn (platform/fm1/midi.c `learned`) to a
    // CC: parameter p to the p-th learnable CC (0..47 are all learnable but bank select, 0 and
    // 32, and data entry, 6 and 38).
    let learned = fw.symbol("learned");
    let learnable: Vec<u32> = (0..120)
        .filter(|&cc| unsafe { sidou_sys::midi_learn_learnable(cc) } != 0)
        .collect();
    for p in 0..sidou_sys::LIVE_PARAM_COUNT {
        let cc = learnable[p as usize];
        m.soc
            .write8(learned + ilp32::midi_learn::param_cc as u32 + p, cc as u8);
        let row = unsafe { sidou_sys::midi_map_for_param(p) };
        let index = (row as usize - unsafe { sidou_sys::midi_map_row(0) } as usize)
            / size_of::<sidou_sys::midi_map_entry>();
        m.soc
            .write8(learned + ilp32::midi_learn::cc_row as u32 + cc, index as u8);
    }
    let changed_learned = worst_send(&mut m, &fw, |m, base, p| {
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
        "midi_send_instructions_all_changed_and_learned": changed_learned,
        "packets": packets,
    });
    println!("{report}");
    assert!(
        packets >= sidou_sys::MIDI_OUT_TICK_PACKETS,
        "{packets} packets"
    );
    // 0.8k idle (the look at 37 parameters, one live_params snapshot: 1.2k with a
    // live_param_get each); 1.7k with every parameter back at its value on the wire; at worst
    // 2.6k, every parameter changed, and less with each bound by MIDI learn (7.1k before the
    // rows were cached). The idle and deduplicated bounds are per parameter, at what M1c
    // Task 3 measured with its character's 42 (906 and 1,914: 21.6 and 45.6 a parameter);
    // a tick of changes is bounded by its packets, not the count.
    let per_param = |x: u64| x as f64 / f64::from(sidou_sys::LIVE_PARAM_COUNT);
    assert!(
        per_param(idle) < IDLE_PER_PARAM
            && per_param(deduplicated) < DEDUPLICATED_PER_PARAM
            && changed < 3_000
            && changed_learned < 3_000,
        "a tick of MIDI out: {idle} instructions idle, {deduplicated} with every parameter \
         deduplicated, {changed} with every parameter changed, {changed_learned} also learned"
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
    // MIDI out's two looks and 16 CCs onto live: 7.5k (8.5k when a look read the parameters
    // one live_param_get at a time). With a look before and after every CC it was 31.5k.
    assert!(
        worst_pass < 8_000,
        "a drain pass of 16 CCs costs {worst_pass} instructions"
    );
    // 9.6M/s, past the 5M/s of the other floods: the CCs themselves cost 6M/s at this rate.
    // USB holds what the ring has no room for in the endpoint and the host retries, so input
    // this dense slows down rather than stalling the main loop. 10.6M/s when a look read the
    // parameters one at a time, 33.7M/s with the looks around every CC.
    assert!(
        main_flood < 10.5e6,
        "the main loop runs {:.2}M/s with 16k CCs a second over USB",
        main_flood / 1e6
    );
}

/// CC 86 (docs/reference/midi.md): age/heat. CC 71: the resonance.
const CC_AGE: u8 = 86;
const CC_RESONANCE: u8 = 71;

/// What a flood of filter writes adds to the audio interrupt, in instructions a sample: 16
/// USB CCs every millisecond of `cc` for a second (each value differs from the last, so every
/// one reaches the chip: CC 74 writes $15 and $16, CC 71 $17), against a second with no
/// input, a note held and age/heat at `age` (the chip's filter blended, filter_t, at 127).
fn filter_flood_isr(fw: &Firmware, cc: u8, age: u8) -> f64 {
    let mut m = booted(fw);
    m.run_ms(1500); // past a USB retry
    m.midi(&[0x90, C4, 0x64, 0xB0, CC_AGE, age]);
    m.run_ms(600); // the ramp: 0 to 127 in 0.32 s
    let s = active_sid(&mut m, fw);
    let t = f32::from_bits(m.soc.read32(s + ilp32::sid::filter_t as u32));
    assert_eq!(
        t,
        f32::from(age) / 127.0,
        "age/heat {age} is not on the chip"
    );
    let (idle, _) = rates(&mut m, 1000);
    let isr0 = m.isr_steps;
    let mut fed = 0;
    for ms in 0..1000u32 {
        let burst: Vec<u32> = (0..16u32)
            .map(|i| cc_packet(cc, ((ms * 16 + i) * 37 % 128) as u8))
            .collect();
        fed += usb_in(&mut m, fw, &burst);
        m.run_ms(1);
    }
    let flood = (m.isr_steps - isr0) as f64;
    assert_eq!(fed, 16_000, "the MIDI in ring refused CCs");
    (flood - idle) / f64::from(common::SAMPLE_RATE)
}

/// Dense filter automation (16 USB CCs a millisecond, as `a_usb_cc_burst_costs_one_look_a_drain_pass`)
/// costs the audio interrupt's drain no more than the ISR's allowance (common::ISR_OVERHEAD),
/// with age/heat at 127, where each cutoff is an atan and a tan from the blended filter rows:
/// sid_write only marks the filter, and the render recomputes it once a block.
#[test]
fn a_filter_cc_flood_fits_the_isr_overhead_at_any_age() {
    let Some(fw) = fixture("the filter flood test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    let mut worst: f64 = 0.0;
    let mut aged: f64 = 0.0;
    for (cc, name) in [(CC_CUTOFF, "cutoff"), (CC_RESONANCE, "resonance")] {
        let [young, old] = [0u8, 127].map(|age| {
            let c = filter_flood_isr(&fw, cc, age);
            report.insert(format!("isr_per_sample_{name}_age_{age}"), c.into());
            c
        });
        worst = worst.max(young).max(old);
        aged = aged.max(old - young);
    }
    println!("{}", serde_json::Value::Object(report));
    // The blend's atan and tan once a block, not once a write: about 70 instructions in 128
    // samples. Per write they cost 50 a sample (108 against 58 at the cutoff).
    assert!(
        aged <= 2.0,
        "age/heat at 127 adds {aged:.1} instructions a sample to a filter CC flood"
    );
    assert!(
        worst <= common::ISR_OVERHEAD,
        "a filter CC flood adds {worst:.1} instructions a sample to the audio interrupt, over \
         the ISR's {} allowance",
        common::ISR_OVERHEAD
    );
}

// --- MIDI learn ---

/// app/panel.h PANEL_BTN_SAVE: held while a knob turns, opens MIDI learn on the knob
/// (docs/reference/controls.md, "SAVE").
const SAVE: u8 = 9;
const HOME: u8 = 8;
/// A CC the map leaves free (docs/reference/midi.md).
const CC_FREE: u8 = 14;
/// The sustain pedal: neither the map nor learn takes it.
const CC_SUSTAIN: u8 = 64;

/// platform/fm1/main.c `instrument`, a `live`, at the device's layout.
fn live_cutoff(m: &mut Machine, fw: &Firmware) -> u32 {
    u32::from(
        m.soc
            .read16(fw.symbol("instrument") + ilp32::live::cutoff as u32),
    )
}

/// SAVE held, K1 turned, SAVE up: MIDI learn open on K1's parameter.
fn open_learn(m: &mut Machine) {
    m.key(SAVE, true);
    m.run_ms(30);
    m.turn(K1, 1);
    m.run_ms(30);
    m.key(SAVE, false);
    m.run_ms(30);
}

/// platform/fm1/midi.c `learned`: the state and CC 14's bound row.
fn learned(m: &mut Machine, fw: &Firmware) -> (u8, u8, u8) {
    let base = fw.symbol("learned");
    (
        m.soc.read8(base + ilp32::midi_learn::state as u32),
        m.soc.read8(base + ilp32::midi_learn::count as u32),
        m.soc
            .read8(base + ilp32::midi_learn::cc_row as u32 + u32::from(CC_FREE)),
    )
}

/// SAVE held and K1 turned open learn on the cutoff (SAVE's press alone does not), SAVE up (the
/// frame stays), CC 14 over TRS: CC 14 drives the cutoff from then on, on any channel, through
/// the map's linear curve, and the map's CC 74 still does too. HOME then cancels a second learn,
/// and SAVE held + HOME, then SAVE, clears the binding.
#[test]
fn midi_learn_over_trs_binds_cc_14_to_k1s_cutoff() {
    let Some(fw) = fixture("the MIDI learn test") else {
        return;
    };
    let mut m = booted_for_the_panel(&fw);
    let root = fw.symbol("live_view_view"); // LIVE, under the frame
    let learn = fw.symbol("learn_view_view");
    let cutoff_row = unsafe { sidou_sys::midi_map_for_param(sidou_sys::LIVE_PARAM_CUTOFF) };
    let row_index = (cutoff_row as usize - unsafe { sidou_sys::midi_map_row(0) } as usize)
        / size_of::<sidou_sys::midi_map_entry>();
    let from_cc = |v: u8| unsafe { sidou_sys::midi_map_from_cc(cutoff_row, v) };

    // SAVE down, and K1 turned before it comes up: the frame opens on the turn and stays.
    m.key(SAVE, true);
    m.run_ms(50);
    assert_eq!(
        common::top_view(&mut m, &fw),
        root,
        "SAVE's press alone opened learn"
    );
    m.turn(K1, 2);
    m.run_ms(50);
    assert_eq!(
        common::top_view(&mut m, &fw),
        learn,
        "SAVE held + K1 did not open learn"
    );
    m.key(SAVE, false);
    m.run_ms(50);
    assert_eq!(
        live_cutoff(&mut m, &fw),
        SAW_LEAD_CUTOFF,
        "K1 picks the cutoff, it does not turn it"
    );
    assert_eq!(
        learned(&mut m, &fw),
        (
            sidou_sys::MIDI_LEARN_CC as u8,
            0,
            sidou_sys::MIDI_LEARN_NONE as u8
        )
    );
    // CC 14 over TRS binds it, and its value sets the cutoff.
    m.midi(&[0xB0, CC_FREE, 32]);
    m.run_ms(100);
    let after = learned(&mut m, &fw);
    let top = common::top_view(&mut m, &fw);
    let mut follow = Vec::new();
    for (ch, v) in [(0u8, 0u8), (5, 127), (15, 64), (9, 1)] {
        m.midi(&[0xB0 | ch, CC_FREE, v]);
        m.run_ms(20);
        follow.push((v, live_cutoff(&mut m, &fw), cutoff(&mut m, &fw)));
    }
    m.midi(&[0xB0, CC_CUTOFF, 100]);
    m.run_ms(20);
    let map_cc = cutoff(&mut m, &fw);
    println!("learned {after:?}; CC 14 (value, live, chip): {follow:?}; CC 74 = 100: {map_cc}");
    assert_eq!(
        after,
        (sidou_sys::MIDI_LEARN_IDLE as u8, 1, row_index as u8),
        "CC 14 is not bound to the cutoff's row"
    );
    assert_eq!(top, root, "the learn frame did not close");
    for (v, l, chip) in follow {
        assert_eq!((l, chip), (from_cc(v), from_cc(v)), "CC 14 = {v}");
    }
    assert_eq!(map_cc, from_cc(100), "the map's CC 74 stopped working");

    // A second learn, cancelled by HOME: nothing changes, HOME does not open the menu.
    open_learn(&mut m);
    press(&mut m, HOME);
    m.run_ms(50);
    assert_eq!(common::top_view(&mut m, &fw), root);
    m.midi(&[0xB0, CC_FREE + 1, 127]);
    m.run_ms(20);
    assert_eq!(learned(&mut m, &fw).1, 1, "a cancelled learn bound CC 15");
    // SAVE held + HOME asks, SAVE confirms: CC 14 does nothing again.
    open_learn(&mut m);
    m.key(SAVE, true);
    m.run_ms(50);
    press(&mut m, HOME);
    m.key(SAVE, false);
    m.run_ms(50);
    press(&mut m, SAVE);
    m.run_ms(50);
    let cleared = learned(&mut m, &fw);
    m.midi(&[0xB0, CC_FREE, 0]);
    m.run_ms(20);
    assert_eq!(
        cleared,
        (
            sidou_sys::MIDI_LEARN_IDLE as u8,
            0,
            sidou_sys::MIDI_LEARN_NONE as u8
        )
    );
    assert_eq!(
        cutoff(&mut m, &fw),
        from_cc(100),
        "a cleared CC 14 still sets the cutoff"
    );
    assert_eq!(common::top_view(&mut m, &fw), root);
}

/// A learned CC costs a lookup: a flood of it at full TRS speed costs the main loop what a
/// flood of a map CC does, with MIDI out watching (no echo), and both stay within the bound
/// of `midi_out_costs_little_and_waits_for_no_one`. So does a flood of CC 64, which neither
/// the map nor learn takes: it skips MIDI out's looks. Each flood keeps the jack busy for
/// the whole 2 s (`Machine::midi_paced`).
#[test]
fn a_learned_cc_flood_costs_what_a_map_cc_flood_does() {
    let Some(fw) = fixture("the MIDI learn test") else {
        return;
    };
    let mut m = booted_for_the_panel(&fw);
    attach_host(&mut m, &fw);
    open_learn(&mut m);
    m.midi(&[0xB0, CC_FREE, 0]);
    m.run_ms(1500); // past a USB retry

    // A CC every 0.96 ms for 2 s, the jack's full speed; then the backlog drains. The last
    // value fed is the one that stays.
    let flood = |m: &mut Machine, cc: u8| {
        let main = m.main_steps;
        let fed = m.midi_paced(2000, |i| vec![0xB0, cc, (i % 128) as u8]);
        let rate = (m.main_steps - main) as f64 / 2.0;
        jack_busy_throughout(m);
        m.run_ms(1000);
        (rate, ((fed - 1) % 128) as u8)
    };
    let (main_map, _) = flood(&mut m, CC_CUTOFF);
    let (main_learned, learned_last) = flood(&mut m, CC_FREE);
    let (main_unmapped, _) = flood(&mut m, CC_SUSTAIN);
    let s = stats(&mut m, &fw);
    let (echoed, _) = queued(&mut m, &fw, 0);
    let report = serde_json::json!({
        "main_instructions_per_s_trs_cc_flood_map": main_map,
        "main_instructions_per_s_trs_cc_flood_learned": main_learned,
        "main_instructions_per_s_trs_cc_flood_unmapped": main_unmapped,
    });
    println!("{report}; {s:?}");
    assert_eq!(
        m.soc.periph.midi_queued(),
        0,
        "the jack has not delivered all"
    );
    assert_eq!(m.soc.periph.midi_dropped(), 0, "the jack dropped bytes");
    assert_eq!((s.lost, s.restarts), (0, 0));
    assert_eq!(learned(&mut m, &fw).1, 1);
    assert_eq!(cutoff(&mut m, &fw), unsafe {
        sidou_sys::midi_map_from_cc(
            sidou_sys::midi_map_for_param(sidou_sys::LIVE_PARAM_CUTOFF),
            learned_last,
        )
    });
    assert!(echoed.is_empty(), "MIDI in echoed: {echoed:08x?}");
    assert!(
        main_learned < main_map * 1.01,
        "the main loop runs {:.3}M/s in a map CC flood, {:.3}M/s in a learned CC flood",
        main_map / 1e6,
        main_learned / 1e6
    );
    assert!(
        main_map < 5e6 && main_learned < 5e6 && main_unmapped < 5e6,
        "the main loop runs {:.3}M/s in a map CC flood, {:.3}M/s in a learned CC flood, \
         {:.3}M/s in a CC 64 flood",
        main_map / 1e6,
        main_learned / 1e6,
        main_unmapped / 1e6
    );
}
