// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! DICE (`app/live_dice.c`): the preset after NOISE PERC, a new sound at every turn.

use sidou_sys::*;

use crate::{preset, Rig};

const DICE: u8 = LIVE_PRESET_COUNT as u8;

fn get(rig: &Rig, p: u32) -> u32 {
    unsafe { live_param_get(&*rig.live, p) }
}

fn roll(rig: &mut Rig, seed: u32) {
    unsafe { live_dice_roll(&mut *rig.live, seed) };
}

fn name(rig: &Rig) -> String {
    let mut buf = [0 as std::ffi::c_char; LIVE_PRESET_NAME_CHARS as usize];
    unsafe { live_preset_name(&*rig.live, buf.as_mut_ptr()) };
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// What the roll sets and the effect slots, to compare.
fn sound(rig: &Rig) -> Vec<u32> {
    (0..LIVE_PARAM_VOLUME)
        .chain(LIVE_PARAM_WAVEFORM..LIVE_PARAM_MODEL)
        .chain(LIVE_PARAM_BEND_RANGE..=LIVE_PARAM_VIBRATO_DELAY)
        .map(|p| get(rig, p))
        .collect()
}

#[test]
fn every_roll_stays_in_its_ranges() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut rig = Rig::new(model);
        let steps = unsafe { live_cutoff_steps }[model as usize];
        for seed in 0..10_000u32 {
            let seed = seed.wrapping_mul(2_654_435_761).wrapping_add(seed);
            roll(&mut rig, seed);
            let wave = get(&rig, LIVE_PARAM_WAVEFORM);
            assert!(matches!(wave, 1 | 2 | 4 | 8), "one waveform: {wave}");
            let (ring, sync) = (get(&rig, LIVE_PARAM_RING), get(&rig, LIVE_PARAM_SYNC));
            assert!(ring == 0 || wave == 1, "ring only on triangle");
            assert!(
                sync == 0 || wave == 2 || wave == 4,
                "sync only on saw or pulse"
            );
            assert!(get(&rig, LIVE_PARAM_ATTACK) < 7);
            assert!((4..12).contains(&get(&rig, LIVE_PARAM_DECAY)));
            let s = get(&rig, LIVE_PARAM_SUSTAIN);
            assert!(if wave == 8 {
                s == 0
            } else {
                (4..16).contains(&s)
            });
            assert!((3..11).contains(&get(&rig, LIVE_PARAM_RELEASE)));
            let c = get(&rig, LIVE_PARAM_CUTOFF);
            assert!(
                steps[16..61].contains(&(c as u16)),
                "cutoff {c} is a table entry"
            );
            assert!(get(&rig, LIVE_PARAM_RESONANCE) < 13);
            assert!(matches!(get(&rig, LIVE_PARAM_FILTER_MODE), 1 | 2 | 4 | 5));
            assert_eq!(get(&rig, LIVE_PARAM_FILTER_VOICES), 7);
            if wave == 4 {
                let pw = get(&rig, LIVE_PARAM_PULSE_WIDTH);
                assert!(
                    (384..=2048).contains(&pw) && (pw - 128).is_multiple_of(64),
                    "pw {pw}"
                );
            }
            let v = get(&rig, LIVE_PARAM_VIBRATO_DEPTH);
            assert!(v == 0 || (8..=40).contains(&v));
            for slot in 0..LIVE_SLOTS {
                assert_eq!(
                    get(
                        &rig,
                        LIVE_PARAM_SLOT + slot * LIVE_SLOT_FIELDS + LIVE_SLOT_ALGORITHM
                    ),
                    0,
                    "a roll never touches an effect slot"
                );
            }
        }
    }
}

#[test]
fn a_roll_leaves_the_effect_slots_alone() {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe { live_slot_pick(&mut *rig.live, 0, 2) };
    let slots: Vec<u32> = (LIVE_PARAM_SLOT..LIVE_PARAM_BEND_RANGE)
        .map(|p| get(&rig, p))
        .collect();
    for seed in [1, 2, 3, 99, 12345] {
        roll(&mut rig, seed);
    }
    let after: Vec<u32> = (LIVE_PARAM_SLOT..LIVE_PARAM_BEND_RANGE)
        .map(|p| get(&rig, p))
        .collect();
    assert_eq!(slots, after);
}

#[test]
fn the_rolls_differ_with_the_seed() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..64u32 {
        roll(&mut rig, seed * 7919);
        seen.insert(sound(&rig));
    }
    assert!(
        seen.len() > 40,
        "{} different sounds in 64 rolls",
        seen.len()
    );
}

#[test]
fn dice_follows_noise_perc_and_rolls_at_every_detent() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert!(!unsafe { live_preset_get(DICE) }.is_null());
    assert!(unsafe { live_preset_get(DICE + 1) }.is_null());
    rig.preset_step(3);
    assert_eq!(rig.live.preset, 3);
    rig.preset_step(1);
    assert_eq!(rig.live.preset, DICE, "NOISE PERC clockwise is DICE");
    assert_eq!(rig.live.dice_rolls, 1);
    assert_eq!(name(&rig), "DICE 01");
    let first = sound(&rig);
    rig.preset_step(1);
    assert_eq!(rig.live.preset, DICE, "clamped: DICE is the last");
    assert_eq!(rig.live.dice_rolls, 2);
    assert_eq!(name(&rig), "DICE 02");
    assert_ne!(sound(&rig), first, "a new roll");
    rig.preset_step(100);
    assert_eq!(rig.live.preset, DICE);
}

#[test]
fn a_roll_is_kept_and_recalled() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.preset_step(4);
    let rolled = sound(&rig);
    let n = rig.live.dice_rolls;
    // counter-clockwise: NOISE PERC as it is; clockwise again: the roll, not a new one
    rig.preset_step(-1);
    assert_eq!(rig.live.preset, 3);
    assert_eq!(rig.live.control, preset(3).control);
    rig.preset_step(1);
    assert_eq!(rig.live.preset, DICE);
    assert_eq!(rig.live.dice_rolls, n, "recalled, not rolled");
    assert_eq!(sound(&rig), rolled);
    assert_eq!(name(&rig), format!("DICE {n:02}"));
}

#[test]
fn dice_has_no_star_and_the_hundredth_roll_wraps() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.preset_step(4);
    unsafe { live_param_set(&mut *rig.live, LIVE_PARAM_CUTOFF, 123) };
    assert_eq!(unsafe { live_preset_edited(&*rig.live) }, 0);
    assert!(!name(&rig).ends_with('*'));
    for _ in 0..99 {
        rig.preset_step(1);
    }
    assert_eq!(rig.live.dice_rolls, 100);
    assert_eq!(name(&rig), "DICE 00");
}

#[test]
fn program_changes_never_reach_dice() {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.set_preset(DICE);
    assert_eq!(rig.live.preset, DICE, "the settings load DICE back");
    rig.set_preset(DICE + 3);
    assert_eq!(rig.live.preset, DICE, "clamped to the last slot");
    assert_eq!(LIVE_PRESET_COUNT, 4, "what a program change selects");
}
