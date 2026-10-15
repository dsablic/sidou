// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Filter sweeps, SIDOU against reSIDfp: one voice of noise (0x4000) through the filter at
//! resonance 8, its cutoff's high byte rising by 4 every frame for 64 frames (1.28 s), in
//! one mode. Compared as RMS envelopes in 20 ms windows, each normalised to its loudest
//! compared window. `sidou-compare` writes them as WAV files; the reference tests assert
//! the small-signal ones.

use sidou_sys::sid_model;

use crate::render::{render_residfp, render_sidou, RegWrite};
use crate::signal_metrics::{
    max_envelope_diff_db, normalized_to_max, rms_envelope_db, rms_envelope_diff_db,
};

/// 20 ms; noise has no period to align to.
pub const WINDOW: usize = 882;
/// Windows 0-2 (the first 60 ms) are not compared: window 0 holds the click of the 0 -> 15
/// volume step and is the loudest of all. Each curve is normalised to its loudest
/// *compared* window, so the baseline tracks the filtered noise, not the click; the floor
/// stays relative to that peak.
pub const FIRST_WINDOW: usize = 3;
pub const FLOOR_DB: f64 = -40.0;
pub const FRAMES: u32 = 64;

/// The voice's sustain nibble: 4 is the small-signal level the filter tables were fitted at
/// (sidou-tablegen fit-filter: envelope 0x44, about a quarter of full scale); 15 is full
/// scale, where the 6581's filter is level-dependent (M1b Task 6).
pub const SMALL: u8 = 0x4;
pub const FULL: u8 = 0xf;

/// The $18 mode bits: low-pass, band-pass, high-pass.
pub const MODES: [(&str, u8); 3] = [("lp", 0x10), ("bp", 0x20), ("hp", 0x40)];

pub fn writes(mode: u8, sustain: u8) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut writes = vec![
        w(0, 0x18, mode | 0x0f),
        w(0, 0x17, 0x81),
        w(0, 0x00, 0x00),
        w(0, 0x01, 0x40),
        w(0, 0x05, 0x00),
        w(0, 0x06, sustain << 4),
        w(0, 0x04, 0x81),
    ];
    writes.extend((0..FRAMES).map(|f| w(f, 0x16, (f * 4) as u8)));
    writes
}

fn envelope(x: &[f32]) -> Vec<f64> {
    let env = rms_envelope_db(x, WINDOW);
    let len = env.len();
    normalized_to_max(env, FIRST_WINDOW, len)
}

/// The loud-signal sweeps (M1b Task 6): the same cutoff sweep with all three voices at
/// full sustain routed into the filter, where the 6581's filter is furthest from linear.
/// Either three noise voices (0x4000, 0x3a00, 0x3300) or a chord (C major at about
/// 262, 330 and 392 Hz: sawtooth, 50% pulse, triangle), resonance 8.
pub const LOUD: [(&str, bool); 2] = [("noise3", false), ("chord", true)];

pub fn loud_writes(mode: u8, chord: bool) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut writes = vec![w(0, 0x18, mode | 0x0f), w(0, 0x17, 0x87)];
    let voices: [(u16, u8, u16); 3] = if chord {
        [(0x1167, 0x21, 0), (0x15ed, 0x41, 0x800), (0x1a13, 0x11, 0)]
    } else {
        [(0x4000, 0x81, 0), (0x3a00, 0x81, 0), (0x3300, 0x81, 0)]
    };
    for (v, (freq, control, pw)) in voices.into_iter().enumerate() {
        let b = (v * 7) as u8;
        writes.extend([
            w(0, b, freq as u8),
            w(0, b + 1, (freq >> 8) as u8),
            w(0, b + 2, pw as u8),
            w(0, b + 3, (pw >> 8) as u8),
            w(0, b + 5, 0x00),
            w(0, b + 6, FULL << 4),
            w(0, b + 4, control),
        ]);
    }
    writes.extend((0..FRAMES).map(|f| w(f, 0x16, (f * 4) as u8)));
    writes
}

/// One sweep on both engines.
pub struct Sweep {
    pub ours: Vec<f32>,
    pub reference: Vec<f32>,
    /// The largest envelope difference over the compared windows, in dB.
    pub max_db: f64,
    /// The RMS envelope difference over the same windows, in dB.
    pub rms_db: f64,
}

/// One sweep on both engines: SIDOU's and reSIDfp's output, and their envelope
/// differences.
pub fn sweep(model: sid_model, mode: u8, sustain: u8) -> Sweep {
    compare(model, &writes(mode, sustain))
}

/// A loud-signal sweep (loud_writes) on both engines.
pub fn sweep_loud(model: sid_model, mode: u8, chord: bool) -> Sweep {
    compare(model, &loud_writes(mode, chord))
}

fn compare(model: sid_model, w: &[RegWrite]) -> Sweep {
    let ours = render_sidou(model, w, FRAMES);
    let reference = render_residfp(model, w, FRAMES, false);
    let (a, b) = (envelope(&ours), envelope(&reference));
    Sweep {
        max_db: max_envelope_diff_db(&a, &b, FLOOR_DB, FIRST_WINDOW),
        rms_db: rms_envelope_diff_db(&a, &b, FLOOR_DB, FIRST_WINDOW),
        ours,
        reference,
    }
}
