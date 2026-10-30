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

/// The cutoff at K1's walk position `pos`: to the walk's first entry, then K1 up `pos` detents.
fn cutoff_at(rig: &mut Rig, pos: i32) {
    let t = steps(rig.live.model);
    set(rig, LIVE_PARAM_CUTOFF, u32::from(t[0]));
    rig.knob(LIVE_KNOB_CUTOFF, -1); // on the walk: its place 0
    rig.knob(LIVE_KNOB_CUTOFF, pos);
    assert_eq!(rig.live.cutoff, t[pos as usize]);
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
        assert_eq!(
            (lo, hi),
            (t[22], t[38]),
            "8 detents each way, model {model}"
        );
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
        assert!(
            distinct.len() > 17,
            "{} values: more than the walk's 17",
            distinct.len()
        );
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
    assert_eq!(
        rig.live.lfo_phase, phase,
        "no PWM, no filter LFO: the phase holds"
    );
    set(&mut rig, LIVE_PARAM_FILTER_LFO, 4);
    swept(&mut rig, 5);
    assert_ne!(rig.live.lfo_phase, phase, "the filter LFO alone moves it");
}

#[test]
fn the_filter_lfo_is_a_parameter_of_the_sound_with_a_cc() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(
        unsafe { live_param_max(LIVE_PARAM_FILTER_LFO) },
        LIVE_FILTER_LFO_MAX
    );
    set(&mut rig, LIVE_PARAM_FILTER_LFO, 99);
    assert_eq!(
        get(&rig, LIVE_PARAM_FILTER_LFO),
        LIVE_FILTER_LFO_MAX,
        "clamped"
    );
    rig.cc(LIVE_CC_FILTER_LFO, 64);
    assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), 16, "CC 31 over 0..32");
    let mut s = [0u8; LAB_SOUND_BYTES as usize];
    unsafe { live_sound_get(&*rig.live, s.as_mut_ptr()) };
    assert_eq!(s[LAB_SOUND_FILTER_LFO as usize], 16);
    assert!(
        unsafe { live_preset_edited(&*rig.live) } != 0,
        "an edit of the sound"
    );
    rig.set_preset(1);
    assert_eq!(get(&rig, LIVE_PARAM_FILTER_LFO), 0, "a preset loads it off");
    let mut out = [0u16; LIVE_PARAM_COUNT as usize];
    unsafe { live_preset_params(1, out.as_mut_ptr()) };
    assert_eq!(out[LIVE_PARAM_FILTER_LFO as usize], 0);
}

/// The arpeggio's rate is a parameter (LAB's MOD K4): CC 28 through its map row (the same rates
/// as before), an NRPN, and MIDI learn can rebind it; a preset loads its default.
#[test]
fn the_arp_rate_is_a_parameter_on_cc_28s_row() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(
        unsafe { live_param_max(LIVE_PARAM_ARP_RATE) },
        LIVE_ARP_RATES - 1
    );
    assert_eq!(get(&rig, LIVE_PARAM_ARP_RATE), LIVE_ARP_RATE_DEFAULT);
    set(&mut rig, LIVE_PARAM_ARP_RATE, 7);
    assert_eq!(rig.live.arp_rate, 7);
    set(&mut rig, LIVE_PARAM_ARP_RATE, 99);
    assert_eq!(u32::from(rig.live.arp_rate), LIVE_ARP_RATES - 1, "clamped");
    let e = unsafe { midi_map_for_cc(LIVE_CC_ARP_RATE as u8) };
    assert!(!e.is_null(), "CC 28 is a row's");
    assert_eq!(u32::from(unsafe { (*e).param }), LIVE_PARAM_ARP_RATE);
    for (value, rate) in [(0u8, 0u8), (14, 0), (15, 1), (100, 7), (127, 8)] {
        rig.cc(LIVE_CC_ARP_RATE, value);
        assert_eq!(rig.live.arp_rate, rate, "CC {value}");
    }
    rig.set_preset(1);
    assert_eq!(get(&rig, LIVE_PARAM_ARP_RATE), LIVE_ARP_RATE_DEFAULT);
    let mut out = [0u16; LIVE_PARAM_COUNT as usize];
    unsafe { live_preset_params(1, out.as_mut_ptr()) };
    assert_eq!(
        u32::from(out[LIVE_PARAM_ARP_RATE as usize]),
        LIVE_ARP_RATE_DEFAULT
    );
}

/// The resonance the chip has (register $d417's high nibble).
fn res_on_chip(rig: &Rig) -> u32 {
    u32::from(rig.reg(0x17) >> 4)
}

