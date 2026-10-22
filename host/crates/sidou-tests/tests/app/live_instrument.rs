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
/* A reset comes after the character's settings (live.h): the profile, then its four
 * controls. */
const CHARACTER: usize = LIVE_CHAR_FIELDS as usize;

/// The character's settings live sends before a reset: the profile, the controls at 0.
fn character_log(profile: u32) -> Vec<(u8, u8)> {
    let mut v = vec![(LIVE_REG_CHAR_FIRST as u8, profile as u8)];
    v.extend((1..LIVE_CHAR_FIELDS).map(|f| ((LIVE_REG_CHAR_FIRST + f) as u8, 0)));
    v
}

#[test]
fn init_resets_the_chip_then_writes_every_register_once() {
    let rig = Rig::new(SID_MODEL_8580);
    let log = rig.log();
    // The character first: the model's default profile, every control at 0.
    assert_eq!(log[..CHARACTER], character_log(SID_CHIP_8580_R5)[..]);
    let log = &log[CHARACTER..];
    assert_eq!(log[0], (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8));
    // The envelope is instant by default: the chip's gate reset on, before any gate.
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
fn the_envelope_mode_is_instant_at_init_and_kept_across_presets() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(rig.live.envelope_mode, LIVE_ENVELOPE_INSTANT as u8);
    let get = |rig: &Rig| unsafe { live_param_get(&*rig.live, LIVE_PARAM_ENVELOPE_MODE) };
    assert_eq!(get(&rig), LIVE_ENVELOPE_INSTANT);
    assert_eq!(unsafe { live_param_max(LIVE_PARAM_ENVELOPE_MODE) }, 1);

    rig.clear();
    unsafe {
        live_param_set(
            &mut *rig.live,
            LIVE_PARAM_ENVELOPE_MODE,
            LIVE_ENVELOPE_AUTHENTIC,
        )
    };
    assert_eq!(
        rig.log(),
        &[(GATE_RESET, 0)],
        "one write: the chip's option off"
    );
    assert_eq!(get(&rig), LIVE_ENVELOPE_AUTHENTIC);
    rig.clear();
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_ENVELOPE_MODE, 5) }; // clamped to 1
    assert!(rig.log().is_empty(), "no change, no write");

    for p in 0..LIVE_PRESET_COUNT as u8 {
        rig.set_preset(p);
        assert_eq!(get(&rig), LIVE_ENVELOPE_AUTHENTIC, "preset {p} kept it");
    }
    assert!(rig.log().iter().all(|&(r, _)| r != GATE_RESET));

    rig.clear();
    unsafe {
        live_param_set(
            &mut *rig.live,
            LIVE_PARAM_ENVELOPE_MODE,
            LIVE_ENVELOPE_INSTANT,
        )
    };
    assert_eq!(rig.log(), &[(GATE_RESET, 1)]);
}

#[test]
fn every_chip_reset_is_followed_by_the_envelope_mode_before_any_register() {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe {
        live_param_set(
            &mut *rig.live,
            LIVE_PARAM_ENVELOPE_MODE,
            LIVE_ENVELOPE_AUTHENTIC,
        )
    };
    rig.note_on(60);
    rig.clear();
    rig.button(LIVE_BUTTON_FX); // at once
    assert_eq!(rig.log()[..CHARACTER], character_log(SID_CHIP_8580_R5)[..]);
    assert_eq!(
        rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8)
    );
    assert_eq!(
        rig.log()[CHARACTER + 1],
        (GATE_RESET, 0),
        "the new chip is authentic too"
    );
    rig.clear();
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_MODEL, 0) };
    rig.tick(); // a model MIDI asked for
    assert_eq!(
        rig.log()[..CHARACTER],
        character_log(SID_CHIP_6581_R4AR)[..]
    );
    assert_eq!(
        rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_6581 as u8)
    );
    assert_eq!(rig.log()[CHARACTER + 1], (GATE_RESET, 0));
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
    assert_eq!((l.master, l.volume), (1023, 100));
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
    assert!(
        !unsafe { live_preset_get(4) }.is_null(),
        "DICE, after the four"
    );
    assert!(unsafe { live_preset_get(5) }.is_null());
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
    rig.preset_step(1);
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
    assert_eq!(rig.reg(0x18), p.filter_mode | LIVE_CHIP_VOLUME as u8);
}

#[test]
fn preset_knob_clamps_at_both_ends() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.preset_step(-1);
    assert_eq!(rig.live.preset, 0);
    rig.preset_step(100);
    assert_eq!(rig.live.preset, 4, "DICE is the last");
    rig.set_preset(200);
    assert_eq!(rig.live.preset, 4);
    rig.preset_step(-2);
    assert_eq!(rig.live.preset, 2);
}

#[test]
fn the_volume_survives_a_preset_change() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_master(512);
    let vol = rig.live.volume;
    rig.set_preset(NOISE_PERC);
    assert_eq!(rig.live.volume, vol);
    assert_eq!(rig.live.master, 512);
    // MASTER is a digital gain after the chips (app/master_gain.h): the register stays at 15
    assert_eq!(u32::from(rig.reg(0x18) & 0x0f), LIVE_CHIP_VOLUME);
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

