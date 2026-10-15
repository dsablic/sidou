// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

use crate::{assert_float_eq, assert_near};

const FS: f32 = 44100.0;
/// SIDOU_PI is a float literal in C, which bindgen exposes as f64; both round to this f32.
const PI: f32 = SIDOU_PI as f32;
const _: () = assert!(PI == std::f32::consts::PI);

fn tanf(x: f32) -> f32 {
    unsafe { sidou_tanf(x) }
}

fn tick(f: &mut sid_svf, v0: f32) {
    unsafe { sid_svf_tick(f, v0) }
}

fn svf(drive: f32, hz: f32, q: f32) -> sid_svf {
    let mut f = sid_svf::default();
    unsafe {
        sid_svf_reset(&mut f, drive);
        sid_svf_set(&mut f, tanf(PI * hz / FS), 1.0 / q);
    }
    f
}

fn linear_svf(hz: f32, q: f32) -> sid_svf {
    svf(0.0, hz, q)
}

fn cutoff(model: sid_model, fc: u32) -> f32 {
    unsafe { sid_cutoff_hz(model, fc) }
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
fn soft_clip_bounds_the_band_pass_state() {
    let mut f = svf(2.0, 2000.0, 3.0);
    for i in 0..4410 {
        tick(&mut f, if (i / 20) % 2 != 0 { 10.0 } else { -10.0 });
        assert!(f.ic1.abs() <= 1.0 / 2.0 + 1e-6);
        assert!(
            f.lp.is_finite() && f.bp.is_finite() && f.hp.is_finite(),
            "i={i}"
        );
        assert!(f.lp.abs() < 20.0, "i={i}");
        assert!(f.bp.abs() < 20.0, "i={i}");
        assert!(f.hp.abs() < 20.0, "i={i}");
    }
}

#[test]
fn soft_clip_is_near_identity_for_small_signals() {
    let (hz, q) = (1000.0f32, 0.707f32);
    let mut linear = linear_svf(hz, q);
    let mut driven = svf(2.0, hz, q);
    for i in 0..4410 {
        let x = 0.01 * (2.0 * PI * hz * i as f32 / FS).sin();
        tick(&mut linear, x);
        tick(&mut driven, x);
        assert_near!(driven.lp, linear.lp, 1e-5, "i={i}");
        assert_near!(driven.bp, linear.bp, 1e-5, "i={i}");
        assert_near!(driven.hp, linear.hp, 1e-5, "i={i}");
    }
}

#[test]
fn silence_never_leaves_subnormal_state() {
    for hz in [30.0f32, 220.0, 6000.0] {
        for drive in [0.0f32, 2.0] {
            let mut f = svf(drive, hz, 0.707);
            for i in 0..64 {
                tick(&mut f, if (i / 8) % 2 != 0 { 1.0 } else { -1.0 });
            }
            for i in 0..5 * 44100 {
                tick(&mut f, 0.0);
                for v in [f.ic1, f.ic2, f.lp, f.bp, f.hp] {
                    assert!(!v.is_subnormal(), "hz={hz} drive={drive} i={i}");
                }
            }
        }
    }
}

fn damping(model: sid_model, res: u32) -> f32 {
    unsafe { sid_resonance_k(model, res) }
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

/// The 6581's cutoff follows its kinked cutoff DAC (reSIDfp's), not FC: ordered by what
/// the DAC outputs it never falls, and where a high bit turns on it drops with the DAC.
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

/// A resonance write sets the filter's damping from the table, and an FC write its
/// cutoff, prewarped at the sample rate.
#[test]
fn the_chip_takes_cutoff_and_damping_from_the_tables() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut s = Box::<sid>::default();
        unsafe {
            sid_init(&mut *s, model, SID_CLOCK_PAL, 44100);
            sid_write(&mut *s, 0x17, 0xa1);
            sid_write(&mut *s, 0x15, 0x05);
            sid_write(&mut *s, 0x16, 0x60);
        }
        assert_eq!(s.svf.k, damping(model, 10));
        let fc = (0x60 << 3) | 5;
        let g = tanf(PI * cutoff(model, fc) / FS);
        assert_float_eq(s.svf.a2, g / (1.0 + g * (g + s.svf.k)));
    }
}
