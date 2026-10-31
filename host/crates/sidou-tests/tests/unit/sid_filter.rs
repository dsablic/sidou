// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

use crate::{assert_float_eq, assert_near, chip};

const FS: f32 = crate::FS as f32;
const FC_HI_1024: u8 = 0x80;
const VOICES_1_TO_3_FILTERED_AT_RESONANCE_0: u8 = 0x07;
const STALE_DAMPING: f32 = 9.0;
const PI: f32 = SIDOU_PI as f32;
const _: () = assert!(PI == std::f32::consts::PI);

fn tanf(x: f32) -> f32 {
    unsafe { sidou_tanf(x) }
}

fn tick(f: &mut sid_svf, v0: f32) {
    unsafe { sid_svf_tick(f, v0) }
}

fn linear_svf(hz: f32, q: f32) -> sid_svf {
    let mut f = sid_svf::default();
    unsafe {
        sid_svf_reset(&mut f);
        sid_svf_set(&mut f, tanf(PI * hz / FS), 1.0 / q);
    }
    f
}

fn tick_6581(f: &mut sid_svf, n: &sid_nonlin, v0: f32) {
    unsafe { sid_svf_tick_6581(f, n, v0) }
}

fn svf_6581_without_dc_load(n: &sid_nonlin, hz: f32, q: f32) -> sid_svf {
    let mut f = sid_svf::default();
    unsafe {
        sid_svf_reset(&mut f);
        sid_svf_set_6581(&mut f, n, tanf(PI * hz / FS), 1.0 / q, 0.0);
    }
    f
}

fn default_nonlin() -> sid_nonlin {
    chip(SID_MODEL_6581).nl
}

fn cutoff(model: sid_model, fc: u32) -> f32 {
    unsafe { sid_cutoff_hz(model, 0, fc) }
}

#[test]
fn tan_matches_libm_on_the_cutoff_range() {
    let mut x = 0.0f32;
    while x < 1.35 {
        assert_near!(tanf(x), x.tan(), 2e-5 * x.tan() + 1e-7, "x={x}");
        x += 0.01;
    }
}

#[test]
fn low_pass_passes_dc_and_high_pass_blocks_it() {
    let mut f = linear_svf(1000.0, 0.707);
    for _ in 0..44100 {
        tick(&mut f, 0.5);
    }
    assert_near!(f.lp, 0.5f32, 1e-4);
    assert_near!(f.hp, 0.0f32, 1e-4);
}

#[test]
fn low_pass_gain_at_cutoff_equals_q() {
    let (hz, q) = (1000.0f32, 0.707f32);
    let mut f = linear_svf(hz, q);
    let mut peak = 0.0f32;
    for i in 0..44100 {
        let x = (2.0 * PI * hz * i as f32 / FS).sin();
        tick(&mut f, x);
        if i > 22050 {
            peak = peak.max(f.lp.abs());
        }
    }
    assert_near!(peak, q, 0.01);
}

#[test]
fn the_6581_tick_without_non_linearity_is_the_linear_filter() {
    let n = sid_nonlin::default();
    for (hz, q) in [(240.0f32, 0.8f32), (2000.0, 3.0), (12000.0, 0.7)] {
        let mut linear = linear_svf(hz, q);
        let mut shaped = svf_6581_without_dc_load(&n, hz, q);
        for i in 0..4410 {
            let x = 1.5 * (2.0 * PI * 440.0 * i as f32 / FS).sin() + 0.8;
            tick(&mut linear, x);
            tick_6581(&mut shaped, &n, x);
            assert_near!(shaped.lp, linear.lp, 1e-4, "hz={hz} i={i}");
            assert_near!(shaped.bp, linear.bp, 1e-4, "hz={hz} i={i}");
            assert_near!(shaped.hp, linear.hp, 1e-4, "hz={hz} i={i}");
            assert_near!(shaped.bpn / shaped.k, linear.bp, 1e-3, "hz={hz} i={i}");
        }
    }
}

fn lp_rms_gain_6581_of_a_1khz_square(n: &sid_nonlin, hz: f32, swing: f32, dc: f32) -> f32 {
    let mut f = svf_6581_without_dc_load(n, hz, 1.0);
    let mut sum = 0.0f32;
    let mut mean = 0.0f32;
    let mut out = Vec::new();
    for i in 0..8820 {
        let x = dc + if (i / 22) % 2 == 0 { swing } else { -swing };
        tick_6581(&mut f, n, x);
        if i >= 4410 {
            out.push(f.lp);
            mean += f.lp;
        }
    }
    mean /= out.len() as f32;
    for v in &out {
        sum += (v - mean) * (v - mean);
    }
    (sum / out.len() as f32).sqrt() / swing
}