fn k1_table(model: sid_model) -> [u16; LIVE_CUTOFF_STEPS as usize] {
    let all = unsafe { live_cutoff_steps };
    all[model as usize]
}

#[test]
fn k1_walks_the_table_and_snaps_once() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let t = k1_table(model);
        let mut rig = Rig::new(model);
        // From the preset's 1200, which the table need not hold: +1 lands above it and on the
        // table, -1 below it and on the table.
        let c = rig.live.cutoff;
        assert_eq!(c, 1200);
        rig.knob(LIVE_KNOB_CUTOFF, 1);
        assert!(
            rig.live.cutoff > c && t.contains(&rig.live.cutoff),
            "{model}"
        );
        let up = rig.live.cutoff;
        rig.set_preset(0);
        rig.knob(LIVE_KNOB_CUTOFF, -1);
        assert!(
            rig.live.cutoff < c && t.contains(&rig.live.cutoff),
            "{model}"
        );
        let down = rig.live.cutoff;
        assert!(down < up);
        // On the table, each detent is the next entry, and back is back.
        let at = t.iter().position(|&e| e == down).unwrap();
        rig.knob(LIVE_KNOB_CUTOFF, 3);
        assert_eq!(rig.live.cutoff, t[at + 3], "{model}");
        rig.knob(LIVE_KNOB_CUTOFF, -3);
        assert_eq!(rig.live.cutoff, down, "{model}");
        // 64 detents span the table, both ways, and stay at its ends.
        rig.knob(LIVE_KNOB_CUTOFF, 64);
        assert_eq!(rig.live.cutoff, t[64], "{model}");
        rig.knob(LIVE_KNOB_CUTOFF, 1);
        assert_eq!(rig.live.cutoff, t[64], "{model}");
        rig.knob(LIVE_KNOB_CUTOFF, -64);
        assert_eq!(rig.live.cutoff, t[0], "{model}");
        // A cutoff MIDI put between two entries is left in the turn's direction.
        for between in [t[10] + 1, t[30] + 1, t[50] - 1] {
            for (steps, ok) in [(1, true), (-1, false)] {
                unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_CUTOFF, u32::from(between)) };
                rig.knob(LIVE_KNOB_CUTOFF, steps);
                let c = rig.live.cutoff;
                assert!(t.contains(&c), "{model} {between}: {c}");
                assert_eq!(c > between, ok, "{model} {between} {steps}: {c}");
            }
        }
        // Past the table's ends (MIDI): a turn outwards leaves the cutoff, a turn inwards walks
        // the table from there.
        unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_CUTOFF, 0) };
        rig.knob(LIVE_KNOB_CUTOFF, -1);
        assert_eq!(rig.live.cutoff, 0, "{model}");
        rig.knob(LIVE_KNOB_CUTOFF, 1);
        assert!(
            rig.live.cutoff > 0 && t.contains(&rig.live.cutoff),
            "{model}"
        );
        unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_CUTOFF, 2047) };
        rig.knob(LIVE_KNOB_CUTOFF, 1);
        assert_eq!(rig.live.cutoff, 2047, "{model}");
        rig.knob(LIVE_KNOB_CUTOFF, -1);
        assert_eq!(rig.live.cutoff, t[63], "{model}");
    }
}

#[test]
fn cutoff_is_written_as_11_bits() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_CUTOFF, -1000);
    rig.cc(LIVE_CC_CUTOFF, 0);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_CUTOFF, 0x5a0) };
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
    rig.pulse(1000);
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
    rig.pulse(i32::MAX);
    assert!(rig.log().is_empty());

    rig.knob(LIVE_KNOB_CUTOFF, i32::MIN);
    rig.knob(LIVE_KNOB_RESONANCE, -1000);
    rig.pulse(-1000);
    let l = &*rig.live;
    assert_eq!(
        (l.cutoff, l.resonance, l.pulse_width),
        (k1_table(SID_MODEL_8580)[0], 0, 0),
        "K1 stops at the table's first entry"
    );
    assert_eq!(rig.reg(0x17) >> 4, 0);
    assert_eq!(rig.voice_pw(0), 0);
}

#[test]
fn resonance_and_pulse_width_reach_the_registers() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.knob(LIVE_KNOB_RESONANCE, -100);
    rig.knob(LIVE_KNOB_RESONANCE, 9);
    assert_eq!(rig.reg(0x17), 9 << 4 | preset(SAW_LEAD).filter_voices);
    rig.pulse(-1000);
    rig.pulse(10);
    for v in 0..3 {
        assert_eq!(rig.voice_pw(v), 320);
    }
}

