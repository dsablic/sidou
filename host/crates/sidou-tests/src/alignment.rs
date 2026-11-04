// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::band_energy::{band_frames_with, Bands};

pub const FRAME_HOP: usize = 256;
const FRAME_LENGTH: usize = 2048;
const RANGE_DB: f64 = 70.0;
const FINE_SPAN: isize = FRAME_HOP as isize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alignment {
    pub lag: isize,
    pub inverted: bool,
}

fn overlap(reference_len: usize, test_len: usize, lag: isize) -> (usize, usize, usize) {
    let r0 = lag.max(0) as usize;
    let t0 = (-lag).max(0) as usize;
    let n = reference_len
        .saturating_sub(r0)
        .min(test_len.saturating_sub(t0));
    (r0, t0, n)
}

fn spectrogram(x: &[f32], fs: f64) -> Vec<Bands> {
    let mut frames = band_frames_with(x, fs, FRAME_LENGTH, FRAME_HOP);
    let top = frames.iter().flatten().fold(f64::MIN, |a, &b| a.max(b));
    for v in frames.iter_mut().flatten() {
        *v = v.max(top - RANGE_DB);
    }
    frames
}

fn pearson(a: &[Bands], b: &[Bands]) -> f64 {
    let n = (a.len() * a.first().map_or(0, |f| f.len())) as f64;
    let mean = |x: &[Bands]| x.iter().flatten().sum::<f64>() / n;
    let (ma, mb) = (mean(a), mean(b));
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().flatten().zip(b.iter().flatten()) {
        ab += (x - ma) * (y - mb);
        aa += (x - ma) * (x - ma);
        bb += (y - mb) * (y - mb);
    }
    ab / (aa * bb).sqrt().max(f64::MIN_POSITIVE)
}

fn coarse_lag(reference: &[f32], test: &[f32], max_lag: usize, fs: f64) -> isize {
    let (r, t) = (spectrogram(reference, fs), spectrogram(test, fs));
    let span = (max_lag / FRAME_HOP) as isize;
    (-span..=span)
        .filter_map(|lag| {
            let (r0, t0, n) = overlap(r.len(), t.len(), lag);
            (n >= 2).then(|| (lag, pearson(&r[r0..r0 + n], &t[t0..t0 + n])))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(lag, _)| lag * FRAME_HOP as isize)
}

fn correlation(reference: &[f32], test: &[f32], lag: isize) -> f64 {
    let (r0, t0, n) = overlap(reference.len(), test.len(), lag);
    reference[r0..r0 + n]
        .iter()
        .zip(&test[t0..t0 + n])
        .map(|(&a, &b)| f64::from(a) * f64::from(b))
        .sum()
}

pub fn align(reference: &[f32], test: &[f32], max_lag: usize, fs: f64) -> Alignment {
    let coarse = coarse_lag(reference, test, max_lag, fs);
    let (lag, c) = (coarse - FINE_SPAN..=coarse + FINE_SPAN)
        .map(|lag| (lag, correlation(reference, test, lag)))
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .expect("a non-empty span");
    Alignment {
        lag,
        inverted: c < 0.0,
    }
}

pub fn overlapping<'a>(
    reference: &'a [f32],
    test: &'a [f32],
    a: Alignment,
) -> (&'a [f32], &'a [f32]) {
    let (r0, t0, n) = overlap(reference.len(), test.len(), a.lag);
    (&reference[r0..r0 + n], &test[t0..t0 + n])
}
