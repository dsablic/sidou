// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Expression (M1d Task 2): velocity, pitch bend, the modulation wheel, aftertouch and the
//! per-voice vibrato, at the register level. Controllers are recorded and applied by
//! live_tick; velocity applies at the note-on.

use sidou_sys::*;

use crate::{freq_table, preset, Rig};

const SAW_LEAD: u8 = 0;
const PWM_BASS: u8 = 1;
const SYNC_LEAD: u8 = 2;
const C4: u8 = 60;
const SEMITONE: i32 = LIVE_PITCH_SEMITONE as i32;
const CENTRE: u16 = LIVE_BEND_CENTRE as u16;

fn freq_at(note: u8, offset: i32) -> u16 {
    unsafe { live_freq_at(note, offset) }
}

fn set(rig: &mut Rig, param: u32, value: u32) {
    unsafe { live_param_set(&mut *rig.live, param, value) }
}

fn get(rig: &Rig, param: u32) -> u32 {
    unsafe { live_param_get(&*rig.live, param) }
}

fn bend(rig: &mut Rig, value: u16) {
    unsafe { live_bend(&mut *rig.live, value) }
}

fn pressure(rig: &mut Rig, value: u8) {
    unsafe { live_pressure(&mut *rig.live, value) }
}

fn poly(rig: &mut Rig, note: u8, value: u8) {
    unsafe { live_poly_pressure(&mut *rig.live, note, value) }
}

fn note_vel(rig: &mut Rig, note: u8, velocity: u8) {
    unsafe { live_note_on(&mut *rig.live, note, velocity) }
}

/// The bend's pitch offset: (bend - 8192) * range, in 1/8192 semitone.
fn bend_offset(value: u16, range: i32) -> i32 {
    (i32::from(value) - i32::from(CENTRE)) * range
}

fn cents(a: u16, b: u16) -> f64 {
    1200.0 * (f64::from(b) / f64::from(a)).log2()
}

/// The voice playing `note`, by its frequency at the given offset.
fn voice_at(rig: &Rig, note: u8, offset: i32) -> u8 {
    let f = freq_at(note, offset);
    let vs: Vec<u8> = (0..3).filter(|&v| rig.voice_freq(v) == f).collect();
    assert_eq!(vs.len(), 1, "note {note} {offset:+} on voices {vs:?}");
    vs[0]
}

fn writes_to(log: &[(u8, u8)], regs: &[u8]) -> usize {
    log.iter().filter(|(r, _)| regs.contains(r)).count()
}

// --- pitch bend ---

#[test]
fn the_default_bend_range_is_two_semitones_on_every_preset() {
    for i in 0..LIVE_PRESET_COUNT as u8 {
        assert_eq!(preset(i).bend_range, 2, "preset {i}");
    }
    let rig = Rig::new(SID_MODEL_8580);
    assert_eq!(get(&rig, LIVE_PARAM_BEND_RANGE), LIVE_BEND_RANGE_DEFAULT);
    assert_eq!(rig.live.bend, CENTRE);
}

#[test]
fn a_bend_is_recorded_and_the_tick_writes_it() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(C4);
    rig.clear();
    bend(&mut rig, 16383);
    assert!(rig.log().is_empty(), "the message itself writes nothing");
    rig.tick();
    assert_eq!(rig.log().len(), 2, "one voice's frequency: {:?}", rig.log());
    assert_eq!(rig.voice_freq(0), freq_at(C4, bend_offset(16383, 2)));
    rig.clear();
    rig.tick();
    assert!(rig.log().is_empty(), "nothing changed: nothing written");
}

#[test]
fn the_bend_reaches_two_semitones_each_way() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(C4);
    let t = freq_table();
    for (value, want) in [
        (0u16, t[58]),
        (CENTRE, t[60]),
        (16383, freq_at(C4, 2 * SEMITONE - 2)),
    ] {
        bend(&mut rig, value);
        rig.tick();
        assert_eq!(rig.voice_freq(0), want, "bend {value}");
    }
    // The top is one 14-bit step short of D4, a fraction of a register step.
    assert!(t[62] - rig.voice_freq(0) <= 1);
}

