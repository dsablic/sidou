// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::slots::*;
use crate::{ac_rms, db, sine, tone_amp, xorshift_noise, FS, FSF, RESIDUAL_MAX};

fn rate_and_half_peak_to_peak(trace: &[f32]) -> (f64, f64) {
    let mean = trace.iter().map(|&v| f64::from(v)).sum::<f64>() / trace.len() as f64;
    let ups: Vec<usize> = (1..trace.len())
        .filter(|&i| f64::from(trace[i - 1]) < mean && f64::from(trace[i]) >= mean)
        .collect();
    let hz = (ups.len() - 1) as f64 * FSF / (ups[ups.len() - 1] - ups[0]) as f64;
    let (lo, hi) = trace
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    (hz, f64::from(hi - lo) / 2.0)
}

fn correlation(a: &[i16], b: &[i16]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa * bb).sqrt()
}

fn second_half_through_mod_at_full_mix(
    alg: u32,
    input: &[i16],
    amount: f32,
    a: f32,
    b: f32,
) -> (Vec<i16>, Vec<i16>) {
    let mut s = Slots::new(1);
    s.set(MOD, params(alg, amount, a, b, 1.0));
    let out = s.run(input);
    let half = out.len() / 2;
    (left(&out[half..]), right(&out[half..]))
}

#[test]
fn chorus_lfo_rate_and_depth() {
    let mut c = Box::<sid_fx_chorus>::default();
    unsafe { sid_fx_chorus_init(&mut *c, FS) };
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut d0 = Vec::new();
    let mut d3 = Vec::new();
    for _ in 0..(5 * FS) {
        unsafe { sid_fx_chorus_tick(&mut *c, 0.0, 1.0, 2.0 / 3.0, 0.0, &mut l, &mut r) };
        d0.push(c.delay[0]);
        d3.push(c.delay[3]);
    }
    let (hz, depth) = rate_and_half_peak_to_peak(&d0);
    let want = SID_FX_CHORUS_SLOW_MS * 1e-3 * FSF;
    println!("chorus: {hz:.3} Hz, depth {depth:.1} samples (want {want:.1})");
    assert!((hz / SID_FX_CHORUS_SLOW_HZ - 1.0).abs() < 0.02);
    assert!((depth / want - 1.0).abs() < 0.01);
    let lag = (0..FS as usize)
        .max_by(|&i, &j| d3[i].total_cmp(&d3[j]))
        .unwrap() as f64
        - (0..FS as usize)
            .max_by(|&i, &j| d0[i].total_cmp(&d0[j]))
            .unwrap() as f64;
    let degrees = (lag * hz / FSF * 360.0).rem_euclid(360.0);
    println!("chorus: right tap {degrees:.0} degrees from the left");
    assert!((degrees - 300.0).abs() < 3.0 || (degrees - 60.0).abs() < 3.0);
}

#[test]
fn chorus_is_wide_and_keeps_the_level() {
    let input = xorshift_noise(4 * FS as usize, 11);
    let (l, r) = second_half_through_mod_at_full_mix(SID_MOD_CHORUS, &input, 1.0, 0.7, 1.0);
    let rho = correlation(&l, &r);
    let lvl = db(ac_rms(&l) / ac_rms(&input));
    println!("chorus on noise: L/R correlation {rho:.3}, level {lvl:.2} dB");
    assert!(rho < 0.5, "L and R correlate at {rho}");
    assert!((lvl + 6.5).abs() < 1.0, "level {lvl} dB");
    let tone = sine(220.0, 0.5, 4 * FS as usize);
    let (l, r) = second_half_through_mod_at_full_mix(SID_MOD_CHORUS, &tone, 0.3, 0.7, 0.0);
    let lvl = db(ac_rms(&l) / ac_rms(&tone));
    let beta = 2.0 * std::f64::consts::PI * 220.0 * 0.3 * SID_FX_CHORUS_SLOW_MS * 1e-3;
    let j0: f64 = (0..20)
        .map(|k| {
            let fact: f64 = (1..=k).map(f64::from).product();
            (-1f64).powi(k) * (beta / 2.0).powi(2 * k) / (fact * fact)
        })
        .sum();
    println!(
        "chorus on 220 Hz: level {lvl:.2} dB (J0 predicts {:.2}), correlation {:.3}",
        db(j0),
        correlation(&l, &r)
    );
    assert!((lvl - db(j0)).abs() < 0.5);
}

