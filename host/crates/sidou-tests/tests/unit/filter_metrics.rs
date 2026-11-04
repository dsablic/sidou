// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;

use sidou_tests::alignment::{align, Alignment};
use sidou_tests::band_energy::{
    band_centre_hz, band_frames, distance, fitted_output_stage, Distance, HOP,
};
use sidou_tests::harmonics::harmonics_db;

use crate::{assert_near, sine, xorshift_noise, FS, FSF};

fn floats(x: &[i16]) -> Vec<f32> {
    x.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

fn noise(seconds: f64, seed: u32) -> Vec<f32> {
    floats(&xorshift_noise((seconds * FSF) as usize, seed))
}

fn mixed(a: &[f32], b: &[f32], gain: f32) -> Vec<f32> {
    a.iter().zip(b).map(|(&x, &y)| x + gain * y).collect()
}

fn compared(reference: &[f32], test: &[f32]) -> Distance {
    distance(&band_frames(reference, FSF), &band_frames(test, FSF))
}

fn tone(hz: f64, seconds: f64) -> Vec<f32> {
    floats(&sine(hz, 0.5, (seconds * FSF) as usize))
}

#[test]
fn band_energy_ignores_a_change_of_level() {
    let x = noise(2.0, 1);
    let quieter: Vec<f32> = x.iter().map(|&v| v * 0.25).collect();
    let d = compared(&x, &quieter);
    assert!(d.shape_db < 1e-6 && d.level_db < 1e-6, "{d:?}");
}

#[test]
fn band_energy_places_an_added_tone_in_its_band() {
    let hz = 3000.0;
    let x: Vec<f32> = noise(2.0, 2).iter().map(|&v| v * 0.05).collect();
    let d = compared(&x, &mixed(&x, &tone(hz, 2.0), 0.6));
    let loudest = (0..d.per_band_db.len())
        .max_by(|&a, &b| d.per_band_db[a].total_cmp(&d.per_band_db[b]))
        .unwrap();
    let nearest = (0..d.per_band_db.len())
        .min_by(|&a, &b| {
            let off = |k: usize| (band_centre_hz(k) / hz).log2().abs();
            off(a).total_cmp(&off(b))
        })
        .unwrap();
    assert_eq!(loudest, nearest, "{:?}", d.per_band_db);
}

#[test]
fn band_energy_floor_hides_what_lies_far_below_the_signal() {
    let x = tone(1000.0, 2.0);
    let n = noise(2.0, 3);
    let faint = compared(&x, &mixed(&x, &n, 1e-4));
    let audible = compared(&x, &mixed(&x, &n, 1e-2));
    assert!(faint.shape_db < 0.05, "{faint:?}");
    assert!(audible.shape_db > 1.0, "{audible:?}");
}

#[test]
fn band_energy_skips_windows_where_the_reference_is_silent() {
    let windows = 10;
    let sounding = windows * HOP;
    let mut reference = noise(2.0, 4);
    reference[sounding..].fill(0.0);
    let mut test = reference.clone();
    let other = tone(500.0, 2.0);
    test[sounding + HOP..].copy_from_slice(&other[sounding + HOP..]);
    let d = compared(&reference, &test);
    assert_eq!(d.windows, windows);
    assert!(d.shape_db < 1e-9, "{d:?}");
}

fn one_pole_low_pass(x: &[f32], hz: f64) -> Vec<f32> {
    let a = 1.0 - (-2.0 * PI * hz / FSF).exp();
    let mut y = 0.0;
    x.iter()
        .map(|&v| {
            y += a * (f64::from(v) - y);
            y as f32
        })
        .collect()
}

#[test]
fn output_stage_fit_finds_a_low_pass_on_the_reference() {
    let x = noise(3.0, 5);
    let recording = one_pole_low_pass(&x, 9000.0);
    let (r, t) = (band_frames(&recording, FSF), band_frames(&x, FSF));
    let flat = distance(&r, &t);
    let (stage, fitted) = fitted_output_stage(&r, &t);
    assert!(
        (7500.0..=11_000.0).contains(&stage.low_pass_hz),
        "{stage:?}"
    );
    assert!(fitted.shape_db < flat.shape_db / 3.0, "{fitted:?} {flat:?}");
}

#[test]
fn harmonics_are_read_relative_to_the_fundamental() {
    let f0 = 437.3;
    let parts = [(1.0, 1.0), (2.0, 0.1), (3.0, 0.01), (5.0, 0.03)];
    let x: Vec<f32> = (0..FS as usize / 2)
        .map(|i| {
            let t = i as f64 / FSF;
            parts
                .iter()
                .map(|(k, a)| 0.5 * a * (2.0 * PI * k * f0 * t).sin())
                .sum::<f64>() as f32
        })
        .collect();
    let h = harmonics_db(&x, FSF, f0 * 1.01);
    assert_near!(h[0], -20.0, 0.05);
    assert_near!(h[1], -40.0, 0.05);
    assert!(h[2] < -80.0, "{h:?}");
    assert_near!(h[3], 20.0 * 0.03f64.log10(), 0.05);
}

fn bursts(seconds: f64) -> Vec<f32> {
    let n = noise(seconds, 6);
    let mut gate = crate::Lcg(7);
    n.chunks(2000)
        .flat_map(|c| {
            let on = !gate.next_u32().is_multiple_of(3);
            c.iter().map(move |&v| if on { v } else { 0.0 })
        })
        .collect()
}

#[test]
fn alignment_finds_the_delay_and_polarity_of_a_copy() {
    let x = bursts(2.0);
    for (delay, inverted) in [(12_345isize, true), (-777, false)] {
        let sign = if inverted { -0.5 } else { 0.5 };
        let copy: Vec<f32> = x.iter().map(|&v| sign * v).collect();
        let (reference, test) = if delay >= 0 {
            let mut r = vec![0.0; delay as usize];
            r.extend(&x);
            (r, copy)
        } else {
            let mut t = vec![0.0; (-delay) as usize];
            t.extend(&copy);
            (x.clone(), t)
        };
        assert_eq!(
            align(&reference, &test, 20_000, FSF),
            Alignment {
                lag: delay,
                inverted
            }
        );
    }
}