#[test]
fn the_bend_moves_every_sounding_voice_and_new_notes_start_bent() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(48);
    rig.note_on(55);
    rig.note_off(55); // releasing: still sounding, still bent
    bend(&mut rig, 12288); // +1 semitone
    rig.tick();
    let one = SEMITONE;
    voice_at(&rig, 48, one);
    voice_at(&rig, 55, one);
    rig.clear();
    rig.note_on(C4);
    let v = voice_at(&rig, C4, one);
    assert_eq!(rig.voice_freq(v), freq_table()[61]);
    assert!(
        rig.log().iter().any(|&(r, _)| r == v * 7),
        "written at the note-on"
    );
}

#[test]
fn the_bend_range_scales_the_bend_and_clamps_at_24() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(C4);
    set(&mut rig, LIVE_PARAM_BEND_RANGE, 12);
    bend(&mut rig, 0);
    rig.tick();
    assert_eq!(rig.voice_freq(0), freq_table()[48]);
    set(&mut rig, LIVE_PARAM_BEND_RANGE, 99);
    assert_eq!(get(&rig, LIVE_PARAM_BEND_RANGE), LIVE_BEND_RANGE_MAX);
    rig.tick();
    assert_eq!(rig.voice_freq(0), freq_table()[36]);
    set(&mut rig, LIVE_PARAM_BEND_RANGE, 0);
    rig.tick();
    assert_eq!(
        rig.voice_freq(0),
        freq_table()[60],
        "range 0: the bend does nothing"
    );
    bend(&mut rig, u16::MAX);
    assert_eq!(rig.live.bend, LIVE_BEND_MAX as u16, "clamped to 14 bits");
}

#[test]
fn a_bend_flood_between_ticks_writes_once() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [48, 52, 55] {
        rig.note_on(n);
    }
    rig.clear();
    for i in 0..2000u16 {
        bend(&mut rig, i * 8);
    }
    assert!(rig.log().is_empty());
    rig.tick();
    assert_eq!(rig.log().len(), 6, "three voices' frequencies, once");
    voice_at(&rig, 52, bend_offset(1999 * 8, 2));
}

/// Every 14-bit bend value in turn at C4: the frequency never falls and moves by at most one
/// register step (0.44 cents at the bottom of the sweep, A#3); one bend step is 0.024 cents.
#[test]
fn the_bend_is_smooth_across_all_16384_values() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(C4);
    let mut prev = None;
    let mut worst = 0.0f64;
    for value in 0..=LIVE_BEND_MAX as u16 {
        bend(&mut rig, value);
        rig.tick();
        let f = rig.voice_freq(0);
        if let Some(p) = prev {
            assert!(f >= p && f - p <= 1, "bend {value}: {p} -> {f}");
            worst = worst.max(cents(p, f));
        }
        prev = Some(f);
    }
    println!("C4, +-2 semitones: the largest step between adjacent bends is {worst:.3} cents");
    let bottom = freq_table()[58];
    assert!(worst <= cents(bottom, bottom + 1) + 1e-9, "{worst}");
}