#[test]
fn flanger_sweeps_its_range_and_stays_stable() {
    let mut f = Box::<sid_fx_flanger>::default();
    unsafe { sid_fx_flanger_init(&mut *f, FS) };
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut dl = Vec::new();
    let mut dr = Vec::new();
    for _ in 0..(4 * FS) {
        unsafe { sid_fx_flanger_tick(&mut *f, 0.0, 1.0, 0.5, 0.5, &mut l, &mut r) };
        dl.push(f.delay_l);
        dr.push(f.delay_r);
    }
    let (hz, half) = rate_and_half_peak_to_peak(&dl);
    let lo = dl.iter().fold(f32::MAX, |m, &v| m.min(v));
    let geometric_middle_hz = SID_FX_FLANGER_MIN_HZ * 10.0;
    println!(
        "flanger: {hz:.3} Hz, {:.2} .. {:.2} ms",
        f64::from(lo) / 44.1,
        (f64::from(lo) + 2.0 * half) / 44.1
    );
    assert!((hz / geometric_middle_hz - 1.0).abs() < 0.03);
    assert!((f64::from(lo) / 44.1 - SID_FX_FLANGER_MIN_MS).abs() < 0.01);
    assert!(((f64::from(lo) + 2.0 * half) / 44.1 - SID_FX_FLANGER_MAX_MS).abs() < 0.05);
    let mid = dl
        .iter()
        .position(|&v| (v - (lo + half as f32)).abs() < 2.0)
        .unwrap();
    let r_off = (dr[mid] - (lo + half as f32)).abs();
    assert!(
        r_off > 0.9 * half as f32,
        "right is {r_off} from the middle"
    );

    for fb in [0.0f32, 1.0] {
        let mut f = Box::<sid_fx_flanger>::default();
        unsafe { sid_fx_flanger_init(&mut *f, FS) };
        let noise = xorshift_noise(5 * FS as usize, 3);
        let mut peak = 0.0f32;
        for &v in &noise {
            unsafe {
                sid_fx_flanger_tick(
                    &mut *f,
                    f32::from(v) / 32768.0,
                    1.0,
                    1.0,
                    fb,
                    &mut l,
                    &mut r,
                )
            };
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak.is_finite() && peak < 6.0, "feedback {fb}: peak {peak}");
        assert!(f.buf.iter().all(|v| v.is_finite()));
    }
}

fn phaser_wet_left_db_with_the_lfo_frozen_mid_sweep(hz: f64, feedback: f32) -> f64 {
    let n = FS as usize / 2;
    let input = sine(hz, 0.25, n);
    let (l, _) = second_half_through_mod_at_full_mix(SID_MOD_PHASER, &input, 0.0, 0.5, feedback);
    db(tone_amp(&l, hz) / tone_amp(&input[n / 2..], hz))
}

#[test]
fn phaser_has_notches_and_quadrature_sides() {
    let mut worst = (0.0, 0.0f64);
    let mut hz = 100.0;
    while hz < 12000.0 {
        let g = phaser_wet_left_db_with_the_lfo_frozen_mid_sweep(hz, 0.0);
        if g < worst.1 {
            worst = (hz, g);
        }
        hz *= 1.03;
    }
    println!("phaser notch: {:.1} dB at {:.0} Hz", worst.1, worst.0);
    assert!(worst.1 < -20.0, "no notch: deepest {:.1} dB", worst.1);
    let peak = |fb: f32| {
        let mut best = -100.0f64;
        let mut hz = 100.0;
        while hz < 12000.0 {
            best = best.max(phaser_wet_left_db_with_the_lfo_frozen_mid_sweep(hz, fb));
            hz *= 1.05;
        }
        best
    };
    let (flat, sharp) = (peak(0.0), peak(1.0));
    println!("phaser peak: {flat:.2} dB without feedback, {sharp:.2} dB with");
    assert!(sharp > flat + 3.0);

    let mut p = sid_fx_phaser::default();
    unsafe { sid_fx_phaser_init(&mut p, FS) };
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut al = Vec::new();
    let mut ar = Vec::new();
    for _ in 0..(4 * FS) {
        unsafe { sid_fx_phaser_tick(&mut p, 0.0, 1.0, 0.5, 0.0, &mut l, &mut r) };
        al.push(p.a_l);
        ar.push(p.a_r);
    }
    let mean = |v: &[f32]| v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64;
    let (ml, mr) = (mean(&al), mean(&ar));
    let (mut c, mut vl, mut vr) = (0.0, 0.0, 0.0);
    for (&x, &y) in al.iter().zip(&ar) {
        let (x, y) = (f64::from(x) - ml, f64::from(y) - mr);
        c += x * y;
        vl += x * x;
        vr += y * y;
    }
    let rho = c / (vl * vr).sqrt();
    println!("phaser L/R coefficient correlation {rho:.3}");
    assert!(rho.abs() < 0.1);
}

