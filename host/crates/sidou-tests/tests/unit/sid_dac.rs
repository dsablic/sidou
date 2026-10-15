// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The voice DACs (sid_dac.c) against reSIDfp's.

use residfp_sys::{dac_table, Model};
use sidou_sys::*;

/* reSIDfp's constants (SID.cpp, FilterModelConfig6581). */
const ZERO_6581: usize = 0x380;
const ZERO_8580: usize = 0x800;
const VOICE_DC_6581: f64 = 0.2143 / 1.5;

fn built(is6581: bool) -> Box<sid_dac> {
    let mut d = Box::<sid_dac>::default();
    unsafe { sid_dac_build_default(&mut *d, i32::from(is6581)) };
    d
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
fn the_kink_is_interpolated_between_grid_points() {
    let build = |kink: f32| {
        let mut d = Box::<sid_dac>::default();
        unsafe { sid_dac_build(&mut *d, 1, kink, 0.0075) };
        d
    };
    let (a, b, mid) = (build(2.20), build(2.25), build(2.225));
    let (lo, hi) = (
        a.wave[0x7ff].min(b.wave[0x7ff]),
        a.wave[0x7ff].max(b.wave[0x7ff]),
    );
    assert!(lo < hi, "the kink moves the 6581's midscale step");
    assert!(mid.wave[0x7ff] > lo && mid.wave[0x7ff] < hi);
    // Off the grid, it clamps.
    assert_eq!(build(1.0).wave[..], build(2.0).wave[..]);
    assert_eq!(build(9.0).wave[..], build(2.4).wave[..]);
    // At reSIDfp's ratio it is exactly the default.
    assert_eq!(build(2.2).wave[..], built(true).wave[..]);
}
