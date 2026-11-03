// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_sound::{valid, Sound};
use crate::Rig;

const NOTE: u8 = 60;
const TICKS_A_SECOND: u32 = 100;
const LOW_CUTOFF: u32 = 400;

fn rig(depth: i32, ad: u8, sr: u8) -> Rig {
    let mut rig = Rig::new(SID_MODEL_6581);
    rig.set_param(LIVE_PARAM_CUTOFF, LOW_CUTOFF);
    unsafe {
        let l = &mut *rig.live;
        live_filter_env_set(
            l,
            LIVE_FENV_DEPTH,
            (depth + LIVE_FENV_DEPTH_MAX as i32) as u32,
        );
        live_filter_env_set(l, LIVE_FENV_ATTACK, u32::from(ad >> 4));
        live_filter_env_set(l, LIVE_FENV_DECAY, u32::from(ad & 15));
        live_filter_env_set(l, LIVE_FENV_SUSTAIN, u32::from(sr >> 4));
        live_filter_env_set(l, LIVE_FENV_RELEASE, u32::from(sr & 15));
    }
    rig
}

fn cutoff(rig: &Rig) -> u32 {
    u32::from(rig.reg(0x15) & 7) | u32::from(rig.reg(0x16)) << 3
}

fn ticks(rig: &mut Rig, n: u32) {
    for _ in 0..n {
        rig.tick();
    }
}

#[test]
fn a_note_sweeps_the_cutoff_up_to_its_peak_and_down_to_the_sustain() {
    let mut rig = rig(48, 0x0c, 0x80);
    ticks(&mut rig, 2);
    assert_eq!(cutoff(&rig), LOW_CUTOFF, "no note, no sweep");
    rig.note_on(NOTE);
    let peak = LOW_CUTOFF + 48 * LIVE_FENV_CUTOFF_UNIT;
    let mut top = 0;
    for _ in 0..TICKS_A_SECOND {
        rig.tick();
        top = top.max(cutoff(&rig));
    }
    assert!(
        top + 40 >= peak,
        "the attack reaches near the peak: {top} of {peak}"
    );
    ticks(&mut rig, 3 * TICKS_A_SECOND);
    let sustain = LOW_CUTOFF + 48 * LIVE_FENV_CUTOFF_UNIT * 8 * 17 / 255;
    assert!(
        cutoff(&rig).abs_diff(sustain) < 40,
        "{} settles at {sustain}",
        cutoff(&rig)
    );
    rig.note_off(NOTE);
    ticks(&mut rig, 3 * TICKS_A_SECOND);
    assert!(
        cutoff(&rig) < LOW_CUTOFF + 40,
        "the release brings it back down"
    );
}

#[test]
fn a_negative_depth_sweeps_it_down_and_zero_is_off() {
    let mut rig = rig(-10, 0x00, 0xf0);
    rig.note_on(NOTE);
    ticks(&mut rig, 20);
    assert!(
        cutoff(&rig) < LOW_CUTOFF - 10 * LIVE_FENV_CUTOFF_UNIT + 40,
        "{}",
        cutoff(&rig)
    );

    let mut rig = rig_off();
    rig.note_on(NOTE);
    ticks(&mut rig, 20);
    assert_eq!(cutoff(&rig), LOW_CUTOFF);
}

fn rig_off() -> Rig {
    rig(0, 0x00, 0xf0)
}

#[test]
fn the_filter_envelope_round_trips_and_its_depth_is_bounded() {
    let r = rig(-33, 0x5a, 0x6c);
    let s = r.sound();
    assert_eq!(s[LAB_SOUND_FILTER_ENV as usize] as i8, -33);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_AD as usize], 0x5a);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_SR as usize], 0x6c);
    let mut back = Rig::new(SID_MODEL_6581);
    unsafe { live_sound_apply(&mut *back.live, s.as_ptr()) };
    assert_eq!(back.sound(), s);
    let mut bad: Sound = s;
    bad[LAB_SOUND_FILTER_ENV as usize] = (LIVE_FENV_DEPTH_MAX as i8 + 1) as u8;
    assert!(!valid(&bad));
}
