// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Every measured cutoff survives the flash encoding and the C decoder within 2.5e-4.

use residfp_sys::Model;
use sidou_sys::{sid_cutoff_hz, sid_filter_curves, SID_MODEL_6581, SID_MODEL_8580};

#[test]
fn every_cutoff_code_decodes_within_the_relative_error_bound() {
    let header =
        std::fs::read_to_string(sidou_tablegen::repo_root().join("core/sid/gen/filter.h")).unwrap();
    for model in ["6581", "8580"] {
        assert!(header.contains(&format!("static const uint16_t SID_CUTOFF_HZ_{model}")));
    }
    let fits = sidou_tablegen::filter_fits();
    let mut worst = 0.0f64;
    let mut entries = 0;
    for (model, c_model) in [
        (Model::Mos6581, SID_MODEL_6581),
        (Model::Csg8580, SID_MODEL_8580),
    ] {
        let rows: Vec<_> = fits.iter().filter(|f| f.setting.model == model).collect();
        assert_eq!(rows.len(), unsafe { sid_filter_curves(c_model) } as usize);
        for (curve, fit) in rows.iter().enumerate() {
            assert_eq!(fit.cutoff_hz.len(), 2048);
            for (fc, hz) in fit.cutoff_hz.iter().enumerate() {
                let original = format!("{hz:.1}").parse::<f32>().unwrap().max(0.5);
                let decoded = unsafe { sid_cutoff_hz(c_model, curve as u32, fc as u32) };
                let error = (f64::from(decoded) / f64::from(original) - 1.0).abs();
                assert!(
                    error <= 2.5e-4,
                    "{model:?} curve {curve} FC {fc}: {original} -> {decoded}"
                );
                let code = (original.to_bits() - (126 << 23) + (1 << 10)) >> 11;
                assert!(u16::try_from(code).is_ok());
                assert_eq!(decoded.to_bits(), (code << 11) + (126 << 23));
                worst = worst.max(error);
                entries += 1;
            }
        }
    }
    println!("{entries} cutoff codes; worst relative error {worst:.9e}");
}
