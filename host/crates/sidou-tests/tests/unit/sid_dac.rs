// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The voice DACs (sid_dac.c) against reSIDfp's.

use residfp_sys::{dac_table, Model};
use sidou_sys::*;

/* reSIDfp's constants (SID.cpp, FilterModelConfig6581). */
const ZERO_6581: usize = 0x380;
const ZERO_8580: usize = 0x800;
const VOICE_DC_6581: f64 = 0.2143 / 1.5;

fn built(is6581: bool) -> &'static sid_dac {
    unsafe { &*sid_dac_default(i32::from(is6581)) }
}

#[test]
fn the_dacs_are_residfps() {
    for (model, is6581) in [(Model::Mos6581, true), (Model::Csg8580, false)] {
        let d = built(is6581);
        // reSIDfp's envDAC, as floats.
        let env = dac_table(model, 8, 1.0, false);
        for (e, r) in env.iter().enumerate() {
            let ours = f64::from(d.env[e]);
            assert!((ours - r).abs() < 1e-6, "{model:?} env {e}: {ours} vs {r}");
        }
        // reSIDfp's oscDAC (the DAC minus its value at the zero), at twice its scale,
        // with the 6581's voice DC added.
        let wave = dac_table(model, 12, 1.0, is6581);
        let (zero, dc) = if is6581 {
            (wave[ZERO_6581], VOICE_DC_6581)
        } else {
            (wave[ZERO_8580], 0.0)
        };
        for (w, r) in wave.iter().enumerate() {
            let ours = f64::from(d.wave[w]);
            let want = 2.0 * (r - zero + dc);
            assert!(
                (ours - want).abs() < 1e-5,
                "{model:?} wave {w:#x}: {ours} vs {want}"
            );
        }
    }
}

#[test]
fn silence_leaks() {
    // An off bit still conducts: at envelope 0 a voice is quieter, not silent.
    let d = built(true);
    assert!(d.env[0] > 0.007 && d.env[0] < 0.008, "{}", d.env[0]);
    let d = built(false);
    assert!(d.env[0] > 0.003 && d.env[0] < 0.004, "{}", d.env[0]);
}

#[test]
fn the_shared_tables_are_the_defaults_built() {
    let mut d = Box::<sid_dac>::default();
    unsafe {
        sid_dac_build(
            &mut *d,
            1,
            SID_DAC_KINK_6581 as f32,
            SID_DAC_LEAKAGE_6581 as f32,
        )
    };
    assert_eq!(d.wave[..], built(true).wave[..]);
    assert_eq!(d.env[..], built(true).env[..]);
    // Every chip of a model shares one.
    assert!(std::ptr::eq(built(false), built(false)));
}

#[test]
fn the_kink_bends_the_6581_and_a_linear_ladder_stays_linear() {
    let build = |is6581: i32, kink: f32| {
        let mut d = Box::<sid_dac>::default();
        unsafe { sid_dac_build(&mut *d, is6581, kink, 0.0) };
        d
    };
    // The 8580's terminated ladder at 2R/R = 2: equal steps.
    let d = build(0, 2.0);
    let step = d.wave[1] - d.wave[0];
    for w in 1..4096 {
        assert!(
            (d.wave[w] - d.wave[w - 1] - step).abs() < 1e-5,
            "step at {w}"
        );
    }
    // The kink moves the 6581's midscale step.
    let (a, b) = (build(1, 2.2), build(1, 2.3));
    assert!((a.wave[0x800] - a.wave[0x7ff] - (b.wave[0x800] - b.wave[0x7ff])).abs() > 1e-4);
}

#[test]
fn sid_set_dac_swaps_the_tables() {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, SID_MODEL_8580, SID_CLOCK_PAL, 44100) };
    assert!(std::ptr::eq(s.dac, built(false)));
    let mut d = Box::<sid_dac>::default();
    unsafe {
        sid_dac_build(&mut *d, 0, 2.3, 0.01);
        sid_set_dac(&mut *s, &*d);
    }
    assert!(std::ptr::eq(s.dac, &*d));
}
