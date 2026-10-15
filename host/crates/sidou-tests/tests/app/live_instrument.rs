// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeSet;
use std::ffi::c_void;

use sidou_sys::*;

use crate::{freq_table, preset, preset_name, Rig};

const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const NOISE: u8 = 0x80;
const SYNC: u8 = 0x02;
const SAW_LEAD: u8 = 0;
const PULSE_BASS: u8 = 1;
const SYNC_LEAD: u8 = 2;
const NOISE_PERC: u8 = 3;

/// The voice whose frequency is the given note's (unique among gated voices).
fn voice_of(rig: &Rig, note: u8) -> u8 {
    let f = freq_table()[usize::from(note)];
    let vs: Vec<u8> = (0..3)
        .filter(|&v| rig.gated(v) && rig.voice_freq(v) == f)
        .collect();
    assert_eq!(vs.len(), 1, "note {note} on voices {vs:?}");
    vs[0]
}

fn gates(rig: &Rig) -> [bool; 3] {
    [rig.gated(0), rig.gated(1), rig.gated(2)]
}

// --- init and registers ---

const GATE_RESET: u8 = LIVE_REG_GATE_RESET as u8;

#[test]
fn init_resets_the_chip_then_writes_every_register_once() {
    let rig = Rig::new(SID_MODEL_8580);
    let log = rig.log();
    assert_eq!(log[0], (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8));
    // The attack is instant by default: the chip's gate reset on, before any gate.
    assert_eq!(log[1], (GATE_RESET, 1));
    let regs: Vec<u8> = log[2..27].iter().map(|&(r, _)| r).collect();
    let set: BTreeSet<u8> = regs.iter().copied().collect();
    assert_eq!(regs.len(), 25);
    assert_eq!(set, (0..=0x18).collect());
    // Then every effect slot's five settings, all off.
    let slots: BTreeSet<(u8, u8)> = log[27..].iter().copied().collect();
    let want: BTreeSet<(u8, u8)> = (0..LIVE_SLOTS)
        .flat_map(|k| {
            (0..LIVE_SLOT_FIELDS).map(move |f| ((LIVE_REG_SLOT_BASE | k << 3 | f) as u8, 0))
        })
        .collect();
    assert_eq!(log.len(), 27 + 15);
    assert_eq!(slots, want);
}

#[test]
fn the_attack_mode_is_instant_at_init_and_kept_across_presets() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(rig.live.attack_mode, LIVE_ATTACK_INSTANT as u8);
    let get = |rig: &Rig| unsafe { live_param_get(&*rig.live, LIVE_PARAM_ATTACK_MODE) };
    assert_eq!(get(&rig), LIVE_ATTACK_INSTANT);
    assert_eq!(unsafe { live_param_max(LIVE_PARAM_ATTACK_MODE) }, 1);

    rig.clear();
    unsafe {
        live_param_set(
            &mut *rig.live,
            LIVE_PARAM_ATTACK_MODE,
            LIVE_ATTACK_AUTHENTIC,
        )
    };
    assert_eq!(
        rig.log(),
        &[(GATE_RESET, 0)],
        "one write: the chip's option off"
    );
    assert_eq!(get(&rig), LIVE_ATTACK_AUTHENTIC);
    rig.clear();
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_ATTACK_MODE, 5) }; // clamped to 1
    assert!(rig.log().is_empty(), "no change, no write");

    for p in 0..LIVE_PRESET_COUNT as u8 {
        rig.set_preset(p);
        assert_eq!(get(&rig), LIVE_ATTACK_AUTHENTIC, "preset {p} kept it");
    }
    assert!(rig.log().iter().all(|&(r, _)| r != GATE_RESET));

    rig.clear();
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_ATTACK_MODE, LIVE_ATTACK_INSTANT) };
    assert_eq!(rig.log(), &[(GATE_RESET, 1)]);
}