/// The sync lead: voice 3 (the sync master) plays the note, voice 1 the ratio above it. Both
/// take the same bend, so the ratio, and the timbre, holds across the bend.
#[test]
fn the_sync_lead_bends_master_and_slave_together() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(SYNC_LEAD);
    let ratio = u32::from(preset(SYNC_LEAD).sync_ratio_q8);
    rig.note_on(C4);
    let mut prev: Option<(u16, u16)> = None;
    let mut worst = 0.0f64;
    for value in (0..=LIVE_BEND_MAX as u16)
        .step_by(7)
        .chain([LIVE_BEND_MAX as u16])
    {
        bend(&mut rig, value);
        rig.tick();
        let master = rig.voice_freq(2);
        let slave = rig.voice_freq(0);
        assert_eq!(master, freq_at(C4, bend_offset(value, 2)), "bend {value}");
        assert_eq!(
            u32::from(slave),
            ((u32::from(master) * ratio) >> 8).min(0xffff),
            "bend {value}"
        );
        assert_eq!(rig.voice_freq(1), 0, "voice 2 stays silent");
        if let Some((pm, ps)) = prev {
            assert!(master >= pm && slave >= ps, "bend {value}");
            worst = worst.max(cents(pm, master)).max(cents(ps, slave));
        }
        prev = Some((master, slave));
    }
    println!("sync lead: the largest step for 7 bend values is {worst:.3} cents");
    assert!(worst < 0.6);
}

// --- vibrato ---

/// The vibrato's phase step per tick for a rate in 0.1 Hz.
fn vibrato_step(rate: u32) -> u32 {
    (rate * 65536 + 500) / 1000
}

/// Its pitch offset at a phase and depth: a triangle from 0, up first.
fn vibrato_offset(phase: u32, depth: i32) -> i32 {
    let p = phase as i32 & 0xffff;
    let t = if p < 16384 {
        p
    } else if p < 49152 {
        32768 - p
    } else {
        p - 65536
    };
    t * depth / 256
}

#[test]
fn vibrato_is_off_by_default_and_writes_nothing() {
    for i in 0..LIVE_PRESET_COUNT as u8 {
        assert_eq!(preset(i).vibrato_depth, 0, "preset {i}");
        assert_eq!(u32::from(preset(i).vibrato_rate), LIVE_VIBRATO_RATE_DEFAULT);
    }
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(C4);
    rig.clear();
    for _ in 0..50 {
        rig.tick();
    }
    assert!(rig.log().is_empty());
}

#[test]
fn vibrato_follows_its_rate_and_depth_ccs_tick_by_tick() {
    let mut rig = Rig::new(SID_MODEL_8580);
    // The CCs land on the knob's walk (knob_curves.rs): CC 89 on 4.7 Hz, CC 102 on depth 69;
    // 5.0 Hz and 64 are not on the walks (their NRPNs set them).
    rig.cc(LIVE_CC_VIBRATO_RATE, 89);
    rig.cc(LIVE_CC_VIBRATO_DEPTH, 102);
    assert_eq!(get(&rig, LIVE_PARAM_VIBRATO_RATE), 47);
    assert_eq!(get(&rig, LIVE_PARAM_VIBRATO_DEPTH), 69);
    set(&mut rig, LIVE_PARAM_VIBRATO_RATE, 50); // 5.0 Hz: 20 ticks a cycle
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 64);
    rig.note_on(C4);
    assert_eq!(
        rig.voice_freq(0),
        freq_table()[60],
        "it starts at the note's pitch"
    );
    let (mut lo, mut hi) = (u16::MAX, 0);
    for k in 1..=40u32 {
        rig.tick();
        let want = freq_at(C4, vibrato_offset(k * vibrato_step(50), 64));
        assert_eq!(rig.voice_freq(0), want, "tick {k}");
        lo = lo.min(want);
        hi = hi.max(want);
    }
    // 64 is half a semitone each way: 100 cents from peak to peak.
    let swing = cents(lo, hi);
    assert!((swing - 100.0).abs() < 1.0, "{swing} cents peak to peak");
}

#[test]
fn the_vibrato_delay_holds_it_off_after_each_note_on() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 127);
    set(&mut rig, LIVE_PARAM_VIBRATO_DELAY, 10);
    assert_eq!(get(&rig, LIVE_PARAM_VIBRATO_DELAY), 10);
    rig.note_on(C4);
    rig.clear();
    for _ in 0..10 {
        rig.tick();
    }
    assert!(rig.log().is_empty(), "within the delay");
    rig.tick();
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    assert_eq!(rig.voice_freq(0), freq_at(C4, vibrato_offset(step, 127)));
}

