// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

fn dft_magnitude(x: &[f32], window: &[f64], f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let mut re = 0.0;
    let mut im = 0.0;
    for (i, (&xi, &wi)) in x.iter().zip(window).enumerate() {
        re += wi * f64::from(xi) * (w * i as f64).cos();
        im -= wi * f64::from(xi) * (w * i as f64).sin();
    }
    (re * re + im * im).sqrt()
}

pub fn estimate_f0(x: &[f32], fs: f64, f_lo: f64, f_hi: f64) -> f64 {
    assert!(x.len() >= 2, "estimate_f0: need at least 2 samples");
    let window = hann_window(x.len());
    let step = fs / x.len() as f64 / 4.0;
    let coarse = coarse_dft_peak(x, &window, fs, f_lo, f_hi, step);
    golden_section_dft_peak(x, &window, fs, coarse - step, coarse + step)
}

fn hann_window(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
        .collect()
}

fn coarse_dft_peak(x: &[f32], window: &[f64], fs: f64, f_lo: f64, f_hi: f64, step: f64) -> f64 {
    let mut best_f = f_lo;
    let mut best = -1.0;
    let mut f = f_lo;
    while f <= f_hi {
        let m = dft_magnitude(x, window, f, fs);
        if m > best {
            best = m;
            best_f = f;
        }
        f += step;
    }
    best_f
}

fn golden_section_dft_peak(x: &[f32], window: &[f64], fs: f64, mut a: f64, mut b: f64) -> f64 {
    let g = (5.0f64.sqrt() - 1.0) / 2.0;
    let mut c = b - g * (b - a);
    let mut d = a + g * (b - a);
    let mut mc = dft_magnitude(x, window, c, fs);
    let mut md = dft_magnitude(x, window, d, fs);
    while b - a > 1e-4 {
        if mc > md {
            b = d;
            d = c;
            md = mc;
            c = b - g * (b - a);
            mc = dft_magnitude(x, window, c, fs);
        } else {
            a = c;
            c = d;
            mc = md;
            d = a + g * (b - a);
            md = dft_magnitude(x, window, d, fs);
        }
    }
    0.5 * (a + b)
}

pub fn rms_envelope_db(x: &[f32], w: usize) -> Vec<f64> {
    assert!(w != 0, "rms_envelope_db: window must not be empty");
    x.chunks_exact(w)
        .map(|win| {
            let mut mean = 0.0;
            for &v in win {
                mean += f64::from(v);
            }
            mean /= w as f64;
            let mut sq = 0.0;
            for &v in win {
                sq += (f64::from(v) - mean) * (f64::from(v) - mean);
            }
            10.0 * (sq / w as f64 + 1e-20).log10()
        })
        .collect()
}

fn check_range(env: &[f64], first: usize, last: usize, what: &str) {
    assert!(
        first < last && last <= env.len(),
        "{what}: bad window range"
    );
}

pub fn normalized_to_max(mut env: Vec<f64>, first: usize, last: usize) -> Vec<f64> {
    check_range(&env, first, last, "normalized_to_max");
    let mut reference = env[first];
    for &v in &env[first + 1..last] {
        if reference < v {
            reference = v;
        }
    }
    for v in &mut env {
        *v -= reference;
    }
    env
}

pub fn normalized_to_mean(mut env: Vec<f64>, first: usize, last: usize) -> Vec<f64> {
    check_range(&env, first, last, "normalized_to_mean");
    let mut sum = 0.0;
    for &v in &env[first..last] {
        sum += v;
    }
    let reference = sum / (last - first) as f64;
    for v in &mut env {
        *v -= reference;
    }
    env
}

pub fn max_envelope_diff_db(a: &[f64], b: &[f64], floor_db: f64, first: usize) -> f64 {
    let mut worst: f64 = 0.0;
    for (&ai, &bi) in a.iter().zip(b).skip(first) {
        if ai.is_nan() || bi.is_nan() {
            return f64::NAN;
        }
        if bi > floor_db {
            worst = worst.max((ai - bi).abs());
        }
    }
    worst
}

pub fn rms_envelope_diff_db(a: &[f64], b: &[f64], floor_db: f64, first: usize) -> f64 {
    let (mut sum, mut n) = (0.0, 0u32);
    for (&ai, &bi) in a.iter().zip(b).skip(first) {
        if ai.is_nan() || bi.is_nan() {
            return f64::NAN;
        }
        if bi > floor_db {
            sum += (ai - bi) * (ai - bi);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (sum / f64::from(n)).sqrt()
    }
}

const WAV_FORMAT_PCM: u16 = 1;
const WAV_CHANNELS_MONO: u16 = 1;

pub fn write_wav16(path: &Path, x: &[f32], fs: u32) -> std::io::Result<()> {
    let bytes = u32::try_from(x.len() * 2).expect("WAV data fits in 4 GiB");
    let mut f = BufWriter::new(File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + bytes).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&WAV_FORMAT_PCM.to_le_bytes())?;
    f.write_all(&WAV_CHANNELS_MONO.to_le_bytes())?;
    f.write_all(&fs.to_le_bytes())?;
    f.write_all(&(fs * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&bytes.to_le_bytes())?;
    for &s in x {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()
}