/// RESO LINK: the resonance follows the cutoff's place on K1's walk, link x (32 - place) / 8 on
/// top of the parameter, clamped; positive opens it as the cutoff closes, negative (squelch) as it
/// opens. A term: the parameter stays, and the link at 0 gives it back.
#[test]
fn reso_link_makes_the_resonance_follow_the_cutoff() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(get(&rig, LIVE_PARAM_RESO_LINK), LIVE_LINK_MAX, "8: no link");
    set(&mut rig, LIVE_PARAM_RESONANCE, 6);
    for (link, pos, want) in [
        (8i32, 32, 6u32),
        (8, 24, 14),
        (8, 16, 15),
        (8, 40, 0),
        (4, 24, 10),
        (-8, 40, 14),
        (-8, 24, 0),
        (-2, 64, 14),
    ] {
        set(
            &mut rig,
            LIVE_PARAM_RESO_LINK,
            (link + LIVE_LINK_MAX as i32) as u32,
        );
        cutoff_at(&mut rig, pos);
        rig.tick(); // the resonance follows at the tick
        assert_eq!(res_on_chip(&rig), want, "link {link} at {pos}");
        assert_eq!(unsafe { live_resonance_effective(&*rig.live) }, want);
        assert_eq!(rig.live.resonance, 6, "the parameter stays");
    }
    set(&mut rig, LIVE_PARAM_RESO_LINK, LIVE_LINK_MAX);
    assert_eq!(res_on_chip(&rig), 6, "no link: the parameter");
    // CC 74 moves the cutoff, and the resonance with it
    set(&mut rig, LIVE_PARAM_RESO_LINK, LIVE_LINK_MAX + 8);
    rig.cc(LIVE_CC_CUTOFF, 0);
    rig.tick();
    assert_eq!(res_on_chip(&rig), 15, "CC 74 closed: up");
    rig.cc(LIVE_CC_CUTOFF, 127);
    rig.tick();
    assert_eq!(res_on_chip(&rig), 0, "CC 74 open: down");
    // the mod wheel on the cutoff, applied at the tick
    rig.cc(LIVE_CC_CUTOFF, 0);
    set(&mut rig, LIVE_PARAM_MOD_TARGET, LIVE_MOD_CUTOFF);
    rig.cc(1, 127);
    rig.tick();
    assert!(
        res_on_chip(&rig) < 15,
        "the wheel opened it: {}",
        res_on_chip(&rig)
    );
    // and the filter LFO's sweep
    rig.cc(1, 0);
    rig.tick();
    cutoff_at(&mut rig, 32);
    set(&mut rig, LIVE_PARAM_PWM_RATE, 125);
    set(&mut rig, LIVE_PARAM_FILTER_LFO, 16);
    let res: Vec<u32> = (0..16)
        .map(|_| {
            rig.tick();
            res_on_chip(&rig)
        })
        .collect();
    assert_eq!(*res.iter().max().unwrap(), 15, "{res:?}");
    assert_eq!(*res.iter().min().unwrap(), 0, "{res:?}");
}

/// PWM LINK: the PWM's depth follows the width's distance from 50 %, link x distance / 8 on top
/// of the parameter, clamped; the LFO runs while the depth it gives is not 0.
#[test]
fn pwm_link_makes_the_pwm_depth_follow_the_width() {
    let mut rig = Rig::new(SID_MODEL_6581);
    let eff = |rig: &Rig| unsafe { live_pwm_depth_effective(&*rig.live) };
    set(&mut rig, LIVE_PARAM_PWM_DEPTH, 100);
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 1024);
    assert_eq!(eff(&rig), 100, "no link");
    set(&mut rig, LIVE_PARAM_PWM_LINK, LIVE_LINK_MAX + 4);
    assert_eq!(eff(&rig), 100 + 4 * 1024 / 8);
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 3072);
    assert_eq!(
        eff(&rig),
        100 + 4 * 1024 / 8,
        "the other side, the same distance"
    );
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 2048);
    assert_eq!(eff(&rig), 100, "a square: the parameter");
    set(&mut rig, LIVE_PARAM_PWM_LINK, 0); // -8
    set(&mut rig, LIVE_PARAM_PULSE_WIDTH, 1024);
    assert_eq!(eff(&rig), 0, "clamped at 0");
    assert_eq!(rig.live.pwm_depth, 100, "the parameter stays");
    // the LFO runs for the linked depth alone
    set(&mut rig, LIVE_PARAM_PWM_DEPTH, 0);
    set(&mut rig, LIVE_PARAM_PWM_RATE, 20);
    set(&mut rig, LIVE_PARAM_PWM_LINK, LIVE_LINK_MAX + 8);
    let phase = rig.live.lfo_phase;
    rig.tick();
    assert_ne!(rig.live.lfo_phase, phase, "the linked depth moves the LFO");
    rig.note_on(60);
    let pws: Vec<u16> = (0..100)
        .map(|_| {
            rig.tick();
            rig.voice_pw(0)
        })
        .collect();
    assert!(
        pws.iter().max().unwrap() - pws.iter().min().unwrap() > 500,
        "the width swings: {:?}",
        (pws.iter().min(), pws.iter().max())
    );
}

/// The links are parameters of the sound, NRPN only (MIDI learn reaches them), 0..16 for
/// -8..+8; a preset loads them at none.
#[test]
fn the_links_are_parameters_of_the_sound() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for p in [LIVE_PARAM_RESO_LINK, LIVE_PARAM_PWM_LINK] {
        assert_eq!(unsafe { live_param_max(p) }, 2 * LIVE_LINK_MAX);
        let e = unsafe { midi_map_for_param(p) };
        assert!(!e.is_null());
        assert_eq!(u32::from(unsafe { (*e).cc }), MIDI_MAP_NO_CC);
        set(&mut rig, p, 3);
    }
    let mut s = [0u8; LAB_SOUND_BYTES as usize];
    unsafe { live_sound_get(&*rig.live, s.as_mut_ptr()) };
    assert_eq!(s[LAB_SOUND_RESO_LINK as usize] as i8, -5);
    assert_eq!(s[LAB_SOUND_PWM_LINK as usize] as i8, -5);
    rig.set_preset(1);
    assert_eq!(get(&rig, LIVE_PARAM_RESO_LINK), LIVE_LINK_MAX);
    let mut out = [0u16; LIVE_PARAM_COUNT as usize];
    unsafe { live_preset_params(1, out.as_mut_ptr()) };
    assert_eq!(out[LIVE_PARAM_PWM_LINK as usize], LIVE_LINK_MAX as u16);
}