#[test]
fn every_chip_reset_is_followed_by_the_attack_mode_before_any_register() {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe {
        live_param_set(
            &mut *rig.live,
            LIVE_PARAM_ATTACK_MODE,
            LIVE_ATTACK_AUTHENTIC,
        )
    };
    rig.note_on(60);
    rig.clear();
    rig.button(LIVE_BUTTON_FX); // at once
    assert_eq!(rig.log()[0], (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8));
    assert_eq!(
        rig.log()[1],
        (GATE_RESET, 0),
        "the new chip is authentic too"
    );
    rig.clear();
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_MODEL, 0) };
    rig.tick(); // a model MIDI asked for
    assert_eq!(rig.log()[0], (LIVE_REG_RESET as u8, SID_MODEL_6581 as u8));
    assert_eq!(rig.log()[1], (GATE_RESET, 0));
    assert_eq!(
        rig.log().iter().filter(|&&(r, _)| r == GATE_RESET).count(),
        1
    );
}

#[test]
fn init_state_is_preset_0_octave_0_full_volume() {
    let rig = Rig::new(SID_MODEL_6581);
    let l = &*rig.live;
    let p = preset(0);
    assert_eq!(l.model, SID_MODEL_6581);
    assert_eq!((l.preset, l.octave), (0, 0));
    assert_eq!(l.cutoff, p.cutoff);
    assert_eq!(l.resonance, p.resonance);
    assert_eq!(l.pulse_width, p.pulse_width);
    assert_eq!((l.master, l.volume), (1023, 15));
    assert_eq!(rig.reg(0x18), p.filter_mode | 15);
    assert_eq!(rig.reg(0x17), p.resonance << 4 | p.filter_voices);
    assert_eq!(gates(&rig), [false; 3]);
}

#[test]
fn adsr_is_written_before_the_gate() {
    let mut rig = Rig::new(SID_MODEL_8580);
    let log = rig.log().to_vec();
    for v in 0..3u8 {
        let pos = |r: u8| log.iter().position(|&(x, _)| x == r).unwrap();
        assert!(pos(v * 7 + 5) < pos(v * 7 + 4));
        assert!(pos(v * 7 + 6) < pos(v * 7 + 4));
    }
    rig.clear();
}

// --- notes and gating ---

#[test]
fn note_on_sets_frequency_and_gate() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.clear();
    rig.note_on(69);
    let v = voice_of(&rig, 69);
    assert_eq!(rig.voice_freq(v), 0x1d45);
    assert_eq!(rig.control(v), preset(SAW_LEAD).control | 1);
    assert_eq!(rig.control(v) & 0xf0, SAW);
}

#[test]
fn note_off_releases_the_gate_and_keeps_the_waveform() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(60);
    let v = voice_of(&rig, 60);
    rig.clear();
    rig.note_off(60);
    assert_eq!(rig.log(), &[(v * 7 + 4, preset(SAW_LEAD).control)]);
}

#[test]
fn note_off_of_a_note_not_held_writes_nothing() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(60);
    rig.clear();
    rig.note_off(61);
    rig.note_off(200);
    assert!(rig.log().is_empty());
}

#[test]
fn velocity_zero_is_a_note_off() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(60);
    unsafe { live_note_on(&mut *rig.live, 60, 0) };
    assert_eq!(gates(&rig), [false; 3]);
}

#[test]
fn a_held_note_played_again_retriggers_its_own_voice() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(60);
    rig.note_on(62);
    let v = voice_of(&rig, 60);
    rig.clear();
    rig.note_on(60);
    assert_eq!(voice_of(&rig, 60), v);
    let ctrl: Vec<u8> = rig
        .log()
        .iter()
        .filter(|&&(r, _)| r == v * 7 + 4)
        .map(|&(_, x)| x & 1)
        .collect();
    assert_eq!(ctrl, [0, 1], "gate off then on restarts the attack");
    assert_eq!(gates(&rig).iter().filter(|&&g| g).count(), 2);
}

