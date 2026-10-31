// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_tests::below_and_not_nan;

use sidou_sys::{sid_model, SID_CLOCK_PAL, SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::render::{render_residfp, render_sidou, RegWrite, REF_SAMPLE_RATE};
use sidou_tests::residfp_engine::residfp_decimate_rate;
use sidou_tests::signal_metrics::{
    estimate_f0, max_envelope_diff_db, normalized_to_mean, rms_envelope_db,
};

const MAX_PITCH_CENTS: f64 = 0.1;
const MAX_ENVELOPE_DB_8580: f64 = 0.2;
const MAX_ENVELOPE_DB_6581: f64 = 0.5;
const ENVELOPE_FLOOR_DB: f64 = -20.0;

const MAX_LEVEL_DB: f64 = 0.2;

const SAW_A4_HZ: f64 = 0x1d45 as f64 * SID_CLOCK_PAL as f64 / 16777216.0;

const ENVELOPE_WINDOW: usize = 401;
const SKIP_WINDOWS: usize = 1;
const SUSTAIN_FIRST: usize = 17;
const SUSTAIN_LAST: usize = 43;
const DECIMATED: bool = true;
const PAST_THE_ATTACK_AND_DC_SETTLING: std::ops::Range<usize> = 17640..52920;

fn saw_a4(ad: u8, sr: u8, gate_off_frame: u32) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    vec![
        w(0, 0x18, 0x0f),
        w(0, 0x00, 0x45),
        w(0, 0x01, 0x1d),
        w(0, 0x05, ad),
        w(0, 0x06, sr),
        w(0, 0x04, 0x21),
        w(gate_off_frame, 0x04, 0x20),
    ]
}

fn cents(a: f64, b: f64) -> f64 {
    (1200.0 * (a / b).log2()).abs()
}

fn model_name(model: sid_model) -> &'static str {
    if model == SID_MODEL_6581 {
        "MOS6581"
    } else {
        "CSG8580"
    }
}

fn unfiltered_saw_pitch_matches(model: sid_model) {
    let writes = saw_a4(0x00, 0xf0, 1000);
    let ours = render_sidou(model, &writes, 60);
    let reference = render_residfp(model, &writes, 60, DECIMATED);
    const START: usize = 4410;
    const LENGTH: usize = 44100;
    assert!(ours.len() >= START + LENGTH);
    assert!(reference.len() >= START + LENGTH);
    let a = &ours[START..START + LENGTH];
    let b = &reference[START..START + LENGTH];
    let fa = estimate_f0(a, REF_SAMPLE_RATE, 400.0, 480.0);
    let fb = estimate_f0(b, residfp_decimate_rate(REF_SAMPLE_RATE), 400.0, 480.0);
    println!(
        "{}: sidou {fa:.9} Hz, reSIDfp {fb:.9} Hz; cents sidou-theory {:.6}, \
         reSIDfp-theory {:.6}, sidou-reSIDfp {:.6}",
        model_name(model),
        cents(fa, SAW_A4_HZ),
        cents(fb, SAW_A4_HZ),
        cents(fa, fb)
    );
    let mut failures = Vec::new();
    if !below_and_not_nan(cents(fa, SAW_A4_HZ), MAX_PITCH_CENTS) {
        failures.push(format!("sidou {fa} Hz"));
    }
    if !below_and_not_nan(cents(fb, SAW_A4_HZ), MAX_PITCH_CENTS) {
        failures.push(format!(
            "reSIDfp {fb} Hz; the reference itself is off, check the harness"
        ));
    }
    if !below_and_not_nan(cents(fa, fb), MAX_PITCH_CENTS) {
        failures.push(format!("sidou {fa} Hz vs reSIDfp {fb} Hz"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn envelope_shape_matches(model: sid_model) {
    assert_eq!(
        ENVELOPE_WINDOW,
        (4.0 * REF_SAMPLE_RATE / SAW_A4_HZ).round() as usize
    );
    let writes = saw_a4(0x29, 0xa9, 20);
    let ours = rms_envelope_db(&render_sidou(model, &writes, 50), ENVELOPE_WINDOW);
    let reference = rms_envelope_db(
        &render_residfp(model, &writes, 50, DECIMATED),
        ENVELOPE_WINDOW,
    );
    assert!(ours.len() >= SUSTAIN_LAST);
    assert!(reference.len() >= SUSTAIN_LAST);
    let ours = normalized_to_mean(ours, SUSTAIN_FIRST, SUSTAIN_LAST);
    let reference = normalized_to_mean(reference, SUSTAIN_FIRST, SUSTAIN_LAST);
    let tolerance = if model == SID_MODEL_6581 {
        MAX_ENVELOPE_DB_6581
    } else {
        MAX_ENVELOPE_DB_8580
    };
    let diff = max_envelope_diff_db(&ours, &reference, ENVELOPE_FLOOR_DB, SKIP_WINDOWS);
    println!("{}: max envelope diff {diff:.6} dB", model_name(model));
    assert!(
        below_and_not_nan(diff, tolerance),
        "{diff} dB, tolerance {tolerance} dB"
    );
}

fn rms(x: &[f32]) -> f64 {
    let mean = x.iter().map(|&v| f64::from(v)).sum::<f64>() / x.len() as f64;
    let power = x
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / x.len() as f64;
    power.sqrt()
}

fn unfiltered_level_matches(model: sid_model) {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut failures = Vec::new();
    for (wave, control, pw_hi) in [
        ("saw", 0x21u8, 0u8),
        ("triangle", 0x11, 0),
        ("pulse", 0x41, 8),
    ] {
        for (lo, hi) in [(0x45u8, 0x1du8), (0xd6, 0x08)] {
            let writes = [
                w(0, 0x18, 0x0f),
                w(0, 0x00, lo),
                w(0, 0x01, hi),
                w(0, 0x03, pw_hi),
                w(0, 0x05, 0x00),
                w(0, 0x06, 0xf0),
                w(0, 0x04, control),
            ];
            let range = PAST_THE_ATTACK_AND_DC_SETTLING;
            let ours = rms(&render_sidou(model, &writes, 60)[range.clone()]);
            let reference = rms(&render_residfp(model, &writes, 60, !DECIMATED)[range]);
            let db = 20.0 * (ours / reference).log10();
            println!(
                "{}: {wave} at {hi:02x}{lo:02x}: {db:+.3} dB",
                model_name(model)
            );
            if !below_and_not_nan(db.abs(), MAX_LEVEL_DB) {
                failures.push(format!("{wave} at {hi:02x}{lo:02x}: {db:+.3} dB"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn unfiltered_level_matches_mos6581() {
    unfiltered_level_matches(SID_MODEL_6581);
}

#[test]
fn unfiltered_level_matches_csg8580() {
    unfiltered_level_matches(SID_MODEL_8580);
}

#[test]
fn unfiltered_saw_pitch_matches_mos6581() {
    unfiltered_saw_pitch_matches(SID_MODEL_6581);
}

#[test]
fn unfiltered_saw_pitch_matches_csg8580() {
    unfiltered_saw_pitch_matches(SID_MODEL_8580);
}

#[test]
fn envelope_shape_matches_mos6581() {
    envelope_shape_matches(SID_MODEL_6581);
}

#[test]
fn envelope_shape_matches_csg8580() {
    envelope_shape_matches(SID_MODEL_8580);
}