#[test]
fn tremolo_depth_and_rate() {
    let n = 4 * FS as usize;
    for depth in [0.5f32, 1.0] {
        let mut t = sid_fx_tremolo::default();
        unsafe { sid_fx_tremolo_init(&mut t, FS) };
        let (mut l, mut r) = (0.0f32, 0.0f32);
        let mut gains = Vec::new();
        for _ in 0..n {
            unsafe { sid_fx_tremolo_tick(&mut t, 1.0, depth, 0.5, 0.0, &mut l, &mut r) };
            gains.push(l);
        }
        let lo = gains.iter().fold(f32::MAX, |m, &v| m.min(v));
        let hi = gains.iter().fold(f32::MIN, |m, &v| m.max(v));
        let (hz, _) = rate_and_half_peak_to_peak(&gains);
        println!("tremolo depth {depth}: gain {lo:.3} .. {hi:.3} at {hz:.3} Hz");
        assert!((hi - 1.0).abs() < 1e-3 && (lo - (1.0 - depth)).abs() < 2e-3);
        assert!((hz - 1.0).abs() < 0.02, "rate {hz} Hz, want 1 at A = 0.5");
    }
}

#[test]
fn autopan_is_equal_power_and_moves() {
    let tone = sine(500.0, 0.5, 4 * FS as usize);
    for shape in [0.0f32, 1.0] {
        let (l, r) = second_half_through_mod_at_full_mix(SID_MOD_AUTOPAN, &tone, 1.0, 0.5, shape);
        let x = &tone[tone.len() / 2..];
        let w = 441;
        let mut worst = 0.0f64;
        let mut swing = (f64::MAX, f64::MIN);
        for k in 0..(x.len() / w) {
            let sl = &l[k * w..(k + 1) * w];
            let sr = &r[k * w..(k + 1) * w];
            let p = |v: &[i16]| v.iter().map(|&a| f64::from(a).powi(2)).sum::<f64>();
            let ratio = (p(sl) + p(sr)) / (2.0 * p(&x[k * w..(k + 1) * w]));
            worst = worst.max((ratio - 1.0).abs());
            let bal = p(sl) / (p(sl) + p(sr));
            swing = (swing.0.min(bal), swing.1.max(bal));
        }
        println!(
            "auto-pan shape {shape}: power within {:.2} %, left share {:.3} .. {:.3}",
            100.0 * worst,
            swing.0,
            swing.1
        );
        assert!(worst < 0.01, "power varies by {worst}");
        assert!(swing.0 < 0.05 && swing.1 > 0.95, "does not pan fully");
    }
}

#[test]
fn modulation_parameter_jumps_are_click_free() {
    use Control::*;
    let all = [
        (Amount, 0.0, 1.0),
        (A, 0.0, 1.0),
        (B, 0.0, 1.0),
        (Mix, 0.0, 1.0),
    ];
    assert_click_free(MOD, params(SID_MOD_CHORUS, 0.7, 0.5, 0.5, 1.0), &all);
    assert_click_free(MOD, params(SID_MOD_FLANGER, 0.7, 0.5, 0.7, 1.0), &all);
    assert_click_free(MOD, params(SID_MOD_PHASER, 0.7, 0.5, 0.5, 1.0), &all);
    assert_click_free(MOD, params(SID_MOD_TREMOLO, 0.7, 0.5, 0.5, 1.0), &all);
    assert_click_free(MOD, params(SID_MOD_AUTOPAN, 0.7, 0.5, 0.5, 1.0), &all);
}

#[test]
fn switching_modulations_is_click_free() {
    let (sw, steady) = worst_second_diff_at_and_between_switches(
        MOD,
        &[
            SID_MOD_CHORUS,
            SID_MOD_FLANGER,
            SID_MOD_PHASER,
            SID_MOD_TREMOLO,
            SID_MOD_AUTOPAN,
            SID_MOD_WOW,
            SID_MOD_CHORUS,
        ],
        0.7,
        0.5,
        0.5,
    );
    println!("modulation switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady + RESIDUAL_MAX, "a switch steps by {sw}");
}