#[test]
fn each_voice_has_its_own_vibrato_phase_from_its_note_on() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 100);
    rig.note_on(48);
    for _ in 0..3 {
        rig.tick();
    }
    rig.note_on(55);
    rig.tick();
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    assert_eq!(
        rig.voice_freq(0),
        freq_at(48, vibrato_offset(4 * step, 100))
    );
    assert_eq!(rig.voice_freq(1), freq_at(55, vibrato_offset(step, 100)));
}

#[test]
fn vibrato_and_bend_add() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 40);
    rig.note_on(C4);
    bend(&mut rig, 4096);
    rig.tick();
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    let want = freq_at(C4, bend_offset(4096, 2) + vibrato_offset(step, 40));
    assert_eq!(rig.voice_freq(0), want);
}

#[test]
fn the_sync_lead_vibrato_moves_master_and_slave_together() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(SYNC_LEAD);
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 90);
    let ratio = u32::from(preset(SYNC_LEAD).sync_ratio_q8);
    rig.note_on(C4);
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    for k in 1..=30u32 {
        rig.tick();
        let master = freq_at(C4, vibrato_offset(k * step, 90));
        assert_eq!(rig.voice_freq(2), master, "tick {k}");
        assert_eq!(
            u32::from(rig.voice_freq(0)),
            ((u32::from(master) * ratio) >> 8).min(0xffff)
        );
    }
}

/// The LFO steps at the tick rate: a triangle of depth d at f Hz moves 4 * d * f / 100 cents
/// a tick. At 6 Hz and the full depth (+-99 cents) that is 24 cents; at a typical +-25 cents,
/// 6 cents.
#[test]
fn the_vibrato_steps_at_the_tick_rate_are_as_documented() {
    for (depth, rate, bound) in [(127u8, 60u8, 24.5), (32, 60, 7.0), (32, 127, 13.5)] {
        let mut rig = Rig::new(SID_MODEL_8580);
        rig.cc(LIVE_CC_VIBRATO_DEPTH, depth);
        rig.cc(LIVE_CC_VIBRATO_RATE, rate);
        rig.note_on(C4);
        let mut prev = rig.voice_freq(0);
        let mut worst = 0.0f64;
        for _ in 0..200 {
            rig.tick();
            let f = rig.voice_freq(0);
            worst = worst.max(cents(prev, f).abs());
            prev = f;
        }
        println!(
            "vibrato depth {depth} at {:.1} Hz: up to {worst:.2} cents a tick",
            f64::from(rate) / 10.0
        );
        assert!(worst < bound, "{worst}");
    }
}

// --- the modulation wheel ---

#[test]
fn the_wheel_sets_the_pulse_width_by_default() {
    let mut rig = Rig::new(SID_MODEL_8580);
    assert_eq!(get(&rig, LIVE_PARAM_MOD_TARGET), LIVE_MOD_PULSE_WIDTH);
    rig.note_on(C4);
    rig.cc(LIVE_CC_MOD_WHEEL, 0);
    assert_eq!(rig.live.pulse_width, 0);
    assert_eq!(rig.voice_pw(0), 0, "at once, as M1a");
    rig.cc(LIVE_CC_MOD_WHEEL, 127);
    assert_eq!(rig.voice_pw(0), 4095);
    rig.clear();
    for _ in 0..10 {
        rig.tick();
    }
    assert!(rig.log().is_empty(), "no vibrato from the wheel");
}

#[test]
fn the_wheel_on_vibrato_adds_depth_and_leaves_the_pulse_width() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_VIBRATO);
    set(&mut rig, LIVE_PARAM_VIBRATO_DEPTH, 20);
    rig.note_on(C4);
    let pw = rig.voice_pw(0);
    rig.cc(LIVE_CC_MOD_WHEEL, 100);
    assert_eq!(rig.live.pulse_width, preset(SAW_LEAD).pulse_width);
    rig.tick();
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    assert_eq!(rig.voice_freq(0), freq_at(C4, vibrato_offset(step, 120)));
    rig.cc(LIVE_CC_MOD_WHEEL, 127); // 20 + 127, clamped to 127
    rig.tick();
    assert_eq!(
        rig.voice_freq(0),
        freq_at(C4, vibrato_offset(2 * step, 127))
    );
    assert_eq!(rig.voice_pw(0), pw);
}

