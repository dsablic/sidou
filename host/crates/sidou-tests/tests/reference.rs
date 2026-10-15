// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU against reSIDfp: pitch and envelope shape, for both chip models.

use std::cmp::Ordering;

use sidou_sys::{sid_model, SID_CLOCK_PAL, SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::render::{render_residfp, render_sidou, RegWrite, REF_SAMPLE_RATE};
use sidou_tests::residfp_engine::residfp_decimate_rate;
use sidou_tests::signal_metrics::{
    estimate_f0, max_envelope_diff_db, normalized_to_mean, rms_envelope_db,
};

/* M0 tolerances. Tighten them in M1; never loosen them to make a failure pass
 * without writing down why. */
/* Measured at M0: sidou 0.0014 cents from theory and from reSIDfp; reSIDfp
 * < 0.0001 cents from theory. Since both engines power up alike (M1b): sidou
 * 0.0002 cents from theory and from reSIDfp. */
const MAX_PITCH_CENTS: f64 = 0.1;
/* Coupled to ENVELOPE_FLOOR_DB: the 8580's worst window sat at the floor, where the
 * release-slope difference below had grown largest. Changing the floor means
 * re-checking this tolerance. */
/* Measured at M0: 0.44 (in the release, at -20 dB); since both engines power up
 * alike, without the old pre-roll that released reSIDfp's envelopes (M1b): 0.39;
 * with ENV3 bit-exact (M1b Task 2): 0.40; with reSIDfp's DACs (M1b Task 4): 0.030. */
const MAX_ENVELOPE_DB_8580: f64 = 0.2;
/* Measured at M0, without the 6581's kinked envelope DAC: 2.04; M1b: 2.07; with
 * reSIDfp's DACs and envelope-following voice DC (M1b Task 4): 0.31. */
const MAX_ENVELOPE_DB_6581: f64 = 0.5;
/* The 8580's release used to fall ~5.5% steeper than reSIDfp's (55.9 vs 53.0 dB/s
 * at M0), with both envelope counters releasing alike (register_reads.rs): an
 * output-path difference. It was the envelope DAC's leakage, which keeps an off bit
 * conducting (0.35% of its weight on the 8580): with it, 53.2 vs 52.8 dB/s (M1b
 * Task 4), and 0.14 dB apart at -30 dB. Below that both engines near the leakage
 * floor and drift apart (1.5 dB at -40 dB), so the floor stays at -20 dB. */
const ENVELOPE_FLOOR_DB: f64 = -20.0;

/* One unfiltered voice's RMS level against reSIDfp's, sawtooth, triangle and 50% pulse
 * at A4 (0x1d45) and A2 (0x08d6): measured within 0.12 dB on both models once the
 * output is scaled to reSIDfp's level (M1b). The 0.2 dB margin covers the spread
 * between waveforms and pitches: reSIDfp's output stage rolls off the top harmonics a
 * little (0.25 dB less for a bright wave at 0x4000), and its 6581 mixer compresses
 * louder mixes (three voices: 0.5-2 dB, a Task 6 item), so only one voice is compared. */
const MAX_LEVEL_DB: f64 = 0.2;

/* A4 from the register value 0x1d45, not 440 Hz. */
const SAW_A4_HZ: f64 = 0x1d45 as f64 * SID_CLOCK_PAL as f64 / 16777216.0;

/* Envelope windows are 4 periods of SAW_A4_HZ (400.9 samples at 44.1 kHz), so
 * the RMS does not ripple with the phase each window starts at. */
const ENVELOPE_WINDOW: usize = 401;
/* First 9.1 ms: start-up transient. The attack is compared from window 1 on;
 * in window 0 the 6581s are 0.29 dB apart, the 8580s 0.25 dB (6.3 and 0.48 dB at
 * M0, when the engines powered up differently; 2.2 and 0.24 dB before M1b's DACs). */
const SKIP_WINDOWS: usize = 1;
const SUSTAIN_FIRST: usize = 17; /* sustain plateau from 154.6 ms ... */
const SUSTAIN_LAST: usize = 43; /* ... to 391.0 ms (exclusive); gate off at 400 ms */

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

/// x < limit, false for NaN, so a NaN measurement fails.
fn below(x: f64, limit: f64) -> bool {
    x.partial_cmp(&limit) == Some(Ordering::Less)
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
    let reference = render_residfp(model, &writes, 60, /*decimate=*/ true);
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
    if !below(cents(fa, SAW_A4_HZ), MAX_PITCH_CENTS) {
        failures.push(format!("sidou {fa} Hz"));
    }
    if !below(cents(fb, SAW_A4_HZ), MAX_PITCH_CENTS) {
        failures.push(format!(
            "reSIDfp {fb} Hz; the reference itself is off, check the harness"
        ));
    }
    if !below(cents(fa, fb), MAX_PITCH_CENTS) {
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
        &render_residfp(model, &writes, 50, /*decimate=*/ true),
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
        below(diff, tolerance),
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
            // From 0.4 s, past the attack and the DC blockers' settling, for 0.8 s.
            let range = 17640..52920;
            let ours = rms(&render_sidou(model, &writes, 60)[range.clone()]);
            let reference = rms(&render_residfp(model, &writes, 60, /*decimate=*/ false)[range]);
            let db = 20.0 * (ours / reference).log10();
            println!(
                "{}: {wave} at {hi:02x}{lo:02x}: {db:+.3} dB",
                model_name(model)
            );
            if !below(db.abs(), MAX_LEVEL_DB) {
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