#[test]
fn notes_out_of_midi_range_are_ignored() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.clear();
    rig.note_on(128);
    rig.note_on(255);
    assert!(rig.log().is_empty());
}

// --- voice allocation ---

#[test]
fn three_notes_take_three_voices_in_order() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    assert_eq!(
        [voice_of(&rig, 60), voice_of(&rig, 64), voice_of(&rig, 67)],
        [0, 1, 2]
    );
}

#[test]
fn a_fourth_note_steals_the_oldest() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    rig.note_on(72);
    assert_eq!(voice_of(&rig, 72), 0);
    rig.note_on(76);
    assert_eq!(voice_of(&rig, 76), 1);
    rig.note_on(79);
    assert_eq!(voice_of(&rig, 79), 2);
    rig.note_on(84);
    assert_eq!(voice_of(&rig, 84), 0);
}

#[test]
fn stealing_retriggers_the_gate() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    rig.clear();
    rig.note_on(72);
    let ctrl: Vec<u8> = rig
        .log()
        .iter()
        .filter(|&&(r, _)| r == 4)
        .map(|&(_, x)| x & 1)
        .collect();
    assert_eq!(ctrl, [0, 1]);
}

#[test]
fn a_released_voice_is_taken_before_stealing() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    rig.note_off(64);
    rig.note_on(72);
    assert_eq!(voice_of(&rig, 72), 1);
    assert_eq!(voice_of(&rig, 60), 0);
    assert_eq!(voice_of(&rig, 67), 2);
}

#[test]
fn the_voice_released_longest_ago_is_taken_first() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    rig.note_off(67);
    rig.note_off(60);
    rig.note_on(72);
    assert_eq!(voice_of(&rig, 72), 2);
    rig.note_on(74);
    assert_eq!(voice_of(&rig, 74), 0);
}

#[test]
fn stealing_follows_note_on_order_not_voice_order() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    rig.note_off(60);
    rig.note_on(72); // voice 0, now the newest
    rig.note_on(74); // steals 64 on voice 1
    assert_eq!(voice_of(&rig, 74), 1);
    assert_eq!(voice_of(&rig, 72), 0);
}

#[test]
fn all_notes_off_releases_every_voice() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        rig.note_on(n);
    }
    unsafe { live_all_notes_off(&mut *rig.live) };
    assert_eq!(gates(&rig), [false; 3]);
}

// --- octave ---

#[test]
fn octave_buttons_stop_at_the_limits() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.clear();
    for _ in 0..10 {
        rig.button(LIVE_BUTTON_OCT_UP);
    }
    assert_eq!(rig.live.octave, LIVE_OCTAVE_MAX as i8);
    assert_eq!(rig.key_note(60), 60 + 12 * LIVE_OCTAVE_MAX as u8);
    for _ in 0..20 {
        rig.button(LIVE_BUTTON_OCT_DOWN);
    }
    assert_eq!(rig.live.octave, LIVE_OCTAVE_MIN as i8);
    assert_eq!(i32::from(rig.key_note(60)), 60 + 12 * LIVE_OCTAVE_MIN);
    assert!(rig.log().is_empty(), "the octave writes no registers");
}

#[test]
fn key_notes_outside_midi_range_are_no_note() {
    let mut rig = Rig::new(SID_MODEL_8580);
    assert_eq!(u32::from(rig.key_note(200)), LIVE_NO_NOTE);
    rig.button(LIVE_BUTTON_OCT_UP);
    assert_eq!(rig.key_note(115), 127);
    assert_eq!(u32::from(rig.key_note(116)), LIVE_NO_NOTE);
    rig.button(LIVE_BUTTON_OCT_DOWN);
    rig.button(LIVE_BUTTON_OCT_DOWN);
    assert_eq!(rig.key_note(12), 0);
    assert_eq!(u32::from(rig.key_note(11)), LIVE_NO_NOTE);
}

