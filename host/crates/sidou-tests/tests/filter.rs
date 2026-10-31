// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_tests::below_and_not_nan;

use residfp_sys::Model;
use sidou_sys::{sid, sid_init, sid_model, SID_CLOCK_PAL, SID_MODEL_6581};
use sidou_tablegen::filter_fit::{
    compare, fit, in_reach, measure_modes, CurveSetting, Mode, Response, FS, MODES,
};
use sidou_tests::filter_limits::{
    MAX_TRANSFER_DB_6581, MAX_TRANSFER_DB_8580, PASSBAND_GAIN_DB_6581, PASSBAND_GAIN_DB_8580,
};
use sidou_tests::filter_passband::{passband, passband_offset, responses, settled_output, TOP_HZ};
use sidou_tests::filter_sweeps::{self, FULL, SMALL};

fn residfp_model(model: sid_model) -> Model {
    if model == SID_MODEL_6581 {
        Model::Mos6581
    } else {
        Model::Csg8580
    }
}

fn name(model: sid_model) -> &'static str {
    if model == SID_MODEL_6581 {
        "6581"
    } else {
        "8580"
    }
}

fn sidou_output(model: sid_model, fc: u32, res: u8, mode: Option<Mode>) -> Vec<f64> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, FS as u32) };
    settled_output(&mut s, fc, res, mode)
}

fn sidou_responses(model: sid_model, fc: u32, res: u8) -> Vec<(Mode, Response)> {
    responses(|mode| sidou_output(model, fc, res, mode))
}

struct TransferErrors {
    shape: [f64; 3],
    gain: [(f64, f64); 3],
}

fn transfer_errors(model: sid_model) -> TransferErrors {
    let setting = CurveSetting::default_for(residfp_model(model));
    let direct = sidou_tablegen::filter_fit::direct_spectra(&setting);
    let mut shape = [0.0f64; 3];
    let mut gain = [(f64::INFINITY, f64::NEG_INFINITY); 3];
    for fc in [64u32, 160, 256, 384, 512] {
        for res in [0u8, 8, 15] {
            let reference = measure_modes(&setting, &direct, fc, res);
            let f0 = fit(&reference, None).f0;
            assert!(in_reach(f0));
            let ours = sidou_responses(model, fc, res);
            let (rms, max) = compare(&reference, &ours, f0 / 2.0, (2.0 * f0).min(TOP_HZ));
            let mut offsets = [f64::NAN; 3];
            for (i, ((m, r), (_, o))) in reference.iter().zip(&ours).enumerate() {
                let (lo, hi) = passband(*m, f0);
                if let Some(g) = passband_offset(r, o, lo, hi) {
                    offsets[i] = g;
                    gain[i] = (gain[i].0.min(g), gain[i].1.max(g));
                }
            }
            println!(
                "{} fc {fc:4} res {res:2} (reSIDfp f0 {f0:7.0} Hz): rms {rms:.2?}, max {max:.2?}, \
                 passband gain {offsets:.2?} dB",
                name(model)
            );
            for i in 0..3 {
                shape[i] = shape[i].max(max[i]);
            }
        }
    }
    TransferErrors { shape, gain }
}