#[test]
fn the_wheel_on_cutoff_opens_the_filter_above_the_knob() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_CUTOFF);
    let base = u32::from(preset(SAW_LEAD).cutoff);
    rig.cc(LIVE_CC_MOD_WHEEL, 64);
    assert_eq!(
        u32::from(rig.live.cutoff),
        base,
        "the parameter is the knob's"
    );
    rig.tick();
    assert_eq!(fc(&rig), base + 64 * 8);
    rig.knob(LIVE_KNOB_CUTOFF, -10);
    let knob = u32::from(rig.live.cutoff);
    assert!(knob < base);
    assert_eq!(fc(&rig), knob + 512, "the knob moves under the wheel");
    rig.cc(LIVE_CC_MOD_WHEEL, 127);
    rig.cc(LIVE_CC_CUTOFF, 127);
    rig.tick();
    assert_eq!(fc(&rig), LIVE_CUTOFF_MAX, "clamped");
}

#[test]
fn a_new_wheel_target_starts_from_0() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_CUTOFF);
    rig.cc(LIVE_CC_MOD_WHEEL, 100);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_VIBRATO);
    assert_eq!(rig.live.mod_wheel, 0);
    rig.tick();
    assert_eq!(fc(&rig), u32::from(preset(SAW_LEAD).cutoff));
}

fn fc(rig: &Rig) -> u32 {
    u32::from(rig.reg(0x15) & 7) | u32::from(rig.reg(0x16)) << 3
}

// --- aftertouch ---

#[test]
fn pressure_does_nothing_by_default() {
    let mut rig = Rig::new(SID_MODEL_8580);
    assert_eq!(get(&rig, LIVE_PARAM_PRESSURE_TARGET), LIVE_PRESSURE_OFF);
    rig.note_on(C4);
    rig.clear();
    pressure(&mut rig, 127);
    poly(&mut rig, C4, 127);
    for _ in 0..10 {
        rig.tick();
    }
    assert!(rig.log().is_empty());
}

#[test]
fn channel_pressure_on_vibrato_shakes_every_voice() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_VIBRATO);
    rig.note_on(48);
    rig.note_on(55);
    pressure(&mut rig, 80);
    rig.tick();
    let step = vibrato_step(LIVE_VIBRATO_RATE_DEFAULT);
    assert_eq!(rig.voice_freq(0), freq_at(48, vibrato_offset(step, 80)));
    assert_eq!(rig.voice_freq(1), freq_at(55, vibrato_offset(step, 80)));
}

#[test]
fn poly_pressure_on_pulse_width_moves_only_its_voice() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PWM_BASS);
    set(
        &mut rig,
        LIVE_PARAM_PRESSURE_TARGET,
        LIVE_PRESSURE_PULSE_WIDTH,
    );
    for n in [40, 47, 50] {
        rig.note_on(n); // voices 1, 2, 3
    }
    poly(&mut rig, 47, 10);
    rig.tick();
    // the PWM LFO moved too: compare with the untouched voices
    let pw = [rig.voice_pw(0), rig.voice_pw(1), rig.voice_pw(2)];
    assert_eq!(pw[1], pw[0] + 160);
    assert_eq!(pw[2], pw[0]);
    // a voice's poly pressure ends with its note: the next note on it starts at 0
    rig.note_off(47);
    rig.note_on(52);
    assert_eq!(rig.voice_freq(1), freq_table()[52]);
    rig.tick();
    assert_eq!(rig.voice_pw(1), rig.voice_pw(0));
}