#[test]
fn the_fm1_key_range_stays_in_midi_range_at_every_octave() {
    let mut rig = Rig::new(SID_MODEL_8580);
    for _ in 0..3 {
        rig.button(LIVE_BUTTON_OCT_DOWN);
    }
    for _ in 0..=6 {
        // F3..G5
        for base in 53..=79 {
            assert_ne!(u32::from(rig.key_note(base)), LIVE_NO_NOTE);
        }
        rig.button(LIVE_BUTTON_OCT_UP);
    }
}

// --- presets ---

#[test]
fn there_are_four_named_presets() {
    assert_eq!(LIVE_PRESET_COUNT, 4);
    let names: BTreeSet<String> = (0..4).map(|i| preset_name(preset(i))).collect();
    assert_eq!(names.len(), 4);
    assert!(names.iter().all(|n| !n.is_empty() && n.len() <= 16));
    assert!(unsafe { live_preset_get(4) }.is_null());
}

#[test]
fn presets_are_saw_lead_pwm_bass_sync_lead_and_noise() {
    let saw = preset(SAW_LEAD);
    assert_eq!(saw.control & 0xf0, SAW);
    assert_eq!(saw.mono_sync, 0);

    let bass = preset(PULSE_BASS);
    assert_eq!(bass.control & 0xf0, PULSE);
    assert!(bass.pwm_depth > 0 && bass.pwm_rate > 0);

    let sync = preset(SYNC_LEAD);
    assert_eq!(sync.control & SYNC, SYNC);
    assert_eq!(sync.mono_sync, 1);
    assert!(sync.sync_ratio_q8 > 256);

    let noise = preset(NOISE_PERC);
    assert_eq!(noise.control & 0xf0, NOISE);
    assert_eq!(noise.sustain_release >> 4, 0, "percussion does not sustain");

    for i in 0..4 {
        let p = preset(i);
        assert_eq!(p.control & 1, 0, "preset {i} holds no gate");
        assert!(u32::from(p.cutoff) <= LIVE_CUTOFF_MAX);
        assert!(u32::from(p.resonance) <= LIVE_RESONANCE_MAX);
        assert!(u32::from(p.pulse_width) <= LIVE_PULSE_WIDTH_MAX);
        assert_eq!(p.filter_voices & !7, 0);
        assert_eq!(p.filter_mode & !0x70, 0);
    }
}

#[test]
fn preset_knob_switches_and_loads_the_preset() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_CUTOFF, -5);
    rig.note_on(60);
    rig.knob(LIVE_KNOB_PRESET, 1);
    let p = preset(PULSE_BASS);
    let l = &*rig.live;
    assert_eq!(l.preset, PULSE_BASS);
    assert_eq!(
        (l.cutoff, l.resonance, l.pulse_width),
        (p.cutoff, p.resonance, p.pulse_width)
    );
    assert_eq!(gates(&rig), [false; 3], "switching releases held notes");
    for v in 0..3 {
        assert_eq!(rig.control(v), p.control);
        assert_eq!(rig.reg(v * 7 + 5), p.attack_decay);
        assert_eq!(rig.reg(v * 7 + 6), p.sustain_release);
    }
    assert_eq!(
        u16::from(rig.reg(0x15) & 7) | u16::from(rig.reg(0x16)) << 3,
        p.cutoff
    );
    assert_eq!(rig.reg(0x17), p.resonance << 4 | p.filter_voices);
    assert_eq!(rig.reg(0x18), p.filter_mode | rig.live.volume);
}

#[test]
fn preset_knob_clamps_at_both_ends() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_PRESET, -1);
    assert_eq!(rig.live.preset, 0);
    rig.knob(LIVE_KNOB_PRESET, 100);
    assert_eq!(rig.live.preset, 3);
    rig.set_preset(200);
    assert_eq!(rig.live.preset, 3);
    rig.knob(LIVE_KNOB_PRESET, -2);
    assert_eq!(rig.live.preset, 1);
}