#[test]
fn the_6581_cutoff_rises_with_swing_and_falls_with_load() {
    let n = default_nonlin();
    let quiet = lp_rms_gain_6581_of_a_1khz_square(&n, 240.0, 0.1, 0.0);
    let loud = lp_rms_gain_6581_of_a_1khz_square(&n, 240.0, 1.0, 0.0);
    assert!(loud > 1.5 * quiet, "threshold region: {quiet} -> {loud}");
    let unloaded = lp_rms_gain_6581_of_a_1khz_square(&n, 1000.0, 0.1, n.load_ref);
    let loaded = lp_rms_gain_6581_of_a_1khz_square(&n, 1000.0, 0.1, n.load_ref + 3.0);
    assert!(loaded < 0.8 * unloaded, "loaded: {unloaded} -> {loaded}");
}

#[test]
fn the_6581_filter_stays_bounded_under_extreme_input() {
    let n = default_nonlin();
    let mut seed = 0x1234_5678u32;
    for model_fc in [0u32, 200, 600, 1023, 1024, 1500, 2047] {
        for res in [0u32, 8, 15] {
            let mut f = sid_svf::default();
            unsafe {
                sid_svf_reset(&mut f);
                let hz = cutoff(SID_MODEL_6581, model_fc).clamp(30.0, 0.45 * FS);
                let k = damping(SID_MODEL_6581, res);
                sid_svf_set_6581(&mut f, &n, tanf(PI * hz / FS), k, 3.5);
            }
            for i in 0..44100 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let x = match (i / 4410) % 4 {
                    0 => 12.0,
                    1 => -12.0,
                    2 => (seed >> 8) as f32 / (1u32 << 24) as f32 * 24.0 - 12.0,
                    _ => {
                        if (i / 3) % 2 == 0 {
                            12.0
                        } else {
                            -12.0
                        }
                    }
                };
                tick_6581(&mut f, &n, x);
                for v in [f.lp, f.bp, f.hp, f.bpn, f.ic1, f.ic2] {
                    assert!(
                        v.is_finite() && v.abs() < 1e3,
                        "fc {model_fc} res {res} i {i}: {v}"
                    );
                }
            }
        }
    }
}

#[test]
fn silence_never_leaves_subnormal_state() {
    let n = default_nonlin();
    for hz in [30.0f32, 220.0, 6000.0] {
        for shaped in [false, true] {
            let mut f = if shaped {
                svf_6581_without_dc_load(&n, hz, 0.707)
            } else {
                linear_svf(hz, 0.707)
            };
            let step = |f: &mut sid_svf, x: f32| {
                if shaped {
                    tick_6581(f, &n, x)
                } else {
                    tick(f, x)
                }
            };
            for i in 0..64 {
                step(&mut f, if (i / 8) % 2 != 0 { 1.0 } else { -1.0 });
            }
            for i in 0..5 * 44100 {
                step(&mut f, 0.0);
                for v in [f.ic1, f.ic2, f.lp, f.bp, f.hp] {
                    assert!(!v.is_subnormal(), "hz={hz} 6581={shaped} i={i}");
                }
            }
        }
    }
}

fn damping(model: sid_model, res: u32) -> f32 {
    unsafe { sid_resonance_k(model, 0, res) }
}

#[test]
fn the_8580_cutoff_rises_with_fc() {
    for fc in 1..2048 {
        assert!(
            cutoff(SID_MODEL_8580, fc) > cutoff(SID_MODEL_8580, fc - 1),
            "fc {fc}"
        );
    }
}

#[test]
fn the_6581_cutoff_follows_its_cutoff_dac() {
    let dac = residfp_sys::filter6581_dac(0.5);
    let mut order: Vec<u32> = (0..2048).collect();
    order.sort_by_key(|&fc| (dac[fc as usize], fc));
    for w in order.windows(2) {
        assert!(
            cutoff(SID_MODEL_6581, w[1]) >= cutoff(SID_MODEL_6581, w[0]),
            "fc {} -> {}",
            w[0],
            w[1]
        );
    }
    assert!(cutoff(SID_MODEL_6581, 1024) < cutoff(SID_MODEL_6581, 1023));
    assert!(cutoff(SID_MODEL_6581, 2047) > cutoff(SID_MODEL_6581, 0));
}

#[test]
fn resonance_lowers_the_damping() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        for res in 1..16 {
            assert!(damping(model, res) < damping(model, res - 1), "res {res}");
        }
        assert!(damping(model, 15) > 0.2);
        assert!(damping(model, 0) < 2.0);
    }
}

