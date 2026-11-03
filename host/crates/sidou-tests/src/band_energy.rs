// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;

use sidou_tablegen::filter_fit::{fft, twiddles};

pub const WINDOW: usize = 8192;
pub const HOP: usize = WINDOW / 2;
pub const BANDS: usize = 26;
pub const FLOOR_DB: f64 = -60.0;
pub const SILENCE_DB: f64 = -40.0;

const LOWEST_THIRD_FROM_1K: i32 = -13;
const HALF_BAND_OCTAVES: f64 = 1.0 / 6.0;
const POWER_EPSILON: f64 = 1e-30;

pub type Bands = [f64; BANDS];

pub fn band_centre_hz(band: usize) -> f64 {
    1000.0 * 2f64.powf(f64::from(LOWEST_THIRD_FROM_1K + band as i32) / 3.0)
}

fn band_bins(fs: f64, window: usize) -> [(usize, usize); BANDS] {
    let bin_hz = fs / window as f64;
    std::array::from_fn(|b| {
        let centre = band_centre_hz(b);
        let lo = (centre * 2f64.powf(-HALF_BAND_OCTAVES) / bin_hz).ceil() as usize;
        let hi = (centre * 2f64.powf(HALF_BAND_OCTAVES) / bin_hz).ceil() as usize;
        (lo, hi.max(lo + 1))
    })
}

fn power_db(p: f64) -> f64 {
    10.0 * (p + POWER_EPSILON).log10()
}

pub fn band_frames(x: &[f32], fs: f64) -> Vec<Bands> {
    band_frames_with(x, fs, WINDOW, HOP)
}

pub fn band_frames_with(x: &[f32], fs: f64, length: usize, hop: usize) -> Vec<Bands> {
    let twiddle = twiddles(length);
    let window: Vec<f64> = (0..length)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / length as f64).cos())
        .collect();
    let bins = band_bins(fs, length);
    let starts = (0..).map(|k| k * hop).take_while(|s| s + length <= x.len());
    starts
        .map(|start| {
            let seg = &x[start..start + length];
            let mean = seg.iter().map(|&v| f64::from(v)).sum::<f64>() / length as f64;
            let mut re: Vec<f64> = seg
                .iter()
                .zip(&window)
                .map(|(&v, w)| (f64::from(v) - mean) * w)
                .collect();
            let mut im = vec![0.0; length];
            fft(&mut re, &mut im, &twiddle);
            bins.map(|(lo, hi)| power_db((lo..hi).map(|k| re[k] * re[k] + im[k] * im[k]).sum()))
        })
        .collect()
}

pub fn total_db(b: &Bands) -> f64 {
    power_db(b.iter().map(|&d| 10f64.powf(d / 10.0)).sum())
}

#[derive(Clone, Copy, Debug)]
pub struct Distance {
    pub shape_db: f64,
    pub level_db: f64,
    pub per_band_db: Bands,
    pub windows: usize,
}

pub fn distance(reference: &[Bands], test: &[Bands]) -> Distance {
    let loudest = reference.iter().map(total_db).fold(f64::MIN, f64::max);
    let mut shifts = Vec::new();
    let mut per_band_db = [0.0; BANDS];
    let mut abs_sum = 0.0;
    for (r, t) in reference.iter().zip(test) {
        let level = total_db(r);
        if level < loudest + SILENCE_DB {
            continue;
        }
        let shift = level - total_db(t);
        let floor = level + FLOOR_DB;
        for (b, sum) in per_band_db.iter_mut().enumerate() {
            let d = (t[b] + shift).max(floor) - r[b].max(floor);
            *sum += d;
            abs_sum += d.abs();
        }
        shifts.push(shift);
    }
    let n = shifts.len() as f64;
    let mean_shift = shifts.iter().sum::<f64>() / n;
    let level_db = (shifts.iter().map(|s| (s - mean_shift).powi(2)).sum::<f64>() / n).sqrt();
    Distance {
        shape_db: abs_sum / (n * BANDS as f64),
        level_db,
        per_band_db: per_band_db.map(|s| s / n),
        windows: shifts.len(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputStage {
    pub high_pass_hz: f64,
    pub low_pass_hz: f64,
}

pub const FLAT: OutputStage = OutputStage {
    high_pass_hz: 0.0,
    low_pass_hz: f64::INFINITY,
};

const HIGH_PASS_GRID_HZ: [f64; 7] = [0.0, 5.0, 10.0, 20.0, 40.0, 80.0, 160.0];
const LOW_PASS_GRID_HZ: [f64; 10] = [
    f64::INFINITY,
    40_000.0,
    28_000.0,
    20_000.0,
    16_000.0,
    13_000.0,
    11_000.0,
    9_000.0,
    7_500.0,
    6_000.0,
];

impl OutputStage {
    pub fn gain_db(&self, hz: f64) -> f64 {
        let hp = (hz / self.high_pass_hz).powi(2);
        let lp = (hz / self.low_pass_hz).powi(2);
        let high_pass = if self.high_pass_hz > 0.0 {
            hp / (1.0 + hp)
        } else {
            1.0
        };
        10.0 * (high_pass / (1.0 + lp)).log10()
    }

    pub fn applied(&self, frames: &[Bands]) -> Vec<Bands> {
        let gain: Bands = std::array::from_fn(|b| self.gain_db(band_centre_hz(b)));
        frames
            .iter()
            .map(|f| std::array::from_fn(|b| f[b] + gain[b]))
            .collect()
    }
}

pub fn fitted_output_stage(reference: &[Bands], test: &[Bands]) -> (OutputStage, Distance) {
    let candidates = HIGH_PASS_GRID_HZ.iter().flat_map(|&high_pass_hz| {
        LOW_PASS_GRID_HZ
            .iter()
            .map(move |&low_pass_hz| OutputStage {
                high_pass_hz,
                low_pass_hz,
            })
    });
    candidates
        .map(|stage| (stage, distance(reference, &stage.applied(test))))
        .min_by(|a, b| a.1.shape_db.total_cmp(&b.1.shape_db))
        .expect("a non-empty grid")
}
