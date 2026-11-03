// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::sid_model;
use sidou_tablegen::filter_fit::CLOCK_HZ;

use crate::render::{render_residfp, render_sidou, RegWrite, REF_SAMPLE_RATE};
use crate::signal_metrics::{dft_magnitude, estimate_f0, hann_window};

pub const HIGHEST: usize = 5;
const F0_SEARCH: f64 = 0.02;
const PROBE_FRAMES: u32 = 45;
const SETTLED_FRAMES: u32 = 20;
const SAMPLES_PER_FRAME: usize = 882;
const PHASE_ACCUMULATOR_STEPS: f64 = 16_777_216.0;
const TRIANGLE_GATE: u8 = 0x11;
const VOLUME_MAX: u8 = 0x0f;

pub fn harmonics_db(x: &[f32], fs: f64, f0: f64) -> [f64; HIGHEST - 1] {
    let window = hann_window(x.len());
    let f0 = estimate_f0(x, fs, f0 * (1.0 - F0_SEARCH), f0 * (1.0 + F0_SEARCH));
    let level = |k: usize| 20.0 * dft_magnitude(x, &window, k as f64 * f0, fs).log10();
    let fundamental = level(1);
    std::array::from_fn(|i| level(i + 2) - fundamental)
}

#[derive(Clone, Copy, Debug)]
pub struct Probe {
    pub mode: Option<u8>,
    pub fc: u32,
    pub tone_hz: f64,
    pub voices: u8,
    pub sustain: u8,
}

pub fn probe_writes(p: &Probe) -> Vec<RegWrite> {
    let w = |reg, value| RegWrite {
        frame: 0,
        reg,
        value,
    };
    let freq = (p.tone_hz * PHASE_ACCUMULATOR_STEPS / CLOCK_HZ).round() as u16;
    let (routing, mode) = match p.mode {
        Some(m) => ((1u8 << p.voices) - 1, m),
        None => (0, 0),
    };
    let mut writes = vec![
        w(0x15, (p.fc & 7) as u8),
        w(0x16, (p.fc >> 3) as u8),
        w(0x17, routing),
        w(0x18, mode | VOLUME_MAX),
    ];
    for v in 0..p.voices {
        let b = v * 7;
        writes.extend([
            w(b, freq as u8),
            w(b + 1, (freq >> 8) as u8),
            w(b + 5, 0x00),
            w(b + 6, p.sustain << 4),
        ]);
    }
    writes.extend((0..p.voices).map(|v| w(v * 7 + 4, TRIANGLE_GATE)));
    writes
}

fn settled(x: Vec<f32>) -> Vec<f32> {
    x[SETTLED_FRAMES as usize * SAMPLES_PER_FRAME..].to_vec()
}

pub struct ProbeResult {
    pub sidou: [f64; HIGHEST - 1],
    pub residfp: [f64; HIGHEST - 1],
}

pub fn probe(model: sid_model, p: &Probe) -> ProbeResult {
    let writes = probe_writes(p);
    let measure = |x: Vec<f32>| harmonics_db(&settled(x), REF_SAMPLE_RATE, p.tone_hz);
    ProbeResult {
        sidou: measure(render_sidou(model, &writes, PROBE_FRAMES)),
        residfp: measure(render_residfp(model, &writes, PROBE_FRAMES, false)),
    }
}