#[test]
fn the_chip_takes_cutoff_and_damping_from_the_tables() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut s = chip(model);
        unsafe {
            sid_write(&mut *s, 0x17, 0xa1);
            sid_write(&mut *s, 0x15, 0x05);
            sid_write(&mut *s, 0x16, 0x60);
        }
        crate::settle_filter_coefficients(&mut s);
        assert_eq!(s.svf.k, damping(model, 10));
        let fc = (0x60 << 3) | 5;
        let g = tanf(PI * cutoff(model, fc) / FS);
        assert_float_eq(s.svf.a2, g / (1.0 + g * (g + s.svf.k)));
    }
}

#[test]
fn each_model_has_its_filter_curves() {
    for (model, curves) in [(SID_MODEL_6581, 5), (SID_MODEL_8580, 2)] {
        assert_eq!(unsafe { sid_filter_curves(model) }, curves);
        for fc in [0u32, 1023, 2047] {
            assert_eq!(
                unsafe { sid_cutoff_hz(model, 7, fc) },
                cutoff(model, fc),
                "a curve out of range is the default"
            );
        }
        assert_eq!(unsafe { sid_resonance_k(model, 7, 15) }, damping(model, 15));
    }
}

#[test]
fn a_filter_curve_rebuilds_the_tables_and_applies_at_the_next_filter_write() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let fresh = chip(model);
        let mut s = chip(model);
        unsafe {
            sid_write(&mut *s, 0x16, 0x40);
            sid_write(&mut *s, 0x17, 0xf1);
        }
        crate::settle_filter_coefficients(&mut s);
        let svf = s.svf;
        s.cutoff_g.fill(0.25);
        s.res_k.fill(1.5);
        assert_eq!(unsafe { sid_set_filter_curve(&mut *s, 0) }, 0);
        assert_eq!(s.cutoff_g, fresh.cutoff_g);
        assert_eq!(s.res_k, fresh.res_k);
        assert_eq!(s.filter_curve, 0);
        assert_eq!(
            format!("{:?}", s.svf),
            format!("{svf:?}"),
            "not applied before a filter write"
        );
        let mut expect = chip(model);
        unsafe {
            sid_write(&mut *expect, 0x16, 0x40);
            sid_write(&mut *expect, 0x17, 0xf1);
            s.svf.k = STALE_DAMPING;
            sid_write(&mut *s, 0x17, 0xf1);
        }
        crate::settle_filter_coefficients(&mut s);
        crate::settle_filter_coefficients(&mut expect);
        assert_eq!(format!("{:?}", s.svf), format!("{:?}", expect.svf));
        s.cutoff_g[5] = 0.25;
        let past = unsafe { sid_filter_curves(model) };
        assert_eq!(
            unsafe { sid_set_filter_curve(&mut *s, past) },
            -1,
            "a curve the model lacks changes nothing"
        );
        assert_eq!(s.cutoff_g[5], 0.25);
        assert_eq!(s.filter_curve, 0);
    }
}

#[test]
fn a_filter_blend_runs_between_two_curve_settings() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let last = unsafe { sid_filter_curves(model) } - 1;
        let mut a = chip(model);
        let mut b = chip(model);
        let mut s = chip(model);
        let filter = |c: &mut sid| {
            unsafe {
                sid_write(c, 0x16, FC_HI_1024);
                sid_write(c, 0x17, 0xf1);
            }
            crate::settle_filter_coefficients(c);
        };
        unsafe {
            sid_set_filter_curve(&mut *a, 0);
            sid_set_filter_curve(&mut *b, last);
            assert_eq!(sid_set_filter_blend(&mut *s, 0, last, 0.0), 0);
        }
        assert_eq!(s.cutoff_g, a.cutoff_g);
        assert_eq!(s.res_k, a.res_k);
        let tangents: Vec<f32> = s.cutoff_x_to.iter().map(|&x| tanf(x)).collect();
        assert_eq!(
            tangents, b.cutoff_g,
            "the second tables are the other's angles"
        );
        assert_eq!(s.res_k_to, b.res_k);
        assert_eq!((s.filter_curve, s.filter_toward), (0, last as u8));
        filter(&mut a);
        filter(&mut b);
        filter(&mut s);
        assert_eq!(
            format!("{:?}", s.svf),
            format!("{:?}", a.svf),
            "t 0 is the first's"
        );
        let x_to = s.cutoff_x_to[1024];
        let blend = |t: f32| unsafe {
            let x = sidou_atanf(a.svf.g);
            sidou_tanf_fast(x + t * (x_to - x))
        };
        let k = |t: f32| a.svf.k + t * (b.svf.k - a.svf.k);
        for (t, want) in [(1.0f32, 1.0f32), (7.0, 1.0), (f32::NAN, 0.0), (0.5, 0.5)] {
            unsafe { sid_set_filter_blend(&mut *s, 0, last, t) };
            assert_eq!(s.filter_t, want, "t {t} clamps to {want}");
            filter(&mut s);
            let g = if want == 0.0 { a.svf.g } else { blend(want) };
            assert_eq!(s.svf.g, g, "t {t}");
            assert_eq!(s.svf.k, k(want), "t {t}");
        }
        unsafe { sid_set_filter_blend(&mut *s, 0, last, 1.0) };
        filter(&mut s);
        assert!(
            (s.svf.g / b.svf.g - 1.0).abs() < 1e-6,
            "t 1 is the other's: {} against {}",
            s.svf.g,
            b.svf.g
        );
        unsafe { sid_set_filter_blend(&mut *s, 0, last, 0.0) };
        assert_eq!(s.svf.g, blend(1.0), "not applied until a filter write");
        let past = last + 1;
        s.cutoff_g[5] = 0.25;
        assert_eq!(unsafe { sid_set_filter_blend(&mut *s, 0, past, 0.5) }, -1);
        assert_eq!(unsafe { sid_set_filter_blend(&mut *s, past, 0, 0.5) }, -1);
        assert_eq!(unsafe { sid_set_filter_toward(&mut *s, past) }, -1);
        assert_eq!(s.cutoff_g[5], 0.25);
        let mut u = chip(model);
        assert_eq!(unsafe { sid_set_filter_toward(&mut *u, last) }, 0);
        assert_eq!(u.cutoff_g, a.cutoff_g);
        assert_eq!(u.cutoff_x_to, s.cutoff_x_to);
        assert_eq!(u.filter_t, 0.0);
    }
}