#[test]
fn pressure_on_cutoff_takes_the_most_pressed() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_CUTOFF);
    let base = u32::from(preset(SAW_LEAD).cutoff);
    rig.note_on(48);
    rig.note_on(55);
    poly(&mut rig, 48, 20);
    poly(&mut rig, 55, 50);
    pressure(&mut rig, 30);
    rig.tick();
    assert_eq!(fc(&rig), base + 50 * 8);
    poly(&mut rig, 55, 0);
    rig.tick();
    assert_eq!(fc(&rig), base + 30 * 8, "now the channel's");
}

// --- velocity ---

#[test]
fn velocity_is_off_by_default() {
    for i in 0..LIVE_PRESET_COUNT as u8 {
        assert_eq!(u32::from(preset(i).velocity_target), LIVE_VELOCITY_OFF);
    }
    let mut a = Rig::new(SID_MODEL_8580);
    let mut b = Rig::new(SID_MODEL_8580);
    note_vel(&mut a, C4, 1);
    note_vel(&mut b, C4, 127);
    assert_eq!(a.log(), b.log());
}

#[test]
fn velocity_on_sustain_scales_the_voice_sustain_before_the_gate() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_SUSTAIN);
    let sr = preset(SAW_LEAD).sustain_release; // sustain 10
    rig.clear();
    note_vel(&mut rig, C4, 64);
    let log = rig.log().to_vec();
    let sr_at = log.iter().position(|&(r, _)| r == 6).expect("SR written");
    let gate_at = log.iter().position(|&(r, x)| r == 4 && x & 1 == 1).unwrap();
    assert!(sr_at < gate_at, "{log:?}");
    let scaled = |s: u32| (s * 64 + 63) / 127;
    assert_eq!(
        u32::from(rig.reg(6)),
        scaled(10) << 4 | u32::from(sr & 0x0f)
    );
    note_vel(&mut rig, 64, 127);
    assert_eq!(rig.reg(13), sr, "127 is the preset's level");
    // The sustain parameter keeps each voice's velocity.
    set(&mut rig, LIVE_PARAM_SUSTAIN, 15);
    assert_eq!(u32::from(rig.reg(6) >> 4), scaled(15));
    assert_eq!(rig.reg(13) >> 4, 15);
}

#[test]
fn velocity_on_cutoff_lowers_the_filter_by_the_last_note() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_CUTOFF);
    let base = u32::from(preset(SAW_LEAD).cutoff);
    note_vel(&mut rig, C4, 127);
    assert_eq!(fc(&rig), base);
    note_vel(&mut rig, 64, 27);
    assert_eq!(fc(&rig), base - 800, "at once, with the note");
    note_vel(&mut rig, 67, 1);
    assert_eq!(fc(&rig), base - 126 * 8);
}

#[test]
fn velocity_on_pulse_width_narrows_each_voice() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PWM_BASS);
    set(
        &mut rig,
        LIVE_PARAM_VELOCITY_TARGET,
        LIVE_VELOCITY_PULSE_WIDTH,
    );
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 2048);
    // PWM BASS's LFO starts at its centre: the knob's value until the first tick
    note_vel(&mut rig, 40, 127);
    note_vel(&mut rig, 47, 64);
    assert_eq!(rig.voice_pw(0), 2048);
    assert_eq!(rig.voice_pw(1), ((2048 * 64 + 63) / 127) as u16);
    // above the square the pulse narrows from the other side
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 3096);
    assert_eq!(rig.voice_pw(0), 3096);
    assert_eq!(rig.voice_pw(1), 4096 - ((1000 * 64 + 63) / 127) as u16);
}

// --- reset and presets ---

#[test]
fn reset_all_controllers_centres_the_bend_and_zeroes_wheel_and_pressure() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_CUTOFF);
    set(&mut rig, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_CUTOFF);
    rig.note_on(C4);
    bend(&mut rig, 0);
    rig.cc(LIVE_CC_MOD_WHEEL, 50);
    pressure(&mut rig, 50);
    poly(&mut rig, C4, 60);
    rig.tick();
    rig.cc(LIVE_CC_RESET_ALL, 0);
    rig.tick();
    assert_eq!(rig.voice_freq(0), freq_table()[60]);
    assert_eq!(fc(&rig), u32::from(preset(SAW_LEAD).cutoff));
    assert_eq!(rig.live.bend, CENTRE);
}