#[test]
fn the_volume_survives_a_preset_change() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_master(512);
    let vol = rig.live.volume;
    rig.set_preset(NOISE_PERC);
    assert_eq!(rig.live.volume, vol);
    assert_eq!(rig.reg(0x18) & 0x0f, vol);
}

#[test]
fn sync_lead_is_one_voice_synced_to_a_silent_voice_3() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(SYNC_LEAD);
    let p = preset(SYNC_LEAD);
    rig.note_on(57);
    let base = u32::from(freq_table()[57]);
    assert_eq!(u32::from(rig.voice_freq(2)), base);
    assert_eq!(
        u32::from(rig.voice_freq(0)),
        ((base * u32::from(p.sync_ratio_q8)) >> 8).min(0xffff)
    );
    assert_eq!(rig.control(0), p.control | 1);
    assert_eq!(
        rig.control(2) & 0xf1,
        0,
        "voice 3 has no waveform and no gate"
    );
    assert!(!rig.gated(1));

    rig.note_on(60);
    assert_eq!(
        u32::from(rig.voice_freq(2)),
        u32::from(freq_table()[60]),
        "a new note takes over"
    );
    assert!(!rig.gated(1) && !rig.gated(2));
    rig.note_off(57);
    assert!(rig.gated(0), "releasing the old note keeps the new one");
    rig.note_off(60);
    assert!(!rig.gated(0));
}

#[test]
fn sync_ratio_saturates_for_high_notes() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(SYNC_LEAD);
    rig.note_on(127);
    assert_eq!(rig.voice_freq(0), 0xffff);
}

// --- knobs ---

#[test]
fn one_cutoff_detent_is_16_steps() {
    let mut rig = Rig::new(SID_MODEL_8580);
    let c = rig.live.cutoff;
    rig.knob(LIVE_KNOB_CUTOFF, -1);
    assert_eq!(rig.live.cutoff, c - 16);
    rig.knob(LIVE_KNOB_CUTOFF, 3);
    assert_eq!(rig.live.cutoff, c + 32);
}

#[test]
fn cutoff_is_written_as_11_bits() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_CUTOFF, -1000);
    rig.cc(LIVE_CC_CUTOFF, 0);
    rig.knob(LIVE_KNOB_CUTOFF, 0x5a); // 0x5a * 16 = 0x5a0
    assert_eq!(rig.live.cutoff, 0x5a0);
    assert_eq!((rig.reg(0x15), rig.reg(0x16)), (0x00, 0xb4));
    rig.cc(LIVE_CC_CUTOFF, 127);
    assert_eq!(rig.live.cutoff, 2047);
    assert_eq!((rig.reg(0x15), rig.reg(0x16)), (0x07, 0xff));
}

#[test]
fn knobs_clamp_and_write_nothing_past_their_limits() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_CUTOFF, 1000);
    rig.knob(LIVE_KNOB_RESONANCE, 1000);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, 1000);
    let l = &*rig.live;
    assert_eq!(
        (
            u32::from(l.cutoff),
            u32::from(l.resonance),
            u32::from(l.pulse_width)
        ),
        (LIVE_CUTOFF_MAX, LIVE_RESONANCE_MAX, LIVE_PULSE_WIDTH_MAX)
    );
    rig.clear();
    rig.knob(LIVE_KNOB_CUTOFF, 1);
    rig.knob(LIVE_KNOB_RESONANCE, 1);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, i32::MAX);
    assert!(rig.log().is_empty());

    rig.knob(LIVE_KNOB_CUTOFF, i32::MIN);
    rig.knob(LIVE_KNOB_RESONANCE, -1000);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, -1000);
    let l = &*rig.live;
    assert_eq!((l.cutoff, l.resonance, l.pulse_width), (0, 0, 0));
    assert_eq!(rig.reg(0x17) >> 4, 0);
    assert_eq!(rig.voice_pw(0), 0);
}