/// MASTER no longer touches the chip: its volume register stays at 15 (a digital gain after the
/// chips scales the output, app/master_gain.h, so a song's own $D418 writes keep their meaning),
/// and the displayed volume, the pot's travel in percent, follows.
#[test]
fn master_leaves_the_chips_volume_register_alone() {
    let mut rig = Rig::new(SID_MODEL_8580);
    let mode = preset(SAW_LEAD).filter_mode;
    rig.clear();
    rig.set_master(0);
    assert_eq!((rig.live.volume, rig.live.master), (0, 0));
    rig.set_master(512);
    assert_eq!(rig.live.volume, 50);
    rig.set_master(5000);
    assert_eq!(u32::from(rig.live.master), LIVE_MASTER_MAX);
    assert_eq!(rig.live.volume, 100);
    rig.set_master(1020);
    assert!(
        rig.log().is_empty(),
        "MASTER writes no register: {:?}",
        rig.log()
    );
    assert_eq!(u32::from(rig.reg(0x18)), u32::from(mode) | LIVE_CHIP_VOLUME);
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
    rig.pulse(1000);
    for _ in 0..(LIVE_TICK_HZ * 10) {
        rig.tick();
        assert!(u32::from(rig.voice_pw(0)) <= LIVE_PULSE_WIDTH_MAX);
    }
}

/// Ergonomics review E09: on PWM BASS at K3's narrow positions the LFO swept the width below 0,
/// clamped it there, and a width of 0 is a DC level: the note went silent for part of every
/// cycle (a quarter of it at 9 %). The sweep now narrows near the ends and never goes thinner
/// than K3's narrowest width, at every K3 position and at the wide end.
#[test]
fn pwm_never_sweeps_thinner_than_k3s_narrowest_width() {
    let mut rig = Rig::new(SID_MODEL_8580);
    rig.set_preset(PULSE_BASS);
    let depth = i32::from(preset(PULSE_BASS).pwm_depth);
    let widths = (0..LIVE_PULSE_POSITIONS as i32).map(|k| LIVE_PULSE_FIRST as i32 + 64 * k);
    for base in widths.chain([4096 - 300, 4095]) {
        unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_PULSE_WIDTH, base as u32) };
        let (mut lo, mut hi) = (i32::MAX, i32::MIN);
        for _ in 0..(LIVE_TICK_HZ * 3) {
            rig.tick();
            let pw = i32::from(rig.voice_pw(0));
            lo = lo.min(pw);
            hi = hi.max(pw);
        }
        let floor = LIVE_PULSE_FIRST as i32;
        if base <= 4096 - floor {
            assert!(lo >= floor.min(base), "width {base}: down to {lo}");
            assert!(hi <= 4096 - floor, "width {base}: up to {hi}");
        }
        let room = (base - floor).min(4096 - floor - base).max(0);
        assert!(
            hi - lo >= depth.min(room) * 3 / 2,
            "width {base}: the sweep {lo}..{hi} keeps the room it has"
        );
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
    rig.pulse(-4);
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
    assert_eq!(log[..CHARACTER], character_log(SID_CHIP_8580_R5)[..]);
    assert_eq!(log[CHARACTER], (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8));
    assert_eq!(log[CHARACTER + 1], (GATE_RESET, 1));
    assert_eq!(
        log.len(),
        CHARACTER + 27,
        "the character, a reset, the envelope mode, then every register"
    );
    assert_eq!(rig.rec.regs, regs_before, "the same register values");
    assert!(rig.gated(voice_of(&rig, 48)), "held notes sound again");

    rig.button(LIVE_BUTTON_FX);
    assert_eq!(rig.live.model, SID_MODEL_6581);
    let back = &rig.log()[CHARACTER + 27..];
    assert_eq!(back[..CHARACTER], character_log(SID_CHIP_6581_R4AR)[..]);
    assert_eq!(
        back[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_6581 as u8)
    );
}

// --- with a real chip ---

struct Chip {
    chip: Box<sid>,
    target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    live: Box<live>,
}

impl Chip {
    fn new(model: sid_model) -> Self {
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, 44100) };
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: 44100,
            dacs: dacs.as_mut_ptr(),
            ..Default::default()
        });
        let mut l = Box::<live>::default();
        let ctx: *mut live_sid_target = &mut *target;
        unsafe { live_init(&mut *l, Some(live_sid_sink), ctx.cast::<c_void>(), model) };
        Chip {
            chip,
            target,
            _dacs: dacs,
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
    unsafe { live_param_set(&mut *c.live, LIVE_PARAM_ENVELOPE_MODE, mode) };
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
        .map(|k| retrigger_samples(LIVE_ENVELOPE_INSTANT, k, 2000).unwrap())
        .collect();
    assert!(instant.iter().all(|&n| n <= 1), "{instant:?}");
    // The ADSR delay: most phases wait for the rate LFSR, about 33 ms.
    let authentic: Vec<_> = phases
        .map(|k| retrigger_samples(LIVE_ENVELOPE_AUTHENTIC, k, 2000).unwrap())
        .collect();
    let waited = authentic.iter().filter(|&&n| n > 1400).count();
    assert!(waited >= 20, "{authentic:?}");
}

