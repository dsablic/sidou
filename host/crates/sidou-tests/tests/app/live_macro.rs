// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The sound's modulation since 1.1 (`app/live.h`): the filter LFO (FILTER's K4 in LAB), a
//! triangle over K1's cutoff walk at the PWM's rate.

use sidou_sys::*;

use crate::Rig;

fn steps(model: u32) -> [u16; LIVE_CUTOFF_STEPS as usize] {
    unsafe { live_cutoff_steps[model as usize] }
}

fn set(rig: &mut Rig, p: u32, v: u32) {
    unsafe { live_param_set(&mut *rig.live, p, v) };
}

fn get(rig: &Rig, p: u32) -> u32 {
    unsafe { live_param_get(&*rig.live, p) }
}

/// K1 to walk position `pos` from the bottom.
fn cutoff_at(rig: &mut Rig, pos: i32) {
    rig.knob(LIVE_KNOB_CUTOFF, -200);
    rig.knob(LIVE_KNOB_CUTOFF, pos);
    assert_eq!(rig.live.cutoff, steps(rig.live.model)[pos as usize]);
}

/// The cutoff written each tick for `ticks` ticks.
fn swept(rig: &mut Rig, ticks: usize) -> Vec<u16> {
    (0..ticks)
        .map(|_| {
            rig.tick();
            rig.live.cutoff_out
        })
        .collect()
}

#[test]
fn the_filter_lfo_sweeps_k1s_walk_each_way_at_the_pwm_rate() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut rig = Rig::new(model);
        let t = steps(model);
        cutoff_at(&mut rig, 30);
        assert_eq!(get(&rig, LIVE_PARAM_PWM_DEPTH), 0, "SAW LEAD: no PWM");
        set(&mut rig, LIVE_PARAM_PWM_RATE, 125); // 6.25 Hz: a cycle in 16 ticks exactly
        let still = swept(&mut rig, 10);
        assert!(still.iter().all(|&c| c == t[30]), "off: the cutoff stays");
        set(&mut rig, LIVE_PARAM_FILTER_LFO, 8);
        assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), 8);
        let cycle = swept(&mut rig, 16);
        let (lo, hi) = (*cycle.iter().min().unwrap(), *cycle.iter().max().unwrap());
        assert_eq!((lo, hi), (t[22], t[38]), "8 detents each way, model {model}");
        assert_eq!(cycle[3], t[38], "up first: the top a quarter cycle in");
        assert_eq!(cycle[11], t[22], "the bottom three quarters in");
        assert_eq!(cycle[15], t[30], "a whole cycle: back at the cutoff");
        // smooth at 1 Hz: between the walk's entries, never more than an entry's step a tick
        set(&mut rig, LIVE_PARAM_PWM_RATE, 20);
        let slow = swept(&mut rig, 100);
        for w in slow.windows(2) {
            assert!(w[0].abs_diff(w[1]) <= t[39] - t[38], "{w:?}");
        }
        let mut distinct = slow.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() > 17, "{} values: more than the walk's 17", distinct.len());
        set(&mut rig, LIVE_PARAM_PWM_RATE, 125);
        rig.live.lfo_phase = 16384; // the cycle's start again
        // the base moves under it: K1 a detent up, the sweep with it
        rig.knob(LIVE_KNOB_CUTOFF, 1);
        let cycle = swept(&mut rig, 16);
        let (lo, hi) = (*cycle.iter().min().unwrap(), *cycle.iter().max().unwrap());
        assert_eq!((lo, hi), (t[23], t[39]));
        // clamped at the walk's ends
        set(&mut rig, LIVE_PARAM_FILTER_LFO, 32);
        let cycle = swept(&mut rig, 16);
        assert_eq!(*cycle.iter().max().unwrap(), t[63], "31 + 32");
        assert_eq!(*cycle.iter().min().unwrap(), t[0]);
        // off again: the cutoff back where K1 has it
        set(&mut rig, LIVE_PARAM_FILTER_LFO, 0);
        rig.tick();
        assert_eq!(rig.live.cutoff_out, t[31]);
    }
}

#[test]
fn the_lfo_runs_for_the_filter_alone_and_holds_while_both_are_off() {
    let mut rig = Rig::new(SID_MODEL_6581);
    set(&mut rig, LIVE_PARAM_PWM_RATE, 20);
    let phase = rig.live.lfo_phase;
    swept(&mut rig, 5);
    assert_eq!(rig.live.lfo_phase, phase, "no PWM, no filter LFO: the phase holds");
    set(&mut rig, LIVE_PARAM_FILTER_LFO, 4);
    swept(&mut rig, 5);
    assert_ne!(rig.live.lfo_phase, phase, "the filter LFO alone moves it");
}

#[test]
fn the_filter_lfo_is_a_parameter_of_the_sound_with_a_cc() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(unsafe { live_param_max(LIVE_PARAM_FILTER_LFO) }, LIVE_FILTER_LFO_MAX);
    set(&mut rig, LIVE_PARAM_FILTER_LFO, 99);
    assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), LIVE_FILTER_LFO_MAX, "clamped");
    rig.cc(LIVE_CC_FILTER_LFO, 64);
    assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), 16, "CC 31 over 0..32");
    let mut s = [0u8; LAB_SOUND_BYTES as usize];
    unsafe { live_sound_get(&*rig.live, s.as_mut_ptr()) };
    assert_eq!(s[LAB_SOUND_FILTER_LFO as usize], 16);
    assert!(unsafe { live_preset_edited(&*rig.live) } != 0, "an edit of the sound");
    rig.set_preset(1);
    assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), 0, "a preset loads it off");
    let mut out = [0u16; LIVE_PARAM_COUNT as usize];
    unsafe { live_preset_params(1, out.as_mut_ptr()) };
    assert_eq!(out[LIVE_PARAM_FILTER_LFO as usize], 0);
}