#[test]
fn resonance_and_pulse_width_reach_the_registers() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_RESONANCE, -100);
    rig.knob(LIVE_KNOB_RESONANCE, 9);
    assert_eq!(rig.reg(0x17), 9 << 4 | preset(SAW_LEAD).filter_voices);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, -1000);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, 10);
    for v in 0..3 {
        assert_eq!(rig.voice_pw(v), 320);
    }
}

#[test]
fn master_sets_the_volume_nibble() {
    let mut rig = Rig::new(SID_MODEL_8580);
    let mode = preset(SAW_LEAD).filter_mode;
    rig.set_master(0);
    assert_eq!((rig.live.volume, rig.reg(0x18)), (0, mode));
    rig.set_master(512);
    assert_eq!(rig.live.volume, 8);
    rig.set_master(5000);
    assert_eq!(u32::from(rig.live.master), LIVE_MASTER_MAX);
    assert_eq!(rig.reg(0x18), mode | 15);
    rig.clear();
    rig.set_master(1020);
    assert!(
        rig.log().is_empty(),
        "no write while the nibble is unchanged"
    );
}

#[test]
fn ccs_drive_the_parameters() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.cc(LIVE_CC_CUTOFF, 0);
    assert_eq!(rig.live.cutoff, 0);
    rig.cc(LIVE_CC_RESONANCE, 127);
    assert_eq!(rig.live.resonance, 15);
    rig.cc(LIVE_CC_PULSE_WIDTH, 127);
    assert_eq!(u32::from(rig.live.pulse_width), LIVE_PULSE_WIDTH_MAX);
    rig.cc(LIVE_CC_VOLUME, 0);
    assert_eq!((rig.live.master, rig.live.volume), (0, 0));
    rig.cc(LIVE_CC_VOLUME, 127);
    assert_eq!(u32::from(rig.live.master), LIVE_MASTER_MAX);
    rig.note_on(60);
    rig.cc(LIVE_CC_ALL_NOTES_OFF, 0);
    assert_eq!(gates(&rig), [false; 3]);
    rig.note_on(60);
    rig.cc(LIVE_CC_ALL_SOUND_OFF, 0);
    assert_eq!(gates(&rig), [false; 3]);
    rig.clear();
    rig.cc(2, 64);
    rig.cc(200, 64);
    rig.cc(LIVE_CC_CUTOFF, 200);
    assert!(
        rig.log().is_empty(),
        "unknown CCs and bad values are ignored"
    );
}

// --- pulse-width modulation ---

#[test]
fn pwm_sweeps_the_pulse_width_around_the_knob() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PULSE_BASS);
    let p = preset(PULSE_BASS);
    let centre = i32::from(rig.live.pulse_width);
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for _ in 0..(LIVE_TICK_HZ * 10) {
        rig.tick();
        let pw = i32::from(rig.voice_pw(0));
        assert_eq!(rig.voice_pw(1), rig.voice_pw(0));
        lo = lo.min(pw);
        hi = hi.max(pw);
    }
    let depth = i32::from(p.pwm_depth);
    assert!(lo >= centre - depth && hi <= centre + depth);
    assert!(hi - lo > depth, "the sweep covers most of its range");
}

#[test]
fn pwm_clamps_at_the_pulse_width_limits() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PULSE_BASS);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, 1000);
    for _ in 0..(LIVE_TICK_HZ * 10) {
        rig.tick();
        assert!(u32::from(rig.voice_pw(0)) <= LIVE_PULSE_WIDTH_MAX);
    }
}

#[test]
fn presets_without_pwm_write_nothing_on_tick() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.clear();
    for _ in 0..100 {
        rig.tick();
    }
    assert!(rig.log().is_empty());
}

// --- chip model ---