/// SAW LEAD's note held into sustain, then let go `phase` samples later: the samples until
/// its envelope leaves the sustain level, at most `limit`.
fn release_samples(mode: u32, phase: usize, limit: usize) -> Option<usize> {
    let mut c = Chip::new(SID_MODEL_6581);
    unsafe { live_param_set(&mut *c.live, LIVE_PARAM_ENVELOPE_MODE, mode) };
    unsafe { live_note_on(&mut *c.live, 60, 100) };
    c.render(22050 + phase); // decay 9 to the sustain level: under 100 ms
    let v = c.chip.env.iter().position(|e| e.gate != 0).unwrap();
    let sustain = c.chip.env[v].counter;
    assert_eq!(
        (c.chip.env[v].state, sustain),
        (SID_ENV_DECAY_SUSTAIN, c.chip.env[v].sustain),
        "holding sustain"
    );
    unsafe { live_note_off(&mut *c.live, 60) };
    (1..=limit).find(|_| {
        c.render(1);
        c.chip.env[v].counter != sustain
    })
}

#[test]
fn an_instant_envelope_releases_every_note_off_at_once_and_authentic_waits() {
    // Decay 9's period is 977 cycles, about 44 samples: a note-off at each of its phases.
    // Release 6's first step comes 267 + 3 cycles after a restarted rate counter: 13
    // samples at most.
    let phases = 0..44;
    let instant: Vec<_> = phases
        .clone()
        .map(|k| release_samples(LIVE_ENVELOPE_INSTANT, k, 2000).unwrap())
        .collect();
    assert!(instant.iter().all(|&n| n <= 13), "{instant:?}");
    // The ADSR delay on release: a note-off that finds the rate LFSR past release 6's
    // position (710 of the 977 phases) waits for it to come round, about 33 ms.
    let authentic: Vec<_> = phases
        .map(|k| release_samples(LIVE_ENVELOPE_AUTHENTIC, k, 2000).unwrap())
        .collect();
    let waited = authentic.iter().filter(|&&n| n > 1400).count();
    assert_eq!(waited, 32, "{authentic:?}"); // 73 %, as 710 / 977
    assert!(
        authentic.iter().all(|&n| n <= 13 || n > 1400),
        "{authentic:?}"
    );
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

/// A profile step is a chip reset, as a model change: the held note's gate is written again on
/// the new chip, so it attacks again rather than stopping.
#[test]
fn a_profile_change_reattacks_the_held_notes_on_the_new_chip() {
    let mut c = Chip::new(SID_MODEL_6581);
    unsafe { live_note_on(&mut *c.live, 69, 100) };
    c.render(4410);
    unsafe {
        live_param_set(&mut *c.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
        live_tick(&mut *c.live);
    }
    assert_eq!(u32::from(c.live.profile), SID_CHIP_6581_WORN);
    assert_ne!(c.chip.env[0].gate, 0, "its gate is set on the new chip");
    c.render(10);
    assert_eq!(
        c.chip.env[0].state, SID_ENV_ATTACK,
        "the held note attacks again"
    );
    assert!(
        peak(&c.render(4410)) > 1000,
        "the held note sounds on the new chip"
    );
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

/// live_params, MIDI out's snapshot of every parameter at once, reads what live_param_get
/// does, parameter by parameter, over random settings of every parameter and preset.
#[test]
fn live_params_reads_every_parameter_as_live_param_get_does() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut seed = 0x5eed_9a7au32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for round in 0..2000 {
        if round % 100 == 0 {
            rig.set_preset((next() % LIVE_PRESET_COUNT) as u8);
        }
        let p = next() % LIVE_PARAM_COUNT;
        let max = unsafe { live_param_max(p) };
        unsafe { live_param_set(&mut *rig.live, p, next() % (max + 1)) };
        let mut all = [0u16; LIVE_PARAM_COUNT as usize];
        unsafe { live_params(&*rig.live, all.as_mut_ptr()) };
        let each: Vec<u16> = (0..LIVE_PARAM_COUNT)
            .map(|p| unsafe { live_param_get(&*rig.live, p) } as u16)
            .collect();
        assert_eq!(all.to_vec(), each, "round {round}, after parameter {p}");
    }
}

// --- the character (M1c Task 3) ---

fn set_param(rig: &mut Rig, p: u32, v: u32) {
    unsafe { live_param_set(&mut *rig.live, p, v) };
}

fn get_param(rig: &Rig, p: u32) -> u32 {
    unsafe { live_param_get(&*rig.live, p) }
}

/// The controls are recorded and the tick sends what changed, once, then the character's tick
/// (LIVE_REG_CHAR_TICK, which the Recorder counts), every tick: the sink ramps the chip
/// towards them. No reset, and no $17: the stage's commit sets the filter's coefficients.
#[test]
fn the_character_controls_change_the_chip_at_the_tick_without_a_reset() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.clear();
    let ticks = rig.rec.char_ticks;
    set_param(&mut rig, LIVE_PARAM_AGE, 40);
    set_param(&mut rig, LIVE_PARAM_BLEED, 9);
    set_param(&mut rig, LIVE_PARAM_BLEED, 10); // the last one wins
    assert!(
        rig.log().is_empty(),
        "applied before the tick: {:?}",
        rig.log()
    );
    assert_eq!(rig.rec.char_ticks, ticks, "a tick before live_tick");
    assert_eq!(
        get_param(&rig, LIVE_PARAM_BLEED),
        10,
        "the request reads back"
    );
    rig.tick();
    let age = (LIVE_REG_CHAR_FIRST + 1) as u8;
    let bleed = (LIVE_REG_CHAR_FIRST + 4) as u8;
    assert_eq!(rig.log(), &[(age, 40), (bleed, 10)]);
    assert_eq!(rig.rec.char_ticks, ticks + 1);
    rig.clear();
    rig.tick();
    assert!(rig.log().is_empty(), "sent twice: {:?}", rig.log());
    assert_eq!(rig.rec.char_ticks, ticks + 2, "the tick goes every tick");
    set_param(&mut rig, LIVE_PARAM_THUMP, 5);
    rig.tick();
    let thump = (LIVE_REG_CHAR_FIRST + 3) as u8;
    assert_eq!(rig.log(), &[(thump, 5)]);
    assert_eq!(rig.live.model, SID_MODEL_6581);
}

/// A profile is a chip and a model: it resets the chip at the tick, the model follows it, and
/// the model goes back to the last 6581 profile asked for.
#[test]
fn a_profile_resets_the_chip_and_the_model_follows_it() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(get_param(&rig, LIVE_PARAM_PROFILE), SID_CHIP_6581_R4AR);
    set_param(&mut rig, LIVE_PARAM_AGE, 7);
    set_param(&mut rig, LIVE_PARAM_PROFILE, SID_CHIP_8580_R5);
    assert_eq!(
        get_param(&rig, LIVE_PARAM_MODEL),
        1,
        "the model follows the request"
    );
    rig.clear();
    rig.tick();
    let mut want = character_log(SID_CHIP_8580_R5);
    want[1].1 = 7; // the age, sent with the reset
    assert_eq!(rig.log()[..CHARACTER], want[..]);
    assert_eq!(
        rig.log()[CHARACTER],
        (LIVE_REG_RESET as u8, SID_MODEL_8580 as u8)
    );
    assert_eq!(rig.live.model, SID_MODEL_8580);
    // Worn, then the model to the 8580 and back: worn again.
    set_param(&mut rig, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN);
    rig.tick();
    set_param(&mut rig, LIVE_PARAM_MODEL, 1);
    rig.tick();
    assert_eq!(get_param(&rig, LIVE_PARAM_PROFILE), SID_CHIP_8580_R5);
    set_param(&mut rig, LIVE_PARAM_MODEL, 0);
    rig.tick();
    assert_eq!(get_param(&rig, LIVE_PARAM_PROFILE), SID_CHIP_6581_WORN);
    assert_eq!(rig.live.profile, SID_CHIP_6581_WORN as u8);
    // The FX button too.
    rig.button(LIVE_BUTTON_FX);
    assert_eq!(rig.live.profile, SID_CHIP_8580_R5 as u8);
    rig.button(LIVE_BUTTON_FX);
    assert_eq!(rig.live.profile, SID_CHIP_6581_WORN as u8);
    // A preset keeps the character.
    rig.set_preset(1);
    assert_eq!(get_param(&rig, LIVE_PARAM_PROFILE), SID_CHIP_6581_WORN);
    assert_eq!(get_param(&rig, LIVE_PARAM_AGE), 7);
}

