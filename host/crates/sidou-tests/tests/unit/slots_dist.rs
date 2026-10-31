// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::slots::*;
use crate::{ac_rms, db, sine, thd, tone_amp, FS, FSF, RESIDUAL_MAX};

fn a_without_character(kind: u32) -> f32 {
    if kind == SID_DIST_OVERDRIVE {
        1.0
    } else {
        0.0
    }
}

fn drive_from_rest(kind: u32, input: &[i16], amount: f32, a: f32, b: f32) -> Vec<i16> {
    let mut od = sid_fx_overdrive::default();
    let mut fz = sid_fx_fuzz::default();
    unsafe {
        sid_fx_overdrive_init(&mut od, FS);
        sid_fx_fuzz_init(&mut fz, FS);
    }
    let out: Vec<i16> = input
        .iter()
        .map(|&v| {
            let x = f32::from(v) / 32768.0;
            let y = unsafe {
                if kind == SID_DIST_OVERDRIVE {
                    sid_fx_overdrive_tick(&mut od, x, amount, a, b)
                } else {
                    sid_fx_fuzz_tick(&mut fz, x, amount, a, b)
                }
            };
            (y * 32768.0).round().clamp(-32768.0, 32767.0) as i16
        })
        .collect();
    out[out.len() / 2..].to_vec()
}

fn harmonic(x: &[i16], hz: f64, k: u32) -> f64 {
    tone_amp(x, hz * f64::from(k))
}

#[test]
fn drives_distort_more_with_amount() {
    let input = sine(441.0, 0.25, 2 * FS as usize);
    for kind in [SID_DIST_OVERDRIVE, SID_DIST_FUZZ] {
        let mut last = 0.0;
        for amount in [0.25f32, 0.5, 0.75, 1.0] {
            let t = thd(
                &drive_from_rest(kind, &input, amount, a_without_character(kind), 0.0),
                441.0,
            );
            println!("drive {kind} amount {amount}: THD {:.1} %", 100.0 * t);
            assert!(
                t > last || t > 0.3,
                "drive {kind}: THD {t} at {amount} is not above {last}"
            );
            last = t;
        }
        assert!(last > 0.3, "drive {kind}: full THD {last}");
    }
}

#[test]
fn drives_hold_the_level_of_a_sid_voice() {
    let input = sine(441.0, 0.25, 2 * FS as usize);
    for kind in [SID_DIST_OVERDRIVE, SID_DIST_FUZZ] {
        for amount in [0.0f32, 0.5, 1.0] {
            let out = drive_from_rest(kind, &input, amount, a_without_character(kind), 0.0);
            let level = db(ac_rms(&out) / ac_rms(&input[FS as usize..]));
            println!("drive {kind} amount {amount}: level {level:.2} dB");
            assert!(
                level.abs() < 4.0,
                "drive {kind} amount {amount}: {level} dB"
            );
        }
    }
}

#[test]
fn overdrive_bias_adds_even_harmonics_and_tone_darkens() {
    let input = sine(441.0, 0.25, 2 * FS as usize);
    let h2 = |b: f32| {
        let out = drive_from_rest(SID_DIST_OVERDRIVE, &input, 0.7, 1.0, b);
        harmonic(&out, 441.0, 2) / harmonic(&out, 441.0, 1)
    };
    let (none, full) = (h2(0.0), h2(1.0));
    println!(
        "overdrive 2nd harmonic: bias 0 {:.4}, bias 1 {full:.4}",
        none
    );
    assert!(full > 10.0 * none && full > 0.02);
    let highs = |tone: f32| {
        let out = drive_from_rest(SID_DIST_OVERDRIVE, &input, 1.0, tone, 0.0);
        harmonic(&out, 441.0, 13) / harmonic(&out, 441.0, 1)
    };
    let (dark, bright) = (highs(0.0), highs(1.0));
    println!("overdrive 13th harmonic: tone 0 {dark:.4}, tone 1 {bright:.4}");
    assert!(bright > 3.0 * dark);
}

