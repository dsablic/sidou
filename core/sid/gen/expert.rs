// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline reSIDfp transfer-function anchors for the expert filter. Build sidou-tablegen
//! --release first, then compile this with rustc -O --edition=2021, -L dependency pointing
//! at host/target/release/deps and --extern sidou_tablegen / --extern residfp_sys pointing
//! at their rlibs. Run from the repository root, redirecting stdout to expert.h.

use residfp_sys::Model;
use sidou_tablegen::filter_fit::{self, CurveSetting, Mode};

fn main() {
    println!("// {}: 2026 SIDOU contributors", "SPDX-FileCopyrightText");
    println!("// {}: GPL-3.0-or-later", "SPDX-License-Identifier");
    println!("/* Generated offline by core/sid/gen/expert.rs from the pinned reSIDfp.");
    println!(" * Cutoff fitted at resonance 8, FC 0,64,..,1984,2047. Linear interpolation");
    println!(" * in FC and physical curve/range coordinates; damping remains the profile's.");
    println!(" * Invalid sub-noise-floor fits clamp to 30 Hz; above-band fits to 19845 Hz.");
    println!(" * These are sparse fitted anchors, not an exact transistor-level model. */");
    println!("#ifndef SIDOU_GEN_EXPERT_H\n#define SIDOU_GEN_EXPERT_H");
    for model in [Model::Mos6581, Model::Csg8580] {
        let ranges: &[f64] = if model == Model::Mos6581 {
            &[0.0, 0.3, 19.0 / 39.0, 1.0]
        } else {
            &[19.0 / 39.0]
        };
        let name = if model == Model::Mos6581 {
            "6581"
        } else {
            "8580"
        };
        let curves: &[f64] = if model == Model::Mos6581 {
            &[0.0, 0.25, 0.5, 0.75, 1.0]
        } else {
            &[0.0, 0.5, 1.0]
        };
        for (coordinate, values) in [("CURVE", curves), ("RANGE", ranges)] {
            print!(
                "static const float SID_EXPERT_{coordinate}_{name}[{}] = {{",
                values.len()
            );
            for value in values {
                print!("{value:.9}f,");
            }
            println!("}};");
        }
        println!(
            "static const float SID_EXPERT_HZ_{name}[{}][{}][33] = {{",
            ranges.len(),
            curves.len()
        );
        for &range in ranges {
            println!("{{");
            for &curve in curves {
                let setting = CurveSetting {
                    model,
                    curve,
                    range6581: Some(range),
                };
                filter_fit::apply_range(&setting);
                eprintln!("Measuring {}", setting.describe());
                let direct = filter_fit::direct_spectra(&setting);
                let values: Vec<f64> = std::thread::scope(|scope| {
                    let handles: Vec<_> = (0..33)
                        .map(|i| {
                            let setting = &setting;
                            let direct = &direct;
                            scope.spawn(move || {
                                let fc = (i * 64).min(2047);
                                let response = filter_fit::measure_modes(setting, direct, fc, 8);
                                let fit = filter_fit::fit(&response, None);
                                if fit.f0 > 19845.0 && fit.rms_db == [0.0; 3] {
                                    let bp = &response
                                        .iter()
                                        .find(|(mode, _)| *mode == Mode::Bp)
                                        .expect("bandpass")
                                        .1;
                                    let peak_db =
                                        bp.db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                                    if peak_db < -30.0 {
                                        30.0
                                    } else {
                                        19845.0
                                    }
                                } else {
                                    fit.f0.clamp(30.0, 19845.0)
                                }
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| h.join().expect("measurement worker"))
                        .collect()
                });
                print!("{{");
                for value in values {
                    assert!(value.is_finite() && value > 0.0);
                    print!("{value:.6}f,");
                }
                println!("}},");
            }
            println!("}},");
        }
        println!("}};");
    }
    println!("#endif");
}