// --- effect slots picked from the panel ---

#[test]
fn a_silent_slot_picks_up_a_musical_start() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for slot in 0..LIVE_SLOTS {
        let s = slot as usize;
        assert_eq!(rig.live.slot[s], [0; 5]);
        for alg in 1..unsafe { live_slot_algorithms(slot) } {
            unsafe { live_slot_pick(&mut *rig.live, slot, alg) };
            let d = unsafe { live_fx_defaults[s][alg as usize] };
            assert_eq!(
                rig.live.slot[s],
                [alg as u8, d[0], d[1], d[2], d[3]],
                "slot {slot} algorithm {alg}: each algorithm its own start while untouched"
            );
            assert!(d[0] > 0 && d[3] > 0, "audible: Amount and Mix above 0");
        }
        unsafe { live_slot_pick(&mut *rig.live, slot, 0) };
        assert_eq!(rig.live.slot[s][0], 0);
    }
    // ENV's step is the same pick.
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe { live_slot_next_algorithm(&mut *rig.live, 0) };
    assert_eq!(rig.live.slot[0], [1, 80, 0, 0, 127]);
}

#[test]
fn an_edited_slot_keeps_its_values() {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe {
        live_slot_pick(&mut *rig.live, 1, SID_MOD_CHORUS);
        live_slot_turn(&mut *rig.live, 1, LIVE_SLOT_A, 2);
    }
    let edited = rig.live.slot[1];
    unsafe { live_slot_pick(&mut *rig.live, 1, SID_MOD_PHASER) };
    assert_eq!(rig.live.slot[1][0], SID_MOD_PHASER as u8);
    assert_eq!(
        &rig.live.slot[1][1..],
        &edited[1..],
        "the player's controls stay"
    );
    // Off and on again: still the player's.
    unsafe { live_slot_pick(&mut *rig.live, 1, 0) };
    unsafe { live_slot_pick(&mut *rig.live, 1, SID_MOD_FLANGER) };
    assert_eq!(&rig.live.slot[1][1..], &edited[1..]);
}