#[test]
fn fuzz_octave_dominates_and_silence_starves_its_bias() {
    let input = sine(441.0, 0.25, 2 * FS as usize);
    let out = drive_from_rest(SID_DIST_FUZZ, &input, 0.6, 1.0, 0.0);
    let (f1, f2) = (harmonic(&out, 441.0, 1), harmonic(&out, 441.0, 2));
    println!("fuzz octave: fundamental {f1:.0}, octave {f2:.0}");
    assert!(f2 > 3.0 * f1, "the octave does not dominate");
    let mut silence = vec![0i16; FS as usize];
    silence.extend(sine(441.0, 0.25, FS as usize / 4));
    silence.extend(vec![0i16; FS as usize]);
    let mut fz = sid_fx_fuzz::default();
    unsafe { sid_fx_fuzz_init(&mut fz, FS) };
    let y: Vec<f32> = silence
        .iter()
        .map(|&v| unsafe { sid_fx_fuzz_tick(&mut fz, f32::from(v) / 32768.0, 1.0, 0.0, 1.0) })
        .collect();
    let lead = y[..FS as usize].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let tail = y[y.len() - FS as usize / 2..]
        .iter()
        .fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        lead == 0.0 && tail < 1e-4,
        "silence: {lead} before, {tail} after"
    );
    let biased = drive_from_rest(SID_DIST_FUZZ, &input, 0.6, 0.0, 1.0);
    let unbiased = drive_from_rest(SID_DIST_FUZZ, &input, 0.6, 0.0, 0.0);
    let even = |x: &[i16]| harmonic(x, 441.0, 2) / harmonic(x, 441.0, 1);
    println!(
        "fuzz 2nd harmonic: bias 0 {:.4}, bias 1 {:.4}",
        even(&unbiased),
        even(&biased)
    );
    assert!(even(&biased) > 10.0 * even(&unbiased));
}

fn worst_aliases_below_15k_and_overall(kind: u32, input: &[i16], f0: f64) -> (f64, f64) {
    let out = drive_from_rest(kind, input, 1.0, a_without_character(kind), 0.0);
    let f1 = harmonic(&out, f0, 1);
    let (mut below, mut all) = (-300.0f64, -300.0f64);
    for k in 2..=60u32 {
        let h = f0 * f64::from(k);
        if h < FSF / 2.0 {
            continue;
        }
        let mut f = h % FSF;
        if f > FSF / 2.0 {
            f = FSF - f;
        }
        if ((f / f0).round() * f0 - f).abs() < 1.0 {
            continue;
        }
        let a = db(tone_amp(&out, f) / f1);
        all = all.max(a);
        if f < 15000.0 {
            below = below.max(a);
        }
    }
    (below, all)
}

#[test]
fn drives_hold_their_aliases_down() {
    let n = 2 * FS as usize;
    let sine8k = sine(8000.0, 0.5, n);
    let saw: Vec<i16> = (0..n)
        .map(|i| {
            let t = i as f64 / FSF;
            let v: f64 = (1..=4)
                .map(|k| {
                    (2.0 * std::f64::consts::PI * 5000.0 * f64::from(k) * t).sin() / f64::from(k)
                })
                .sum();
            (0.5 * 32767.0 * v * 2.0 / std::f64::consts::PI) as i16
        })
        .collect();
    for kind in [SID_DIST_OVERDRIVE, SID_DIST_FUZZ] {
        let (b8, a8) = worst_aliases_below_15k_and_overall(kind, &sine8k, 8000.0);
        let (bs, as_) = worst_aliases_below_15k_and_overall(kind, &saw, 5000.0);
        println!(
            "drive {kind} at full: 8 kHz sine worst_aliases_below_15k_and_overall {b8:.1} dBc below 15 kHz ({a8:.1} overall); \
             5 kHz saw {bs:.1} dBc below 15 kHz ({as_:.1} overall)"
        );
        assert!(
            b8 < -25.0,
            "drive {kind}: 8 kHz alias below 15 kHz at {b8} dBc"
        );
        assert!(
            bs < -25.0,
            "drive {kind}: saw alias below 15 kHz at {bs} dBc"
        );
    }
}

#[test]
fn drive_parameter_jumps_are_click_free() {
    use Control::*;
    let g = SID_SLOT_DIST_GAIN;
    assert_click_free_at(
        0.03 / g,
        DIST,
        params(SID_DIST_OVERDRIVE, 0.5, 0.5, 0.5, 1.0),
        &[
            (Amount, 0.0, 1.0),
            (A, 0.0, 1.0),
            (B, 0.0, 1.0),
            (Mix, 0.0, 1.0),
        ],
    );
    assert_click_free_at(
        0.01 / g,
        DIST,
        params(SID_DIST_FUZZ, 0.3, 0.5, 0.5, 1.0),
        &[(A, 0.0, 1.0), (B, 0.0, 1.0), (Mix, 0.0, 1.0)],
    );
    assert_click_free_at(
        0.03 / g,
        DIST,
        params(SID_DIST_FUZZ, 0.0, 0.0, 0.0, 1.0),
        &[(Amount, 0.0, 0.5)],
    );
}

#[test]
fn switching_between_drives_is_click_free() {
    let (sw, steady) = worst_second_diff_at_and_between_switches(
        DIST,
        &[
            SID_DIST_OVERDRIVE,
            SID_DIST_FUZZ,
            SID_DIST_TAPE,
            SID_DIST_OVERDRIVE,
        ],
        0.5,
        0.5,
        0.0,
    );
    println!("drive switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady + RESIDUAL_MAX, "a switch steps by {sw}");
}