#[test]
fn fx_toggles_the_model_and_keeps_every_parameter() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.set_preset(PULSE_BASS);
    rig.knob(LIVE_KNOB_CUTOFF, -7);
    rig.knob(LIVE_KNOB_RESONANCE, 3);
    rig.knob(LIVE_KNOB_PULSE_WIDTH, -4);
    rig.set_master(700);
    rig.button(LIVE_BUTTON_OCT_UP);
    rig.note_on(48);
    let before = *rig.live;
    let regs_before = rig.rec.regs;
    rig.clear();

    rig.button(LIVE_BUTTON_FX);
    let l = &*rig.live;
    assert_eq!(l.model, SID_MODEL_8580);
    assert_eq!(
        (
            l.preset,
            l.octave,
            l.cutoff,
            l.resonance,
            l.pulse_width,
            l.master,
            l.volume
        ),
        (
            before.preset,
            before.octave,
            before.cutoff,
            before.resonance,
            before.pulse_width,
            before.master,
            before.volume
        )
    );
    let log = rig.log();
    assert_eq!(log[0], (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8));
    assert_eq!(log[1], (GATE_RESET, 1));
    assert_eq!(
        log.len(),
        27,
        "a reset, the attack mode, then every register"
    );
    assert_eq!(rig.rec.regs, regs_before, "the same register values");
    assert!(rig.gated(voice_of(&rig, 48)), "held notes sound again");

    rig.button(LIVE_BUTTON_FX);
    assert_eq!(rig.live.model, SID_MODEL_6581);
    assert_eq!(rig.log()[27], (LIVE_REG_RESET as u8, SID_MODEL_6581 as u8));
}

// --- with a real chip ---

struct Chip {
    chip: Box<sid>,
    target: Box<live_sid_target>,
    live: Box<live>,
}

impl Chip {
    fn new(model: sid_model) -> Self {
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, 44100) };
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: 44100,
        });
        let mut l = Box::<live>::default();
        let ctx: *mut live_sid_target = &mut *target;
        unsafe { live_init(&mut *l, Some(live_sid_sink), ctx.cast::<c_void>(), model) };
        Chip {
            chip,
            target,
            live: l,
        }
    }

    fn render(&mut self, n: usize) -> Vec<i16> {
        let mut out = vec![0i16; n];
        unsafe { sid_render(&mut *self.chip, out.as_mut_ptr(), n as u32) };
        out
    }
}

fn peak(v: &[i16]) -> i32 {
    v.iter().map(|&x| i32::from(x).abs()).max().unwrap_or(0)
}

/// Rising zero crossings per second, a rough pitch.
fn pitch(v: &[i16], rate: f64) -> f64 {
    let n = v.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
    n as f64 * rate / v.len() as f64
}

/// SAW LEAD's note, held and released, then the same note again `phase` samples later:
/// the samples until its envelope leaves 0, at most `limit`.
fn retrigger_samples(mode: u32, phase: usize, limit: usize) -> Option<usize> {
    let mut c = Chip::new(SID_MODEL_6581);
    unsafe { live_param_set(&mut *c.live, LIVE_PARAM_ATTACK_MODE, mode) };
    unsafe { live_note_on(&mut *c.live, 60, 100) };
    c.render(4410);
    unsafe { live_note_off(&mut *c.live, 60) };
    c.render(44100 + phase); // release 6 from sustain 10: about 0.6 s to 0
    assert!(c.chip.env.iter().all(|e| e.counter == 0), "released");
    unsafe { live_note_on(&mut *c.live, 60, 100) };
    (1..=limit).find(|_| {
        c.render(1);
        c.chip.env.iter().any(|e| e.counter != 0)
    })
}

#[test]
fn an_instant_attack_starts_every_retrigger_at_once_and_authentic_waits() {
    // Release 6's period is 267 cycles, about 12 samples at 44.1 kHz: two periods of phases.
    let phases = 0..24;
    let instant: Vec<_> = phases
        .clone()
        .map(|k| retrigger_samples(LIVE_ATTACK_INSTANT, k, 2000).unwrap())
        .collect();
    assert!(instant.iter().all(|&n| n <= 1), "{instant:?}");
    // The ADSR delay: most phases wait for the rate LFSR, about 33 ms.
    let authentic: Vec<_> = phases
        .map(|k| retrigger_samples(LIVE_ATTACK_AUTHENTIC, k, 2000).unwrap())
        .collect();
    let waited = authentic.iter().filter(|&&n| n > 1400).count();
    assert!(waited >= 20, "{authentic:?}");
}