#[test]
fn a_midi_algorithm_loads_nothing() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.cc(LIVE_CC_SLOT_FIRST + LIVE_SLOT_FIELDS, 40);
    assert_ne!(rig.live.slot[1][0], 0);
    assert_eq!(&rig.live.slot[1][1..], &[0; 4]);
    for slot in 0..LIVE_SLOTS {
        for v in (0..128u8).step_by(9) {
            rig.cc(LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS, v);
            assert_eq!(&rig.live.slot[slot as usize][1..], &[0; 4]);
        }
    }
}

// --- K3 and K4 on LIVE: the sound's own control and the release ---

const PWM_BASS_PRESET: u8 = 1;
const SYNC_LEAD_PRESET: u8 = 2;
const NOISE_PERC_PRESET: u8 = 3;

fn tone(l: &live) -> u32 {
    unsafe { live_tone(l) }
}

#[test]
fn k3_follows_the_sound() {
    let mut rig = Rig::new(SID_MODEL_6581);
    // SAW LEAD and NOISE PERC: the decay (a nibble of the attack/decay register, 9 and 8).
    for (preset, decay) in [(SAW_LEAD, 9), (NOISE_PERC_PRESET, 8)] {
        rig.set_preset(preset);
        assert_eq!(tone(&rig.live), LIVE_TONE_DECAY, "preset {preset}");
        assert_eq!(rig.live.attack_decay & 15, decay);
        rig.knob(LIVE_KNOB_TONE, -3);
        assert_eq!(rig.live.attack_decay & 15, decay - 3);
        assert_eq!(rig.reg(5) & 15, decay - 3, "the register");
        rig.knob(LIVE_KNOB_TONE, 100);
        assert_eq!(rig.live.attack_decay & 15, 15, "clamped");
        rig.knob(LIVE_KNOB_TONE, -100);
        assert_eq!(rig.live.attack_decay & 15, 0);
    }
    // PWM BASS: the pulse width, positions 128 + 64 i (3 % to 50 %), clockwise wider.
    rig.set_preset(PWM_BASS_PRESET);
    assert_eq!(tone(&rig.live), LIVE_TONE_PULSE);
    assert_eq!(rig.live.pulse_width, 1024);
    rig.knob(LIVE_KNOB_TONE, 1);
    assert_eq!(rig.live.pulse_width, 1088);
    rig.knob(LIVE_KNOB_TONE, -3);
    assert_eq!(rig.live.pulse_width, 896);
    rig.knob(LIVE_KNOB_TONE, 100);
    assert_eq!(rig.live.pulse_width, 2048, "50 % is the widest");
    rig.knob(LIVE_KNOB_TONE, -100);
    assert_eq!(rig.live.pulse_width, 128, "3 % the narrowest");
    // 31 positions: 30 detents from end to end, and one side only.
    rig.knob(LIVE_KNOB_TONE, 30);
    assert_eq!(rig.live.pulse_width, 2048);
    // A width off the grid (MIDI) is mirrored above a half and rounded to the nearest position
    // before the step.
    for (set, one_up) in [
        (1100u32, 1152u16),
        (3000, 1152),
        (4000, 192),
        (100, 192),
        (2048, 2048),
    ] {
        unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_PULSE_WIDTH, set) };
        rig.knob(LIVE_KNOB_TONE, 1);
        assert_eq!(rig.live.pulse_width, one_up, "from {set}");
    }
    // A sound the CC switched to the pulse waveform follows: K3 is the pulse width on any preset.
    rig.set_preset(SAW_LEAD);
    assert_eq!(tone(&rig.live), LIVE_TONE_DECAY);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_WAVEFORM, 4) };
    assert_eq!(tone(&rig.live), LIVE_TONE_PULSE);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_WAVEFORM, 2) };
    assert_eq!(tone(&rig.live), LIVE_TONE_DECAY);
    // SYNC LEAD: the ratio, -12..+12 semitones from the preset's 2.75, a semitone a detent.
    rig.set_preset(SYNC_LEAD_PRESET);
    assert_eq!(tone(&rig.live), LIVE_TONE_RATIO);
    assert_eq!((rig.live.sync_semis, rig.live.sync_q8), (0, 704));
    rig.note_on(57);
    let a3 = freq_table()[57];
    assert_eq!(u32::from(rig.voice_freq(0)), (u32::from(a3) * 704) >> 8);
    rig.knob(LIVE_KNOB_TONE, 12);
    assert_eq!(rig.live.sync_semis, 12);
    assert_eq!(rig.live.sync_q8, 1408, "an octave up doubles the ratio");
    rig.tick();
    assert_eq!(
        u32::from(rig.voice_freq(0)),
        (u32::from(a3) * 1408) >> 8,
        "voice 1"
    );
    assert_eq!(rig.voice_freq(2), a3, "voice 3 keeps the pitch");
    rig.knob(LIVE_KNOB_TONE, 100);
    assert_eq!(rig.live.sync_semis, 12);
    rig.knob(LIVE_KNOB_TONE, -100);
    assert_eq!((rig.live.sync_semis, rig.live.sync_q8), (-12, 352));
    rig.knob(LIVE_KNOB_TONE, 12);
    assert_eq!((rig.live.sync_semis, rig.live.sync_q8), (0, 704));
    // A preset load puts the ratio back; no parameter carries it (it has no CC, and is not saved).
    rig.knob(LIVE_KNOB_TONE, 5);
    rig.set_preset(SYNC_LEAD_PRESET);
    assert_eq!((rig.live.sync_semis, rig.live.sync_q8), (0, 704));
    assert_eq!(
        unsafe { panel_knob_param(std::ptr::null(), &*rig.live, 0, PANEL_ENC_K1 + 2) },
        LIVE_PARAM_COUNT
    );
}

