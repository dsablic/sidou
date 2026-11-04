// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_sound::valid;
use crate::Rig;

const NOTE: u8 = 60;
const FASTEST: u8 = LAB_LFO_RATES as u8 - 1;
const A_SECOND: usize = 100;

fn lfo(shape: u32, dest: u32, rate: u8, depth: i8) -> [u8; 3] {
    [
        (shape | dest << LAB_LFO_DEST_SHIFT) as u8,
        rate,
        depth as u8,
    ]
}

fn rig_with(lfos: &[[u8; 3]]) -> Rig {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut s = rig.sound();
    s[LAB_SOUND_CONTROL as usize] = 0x40;
    for (n, f) in lfos.iter().enumerate() {
        let at = (LAB_SOUND_LFO + n as u32 * LAB_SOUND_LFO_BYTES) as usize;
        s[at..at + 3].copy_from_slice(f);
    }
    assert!(valid(&s));
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
    rig.note_on(NOTE);
    rig
}

fn sampled(rig: &mut Rig, ticks: usize, read: impl Fn(&Rig) -> u32) -> Vec<u32> {
    (0..ticks)
        .map(|_| {
            rig.tick();
            read(rig)
        })
        .collect()
}

fn span(v: &[u32]) -> (u32, u32) {
    (*v.iter().min().unwrap(), *v.iter().max().unwrap())
}

fn cutoff(rig: &Rig) -> u32 {
    u32::from(rig.reg(0x15) & 7) | u32::from(rig.reg(0x16)) << 3
}

#[test]
fn an_lfo_off_or_at_no_depth_moves_nothing() {
    for f in [[0u8; 3], lfo(LAB_LFO_TRIANGLE, LAB_LFO_PITCH, FASTEST, 0)] {
        let mut rig = rig_with(&[f]);
        let freqs = sampled(&mut rig, A_SECOND, |r| u32::from(r.voice_freq(0)));
        assert_eq!(span(&freqs).0, span(&freqs).1);
    }
}

#[test]
fn a_full_pitch_lfo_swings_an_octave_each_way() {
    let mut rig = rig_with(&[lfo(LAB_LFO_TRIANGLE, LAB_LFO_PITCH, FASTEST, 63)]);
    let base = f64::from(unsafe { live_freq_at(NOTE, 0) });
    let (lo, hi) = span(&sampled(&mut rig, A_SECOND, |r| u32::from(r.voice_freq(0))));
    assert!(
        (f64::from(hi) / base - 2.0).abs() < 0.08,
        "{hi} over {base}"
    );
    assert!(
        (base / f64::from(lo) - 2.0).abs() < 0.08,
        "{lo} under {base}"
    );
}

#[test]
fn the_width_and_the_cutoff_follow_their_lfos() {
    let mut rig = rig_with(&[
        lfo(LAB_LFO_SQUARE, LAB_LFO_PULSE, FASTEST, 32),
        lfo(LAB_LFO_SAW_UP, LAB_LFO_CUTOFF, FASTEST, -63),
    ]);
    let width = u32::from(rig.live.pulse_width);
    let cut = u32::from(rig.live.cutoff);
    let both = sampled(&mut rig, A_SECOND, |r| {
        u32::from(r.voice_pw(0)) << 16 | cutoff(r)
    });
    let widths: Vec<u32> = both.iter().map(|w| w >> 16).collect();
    let cuts: Vec<u32> = both.iter().map(|w| w & 0xffff).collect();
    let mut levels = widths.clone();
    levels.sort_unstable();
    levels.dedup();
    assert_eq!(levels.len(), 2, "a square has two levels: {levels:?}");
    assert!(
        levels[1] - width > 900 && width - levels[0] > 900,
        "{levels:?} around {width}"
    );
    let (lo, hi) = span(&cuts);
    assert!(hi > cut + 700 && lo + 700 < cut, "{lo}..{hi} around {cut}");
}

#[test]
fn a_volume_lfo_is_a_tremolo_down_from_full_either_way() {
    for depth in [63i8, -63] {
        let mut rig = rig_with(&[lfo(LAB_LFO_SQUARE, LAB_LFO_VOLUME, FASTEST, depth)]);
        let vols = sampled(&mut rig, A_SECOND, |r| u32::from(r.reg(0x18) & 15));
        assert_eq!(span(&vols), (0, 15), "depth {depth}");
    }
}

#[test]
fn sample_and_hold_keeps_a_level_for_a_whole_cycle() {
    let slow = 10;
    let mut rig = rig_with(&[lfo(LAB_LFO_SAMPLE_HOLD, LAB_LFO_CUTOFF, slow, 63)]);
    let cuts = sampled(&mut rig, 4 * A_SECOND, cutoff);
    let changes = cuts.windows(2).filter(|w| w[0] != w[1]).count();
    let cycles = 4.0 * 1.0
        / (65536.0 / f64::from(unsafe { live_walk_at(LIVE_WALK_PWM_RATE, slow.into()) }) / 100.0);
    assert!(
        (changes as f64 - cycles).abs() <= 1.5,
        "{changes} changes in about {cycles:.1} cycles"
    );
}

#[test]
fn turning_an_lfo_off_puts_everything_back() {
    let mut rig = rig_with(&[lfo(LAB_LFO_TRIANGLE, LAB_LFO_VOLUME, FASTEST, 63)]);
    for _ in 0..7 {
        rig.tick();
    }
    unsafe { live_lfo_set(&mut *rig.live, 0, [0u8; 3].as_ptr()) };
    rig.tick();
    assert_eq!(rig.reg(0x18) & 15, 15);
}