#[test]
fn the_sid_sink_plays_a_note_at_its_pitch() {
    let mut c = Chip::new(SID_MODEL_8580);
    c.render(4410);
    unsafe { live_cc(&mut *c.live, LIVE_CC_CUTOFF as u8, 127) };
    unsafe { live_note_on(&mut *c.live, 57, 100) }; // A3, 220 Hz
    let out = c.render(44100);
    assert!(peak(&out) > 1000);
    let hz = pitch(&out[4410..], 44100.0);
    assert!((hz - 220.0).abs() < 5.0, "{hz} Hz");
}

#[test]
fn the_sid_sink_reinitialises_with_the_new_model_and_keeps_parameters() {
    let mut c = Chip::new(SID_MODEL_6581);
    assert_eq!(c.target.clock_hz, SID_CLOCK_PAL);
    unsafe {
        live_knob(&mut *c.live, LIVE_KNOB_CUTOFF, -10);
        live_knob(&mut *c.live, LIVE_KNOB_RESONANCE, 5);
        live_note_on(&mut *c.live, 69, 100);
    }
    let fc = c.chip.fc;
    let res = c.chip.res;
    c.render(1000);
    unsafe { live_button(&mut *c.live, LIVE_BUTTON_FX) };
    assert_eq!(c.chip.model, SID_MODEL_8580);
    assert_eq!((c.chip.fc, c.chip.res), (fc, res));
    assert_eq!(u32::from(c.live.cutoff), fc);
    assert_eq!(c.chip.osc[0].freq, 0x1d45);
    assert_eq!(
        unsafe { (*c.chip.dac).wave[0x800] },
        0.0,
        "8580 DACs (zero at 0x800) after re-init"
    );
    assert!(peak(&c.render(4410)) > 1000, "the held note sounds again");
}

#[test]
fn stealing_a_sustained_voice_restarts_its_envelope_attack() {
    let mut c = Chip::new(SID_MODEL_8580);
    for n in [60, 64, 67] {
        unsafe { live_note_on(&mut *c.live, n, 100) };
    }
    c.render(22050);
    let sustain = preset(SAW_LEAD).sustain_release >> 4;
    let env = c.chip.env[0];
    assert_eq!(env.state, SID_ENV_DECAY_SUSTAIN);
    assert_eq!(
        env.counter,
        sustain * 0x11,
        "voice 1 sits at its sustain level"
    );

    // Gate off and on with no chip cycles in between: the edge must still latch.
    unsafe { live_note_on(&mut *c.live, 72, 100) };
    assert_eq!(c.chip.env[0].gate, 1);
    // The state follows the gate two cycles later, as on the chip (reSIDfp's pipeline).
    unsafe { sid_clock(&mut *c.chip, 2) };
    let env = c.chip.env[0];
    assert_eq!(env.state, SID_ENV_ATTACK);
    assert_eq!(c.chip.osc[0].freq, u32::from(freq_table()[72]));
    assert_eq!(
        c.chip.env[1].state, SID_ENV_DECAY_SUSTAIN,
        "others untouched"
    );
}

#[test]
fn a_preset_change_counts_as_releasing_the_held_notes_now() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.note_on(60); // voice 0
    rig.note_on(64); // voice 1
    rig.note_off(60); // voice 0 released first
    rig.set_preset(PULSE_BASS); // voice 1 released by the switch, after voice 0
    rig.note_on(48);
    assert_eq!(voice_of(&rig, 48), 2, "voice 3 never played");
    rig.note_on(50);
    assert_eq!(voice_of(&rig, 50), 0);
    rig.note_on(52);
    assert_eq!(voice_of(&rig, 52), 1);
}