#[test]
fn a_program_change_resets_the_expression_settings_and_keeps_the_controllers() {
    let mut rig = Rig::new(SID_MODEL_8580);
    set(&mut rig, LIVE_PARAM_BEND_RANGE, 12);
    set(&mut rig, LIVE_PARAM_VELOCITY_TARGET, LIVE_VELOCITY_CUTOFF);
    set(&mut rig, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_VIBRATO);
    rig.cc(LIVE_CC_VIBRATO_RATE, 10);
    rig.cc(LIVE_CC_VIBRATO_DEPTH, 10);
    rig.cc(LIVE_CC_VIBRATO_DELAY, 10);
    bend(&mut rig, 0);
    rig.set_preset(SAW_LEAD);
    for (param, want) in [
        (LIVE_PARAM_BEND_RANGE, 2),
        (LIVE_PARAM_VELOCITY_TARGET, 0),
        (LIVE_PARAM_MOD_TARGET, 0),
        (LIVE_PARAM_PRESSURE_TARGET, 0),
        (LIVE_PARAM_VIBRATO_RATE, LIVE_VIBRATO_RATE_DEFAULT),
        (LIVE_PARAM_VIBRATO_DEPTH, 0),
        (LIVE_PARAM_VIBRATO_DELAY, 0),
    ] {
        assert_eq!(get(&rig, param), want, "parameter {param}");
    }
    rig.note_on(C4);
    assert_eq!(rig.voice_freq(0), freq_table()[58], "still bent down 2");
}

/// Everything moving at once: a tick writes three frequencies, three pulse widths and the
/// cutoff, 14 at most (reg_ring.c's budget).
#[test]
fn a_tick_writes_at_most_14() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PWM_BASS);
    set(&mut rig, LIVE_PARAM_PRESSURE_TARGET, LIVE_PRESSURE_CUTOFF);
    rig.cc(LIVE_CC_VIBRATO_DEPTH, 127);
    for n in [40, 47, 52] {
        rig.note_on(n);
    }
    let mut most = 0;
    for i in 0..100u8 {
        bend(&mut rig, u16::from(i) * 150);
        pressure(&mut rig, i + 1);
        rig.clear();
        rig.tick();
        let n = rig.log().len();
        assert!(n <= 14, "{:?}", rig.log());
        assert_eq!(writes_to(rig.log(), &[0x15, 0x16]), 2);
        most = most.max(n);
    }
    assert_eq!(most, 14);
}

/// controls.md, Presets: the sync lead's voice 1 (2.75 times the pitch) tops out at 0xffff
/// from note 90 at rest and from note 88 bent up 2 semitones; voice 3 keeps the pitch.
#[test]
fn the_sync_leads_voice_1_tops_out_where_the_reference_says() {
    let voice_1 = |note: u8, value: u16| {
        let mut rig = Rig::new(SID_MODEL_8580);
        rig.set_preset(SYNC_LEAD);
        bend(&mut rig, value);
        rig.note_on(note);
        rig.tick();
        (rig.voice_freq(0), rig.voice_freq(2))
    };
    assert!(voice_1(89, CENTRE).0 < 0xffff);
    assert_eq!(voice_1(90, CENTRE).0, 0xffff);
    assert!(voice_1(87, LIVE_BEND_MAX as u16).0 < 0xffff);
    let (top, pitch) = voice_1(88, LIVE_BEND_MAX as u16);
    assert_eq!(top, 0xffff);
    assert_eq!(
        pitch,
        unsafe { live_freq_at(88, bend_offset(LIVE_BEND_MAX as u16, 2)) },
        "voice 3 still follows the bend"
    );
}