#[test]
fn ratio_table_is_exact() {
    let q16 = unsafe { live_ratio_q16 };
    assert_eq!(q16.len(), 25);
    for (k, &q) in q16.iter().enumerate() {
        let exact = 65536.0 * 2f64.powf((k as f64 - 12.0) / 12.0);
        assert!(
            (f64::from(q) - exact).abs() <= 1.0,
            "entry {k}: {q} against {exact}"
        );
        assert!(
            (exact.fract() - 0.5).abs() > 1e-6,
            "entry {k} is far from a rounding tie"
        );
    }
    assert_eq!(q16[12], 65536, "no change is exactly 1");
}

/// Renders A3 for half a second on the real chip, after `turn` detents of K3 (or K4) on `preset`.
fn render_a3(preset: u8, knob: live_knob_id, turn: i32) -> Vec<i16> {
    let mut c = Chip::new(SID_MODEL_6581);
    unsafe { live_set_preset(&mut *c.live, preset) };
    unsafe { live_knob(&mut *c.live, knob, turn) };
    unsafe { live_note_on(&mut *c.live, 57, 127) };
    let mut out = Vec::new();
    for _ in 0..22050 / 441 {
        unsafe { live_tick(&mut *c.live) };
        out.extend(c.render(441));
    }
    out
}