fn transfer_matches(model: sid_model, limits: [f64; 3], gain_limits: [(f64, f64); 3]) {
    let e = transfer_errors(model);
    let mut failures: Vec<String> = Vec::new();
    for (i, m) in MODES.iter().enumerate() {
        if !below_and_not_nan(e.shape[i], limits[i]) {
            failures.push(format!(
                "{}: {:.2} dB, tolerance {} dB",
                m.name(),
                e.shape[i],
                limits[i]
            ));
        }
        let ((lo, hi), (min, max)) = (e.gain[i], gain_limits[i]);
        println!(
            "{} {} passband gain {lo:.3} to {hi:.3} dB",
            name(model),
            m.name()
        );
        if !(below_and_not_nan(min, lo) && below_and_not_nan(hi, max)) {
            failures.push(format!(
                "{} passband gain: {lo:.3} to {hi:.3} dB, tolerance {min} to {max} dB",
                m.name()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn transfer_functions_match_csg8580() {
    transfer_matches(
        sidou_sys::SID_MODEL_8580,
        MAX_TRANSFER_DB_8580,
        PASSBAND_GAIN_DB_8580,
    );
}

#[test]
fn transfer_functions_match_mos6581() {
    transfer_matches(SID_MODEL_6581, MAX_TRANSFER_DB_6581, PASSBAND_GAIN_DB_6581);
}

const SWEEP_SMALL_8580: Limits = Limits {
    max: [1.62, 2.03, 1.70],
    rms: [0.54, 0.80, 0.87],
};
const SWEEP_SMALL_6581: Limits = Limits {
    max: [4.84, 2.46, 1.01],
    rms: [1.07, 0.68, 0.43],
};

const SWEEP_FULL_8580: Limits = Limits {
    max: [0.83, 2.05, 1.27],
    rms: [0.39, 0.70, 0.49],
};
const SWEEP_FULL_6581: Limits = Limits {
    max: [1.88, 1.89, 1.49],
    rms: [0.52, 0.58, 0.84],
};

struct Limits {
    max: [f64; 3],
    rms: [f64; 3],
}

fn sweeps_match(model: sid_model, sustain: u8, limits: Limits) {
    let mut failures = Vec::new();
    for (i, (mode_name, mode)) in filter_sweeps::MODES.iter().enumerate() {
        let d = filter_sweeps::sweep(model, *mode, sustain);
        println!(
            "{} {mode_name} sustain {sustain:#x}: max {:.4} dB, rms {:.4} dB",
            name(model),
            d.max_db,
            d.rms_db
        );
        for (what, x, limit) in [
            ("max", d.max_db, limits.max[i]),
            ("rms", d.rms_db, limits.rms[i]),
        ] {
            if !below_and_not_nan(x, limit) {
                failures.push(format!(
                    "{mode_name} {what}: {x:.4} dB, tolerance {limit} dB"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn filter_sweeps_match_csg8580() {
    sweeps_match(sidou_sys::SID_MODEL_8580, SMALL, SWEEP_SMALL_8580);
}

#[test]
fn filter_sweeps_match_mos6581() {
    sweeps_match(SID_MODEL_6581, SMALL, SWEEP_SMALL_6581);
}

#[test]
fn full_scale_filter_sweeps_csg8580() {
    sweeps_match(sidou_sys::SID_MODEL_8580, FULL, SWEEP_FULL_8580);
}

#[test]
fn full_scale_filter_sweeps_mos6581() {
    sweeps_match(SID_MODEL_6581, FULL, SWEEP_FULL_6581);
}

const LOUD_6581: [Limits; 2] = [
    Limits {
        max: [2.44, 1.92, 2.81],
        rms: [0.96, 0.74, 1.13],
    },
    Limits {
        max: [2.33, 4.37, 2.84],
        rms: [0.73, 3.49, 1.60],
    },
];

#[test]
fn loud_filter_sweeps_mos6581() {
    let mut failures = Vec::new();
    for ((what, chord), limits) in filter_sweeps::LOUD.iter().zip(&LOUD_6581) {
        for (i, (mode_name, mode)) in filter_sweeps::MODES.iter().enumerate() {
            let d = filter_sweeps::sweep_loud(SID_MODEL_6581, *mode, *chord);
            println!(
                "6581 {what} {mode_name}: max {:.4} dB, rms {:.4} dB",
                d.max_db, d.rms_db
            );
            for (stat, x, limit) in [
                ("max", d.max_db, limits.max[i]),
                ("rms", d.rms_db, limits.rms[i]),
            ] {
                if !below_and_not_nan(x, limit) {
                    failures.push(format!(
                        "{what} {mode_name} {stat}: {x:.4} dB, tolerance {limit} dB"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