#[test]
fn the_blend_is_of_the_cutoff_in_hz() {
    let mut worst = (0.0f64, 0u32, 0u32, 0u32);
    for id in 0..SID_CHIPS {
        let p = unsafe { &*sid_chip_profile_get(id) };
        let model = p.model as sid_model;
        let (a, b) = (u32::from(p.curve), u32::from(p.hot_curve));
        let mut s = chip(model);
        for age in (1..127u32).step_by(4) {
            let t = age as f32 / 127.0;
            unsafe { sid_set_filter_blend(&mut *s, a, b, t) };
            for fc in (0..2048u32).step_by(8) {
                unsafe {
                    sid_write(&mut *s, 0x15, (fc & 7) as u8);
                    sid_write(&mut *s, 0x16, (fc >> 3) as u8);
                }
                crate::settle_filter_coefficients(&mut s);
                let hz = |c: u32| {
                    let f = f64::from(unsafe { sid_cutoff_hz(model, c, fc) });
                    f.clamp(30.0, 0.45 * f64::from(FS))
                };
                let want = hz(a) + f64::from(t) * (hz(b) - hz(a));
                let got = f64::from(s.svf.g).atan() * f64::from(FS) / std::f64::consts::PI;
                let off = (got / want - 1.0).abs();
                if off > worst.0 {
                    worst = (off, id, fc, age);
                }
            }
        }
    }
    println!(
        "the blended cutoff is the Hz blend's within {:.2e} (profile {}, FC {}, age {})",
        worst.0, worst.1, worst.2, worst.3
    );
    assert!(worst.0 < 1e-5, "{worst:?}");
}

#[test]
fn the_6581_newton_step_is_the_load_damping() {
    let mut s = chip(SID_MODEL_6581);
    unsafe {
        sid_write(&mut *s, 0x15, 0x00);
        sid_write(&mut *s, 0x16, FC_HI_1024);
        sid_write(&mut *s, 0x17, VOICES_1_TO_3_FILTERED_AT_RESONANCE_0);
        sid_write(&mut *s, 0x18, 0x1f);
    }
    crate::settle_filter_coefficients(&mut s);
    let n = s.nl;
    let mut f = s.svf;
    let (g0, k) = (s.cutoff_g[1024], s.res_k[0]);
    f.ic1 = 0.1;
    f.ic2 = 0.0;
    f.hp = n.level_ref;
    tick_6581(&mut f, &n, 0.0);
    let k_run = f.bpn / f.bp;
    let m = 1.0 + n.load * (n.load_ref - 3.0 * 1.15625);
    let g = g0 * m;
    let a10 = 1.0 / (1.0 + g0 * (g0 + k));
    let a1 = a10 * (2.0 - (1.0 + g * (g + k)) * a10);
    let k_model = (1.0 / a1 - 1.0) / g - g;
    println!("FC 1024, three voices: k {k}, k' {k_run} (model {k_model}), g {g} of {g0}");
    assert!(m < 0.8, "the load barely moves the cutoff: m {m}");
    assert_near!(k_run, k_model, 1e-3 * k_model);
    assert!(
        k_run > 1.05 * k,
        "the load's damping: k' {k_run} against the table's {k}"
    );
    let exact = 1.0 / (1.0 + g * (g + k));
    assert_near!((1.0 / exact - 1.0) / g - g, k, 1e-3 * k);
}