fn rms(v: &[i16]) -> f64 {
    (v.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

#[test]
fn k3_is_audible_on_every_preset() {
    for (preset, detents) in [
        (SAW_LEAD, -8),
        (PWM_BASS_PRESET, 8),
        (SYNC_LEAD_PRESET, 8),
        (NOISE_PERC_PRESET, -8),
    ] {
        let base = render_a3(preset, LIVE_KNOB_TONE, 0);
        let turned = render_a3(preset, LIVE_KNOB_TONE, detents);
        let diff: Vec<i16> = base
            .iter()
            .zip(&turned)
            .map(|(&a, &b)| a.wrapping_sub(b))
            .collect();
        let db = 20.0 * (rms(&diff) / rms(&base)).log10();
        println!("preset {preset}: K3 {detents:+} detents differ by {db:.1} dB of the note");
        assert!(
            db >= -30.0,
            "preset {preset}: K3 {detents:+} is inaudible ({db:.1} dB)"
        );
    }
}

#[test]
fn k4_release_is_audible() {
    // The note's tail after the note-off: the release 6 against 14.
    for preset in [
        SAW_LEAD,
        PWM_BASS_PRESET,
        SYNC_LEAD_PRESET,
        NOISE_PERC_PRESET,
    ] {
        let tail = |turn: i32| {
            let mut c = Chip::new(SID_MODEL_6581);
            unsafe { live_set_preset(&mut *c.live, preset) };
            // a sound with a sustain: the release is what ends it
            unsafe { live_param_set(&mut *c.live, LIVE_PARAM_SUSTAIN, 12) };
            unsafe { live_knob(&mut *c.live, LIVE_KNOB_RELEASE, turn) };
            unsafe { live_note_on(&mut *c.live, 57, 127) };
            c.render(44100 / 4);
            unsafe { live_note_off(&mut *c.live, 57) };
            c.render(44100 / 4)
        };
        let base = tail(0);
        let longer = tail(8);
        let diff: Vec<i16> = base
            .iter()
            .zip(&longer)
            .map(|(&a, &b)| a.wrapping_sub(b))
            .collect();
        let db = 20.0 * (rms(&diff) / rms(&longer).max(1.0)).log10();
        println!("preset {preset}: release +8 differs by {db:.1} dB of the tail");
        assert!(db >= -30.0, "preset {preset}: {db:.1} dB");
    }
}

#[test]
fn k4_steps_the_release_a_detent_at_a_time_and_clamps() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(unsafe { live_param_get(&*rig.live, LIVE_PARAM_RELEASE) }, 6);
    rig.knob(LIVE_KNOB_RELEASE, 1);
    assert_eq!(rig.live.sustain_release & 15, 7);
    assert_eq!(rig.live.sustain_release >> 4, 0xa, "the sustain stays");
    rig.knob(LIVE_KNOB_RELEASE, -1000);
    assert_eq!(rig.live.sustain_release & 15, 0);
    rig.knob(LIVE_KNOB_RELEASE, i32::MAX);
    assert_eq!(rig.live.sustain_release & 15, 15);
    rig.clear();
    rig.knob(LIVE_KNOB_RELEASE, 1);
    assert!(rig.log().is_empty(), "no write past the end");
}

// --- an edit memory per preset ---

fn edited(l: &live) -> bool {
    unsafe { live_preset_edited(l) != 0 }
}

fn preset_name_of(l: &live) -> String {
    let mut buf = [0 as std::ffi::c_char; LIVE_PRESET_NAME_CHARS as usize];
    unsafe { live_preset_name(l, buf.as_mut_ptr()) };
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn a_preset_remembers_its_edits() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert!(!edited(&rig.live));
    assert_eq!(preset_name_of(&rig.live), "SAW LEAD");
    rig.knob(LIVE_KNOB_CUTOFF, -10);
    rig.knob(LIVE_KNOB_RESONANCE, 3);
    rig.knob(LIVE_KNOB_TONE, 2);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_WAVEFORM, 8) };
    let mine = (
        rig.live.cutoff,
        rig.live.resonance,
        rig.live.attack_decay,
        rig.live.control,
    );
    assert!(edited(&rig.live));
    assert_eq!(preset_name_of(&rig.live), "SAW LEAD*");
    // On to PWM BASS: its factory settings, no star; back: the edits and the star.
    rig.preset_step(1);
    let p = preset(PULSE_BASS);
    assert_eq!(
        (rig.live.cutoff, rig.live.resonance, rig.live.pulse_width),
        (p.cutoff, p.resonance, p.pulse_width)
    );
    assert!(!edited(&rig.live));
    assert_eq!(preset_name_of(&rig.live), "PWM BASS");
    rig.preset_step(-1);
    assert_eq!(
        (
            rig.live.cutoff,
            rig.live.resonance,
            rig.live.attack_decay,
            rig.live.control
        ),
        mine
    );
    assert!(edited(&rig.live));
    // Each preset keeps its own: edit PWM BASS, go through the others and back.
    rig.preset_step(1);
    rig.knob(LIVE_KNOB_TONE, 4);
    let pw = rig.live.pulse_width;
    assert_ne!(u32::from(pw), p.pulse_width as u32);
    rig.preset_step(1);
    assert!(!edited(&rig.live), "SYNC LEAD is not edited");
    rig.preset_step(-1);
    assert_eq!(rig.live.pulse_width, pw);
    rig.preset_step(-1);
    assert_eq!(
        rig.live.cutoff, mine.0,
        "SAW LEAD's edit survived the round"
    );
    // The sync ratio is part of the memory and of the star.
    rig.preset_step(2);
    rig.knob(LIVE_KNOB_TONE, 5);
    assert!(edited(&rig.live));
    rig.preset_step(1);
    rig.preset_step(-1);
    assert_eq!((rig.live.sync_semis, rig.live.sync_q8), (5, 940));
}

#[test]
fn program_change_loads_the_factory() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.knob(LIVE_KNOB_CUTOFF, -10);
    assert!(edited(&rig.live));
    // A program change (MIDI, the settings) loads the preset's settings, over any edits.
    rig.set_preset(PULSE_BASS);
    rig.set_preset(SAW_LEAD);
    assert_eq!(rig.live.cutoff, 1200);
    assert!(!edited(&rig.live));
    // And a PRESETS round trip does not bring back what the program change put aside.
    rig.preset_step(1);
    rig.preset_step(-1);
    assert_eq!(rig.live.cutoff, 1200);
    rig.knob(LIVE_KNOB_CUTOFF, -10);
    rig.preset_step(1);
    rig.set_preset(SAW_LEAD);
    rig.preset_step(1);
    rig.preset_step(-1);
    assert_eq!(
        rig.live.cutoff, 1200,
        "the kept edit went with the program change"
    );
}

#[test]
fn edit_presets_reverts() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.knob(LIVE_KNOB_CUTOFF, -10);
    rig.knob(LIVE_KNOB_RESONANCE, 4);
    assert!(edited(&rig.live));
    unsafe { live_preset_step(&mut *rig.live, 1, 1) };
    assert_eq!(rig.live.preset, 0, "the same preset");
    assert_eq!((rig.live.cutoff, rig.live.resonance), (1200, 6));
    assert!(!edited(&rig.live));
    // The memory of that preset went too.
    rig.preset_step(1);
    rig.preset_step(-1);
    assert_eq!(rig.live.cutoff, 1200);
}
