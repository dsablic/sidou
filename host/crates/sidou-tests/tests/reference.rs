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
/* Coupled to ENVELOPE_FLOOR_DB: the 8580's worst window sits at the floor, where
 * the release-slope difference below has grown largest. Changing the floor
 * means re-checking this tolerance. */
/* Measured at M0: 0.44 (in the release, at -20 dB); since both engines power up
 * alike, without the old pre-roll that released reSIDfp's envelopes (M1b): 0.39. */
const MAX_ENVELOPE_DB_8580: f64 = 0.75;
/* reSIDfp models the 6581's non-linear envelope DAC, which M0 does not; the
 * release tail drifts ~2 dB apart by -20 dB. M1 adds the DAC and tightens this. */
const MAX_ENVELOPE_DB_6581: f64 = 2.5; /* measured at M0: 2.04; M1b power-on: 2.07 */
/* M1 investigation item: on the 8580 our release falls ~5.5% steeper than
 * reSIDfp's (55.9 vs 53.0 dB/s at M0), 1.8 dB apart by -30 dB; below
 * ENVELOPE_FLOOR_DB, so this test does not see it yet. The ENV3 test
 * (register_reads.rs) shows both engines' envelope counters releasing at the
 * same rate, so the cause is in the output path rather than the envelope timing. */
const ENVELOPE_FLOOR_DB: f64 = -20.0;

/* A4 from the register value 0x1d45, not 440 Hz. */
const SAW_A4_HZ: f64 = 0x1d45 as f64 * SID_CLOCK_PAL as f64 / 16777216.0;

/* Envelope windows are 4 periods of SAW_A4_HZ (400.9 samples at 44.1 kHz), so
 * the RMS does not ripple with the phase each window starts at. */
const ENVELOPE_WINDOW: usize = 401;
/* First 9.1 ms: start-up transient. The attack is compared from window 1 on;
 * in window 0 reSIDfp's 6581 is 2.2 dB quieter than ours, the 8580 0.24 dB apart
 * (6.3 and 0.48 dB at M0, when the engines powered up differently). */
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
