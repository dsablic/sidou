// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{holding, A4_KEY, ALGORITHM, EDIT, K1};
use common::{booted, firmware, Machine, FS};

const AMOUNT: u8 = K1;

fn take(m: &mut Machine) -> (Vec<f64>, Vec<f64>) {
    let from = m.soc.periph.audio.len() / 2;
    m.key(A4_KEY, true);
    m.run_ms(400);
    m.key(A4_KEY, false);
    m.run_ms(600);
    let f = |v: Vec<i16>| v[from..].iter().map(|&s| f64::from(s)).collect();
    (f(m.left()), f(m.right()))
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn db(a: f64, b: f64) -> f64 {
    20.0 * (a.max(1e-9) / b.max(1e-9)).log10()
}

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let w = -2.0 * std::f64::consts::PI / len as f64;
        for s in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (c, si) = ((w * k as f64).cos(), (w * k as f64).sin());
                let (a, b) = (s + k, s + k + len / 2);
                let tr = re[b] * c - im[b] * si;
                let ti = re[b] * si + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

fn bands(x: &[f64]) -> Vec<f64> {
    const N: usize = 4096;
    let mut power = vec![0.0; N / 2];
    let mut frames = 0;
    let mut s = 0;
    while s + N <= x.len() {
        let mut re: Vec<f64> = (0..N)
            .map(|i| {
                let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos();
                x[s + i] * w
            })
            .collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im);
        for k in 0..N / 2 {
            power[k] += re[k] * re[k] + im[k] * im[k];
        }
        frames += 1;
        s += N / 2;
    }
    let mut out = Vec::new();
    let mut f = 63.0f64;
    while f < 16000.0 {
        let (lo, hi) = (f / 2f64.powf(1.0 / 6.0), f * 2f64.powf(1.0 / 6.0));
        let e: f64 = (0..N / 2)
            .filter(|&k| {
                let fk = k as f64 * FS / N as f64;
                fk >= lo && fk < hi
            })
            .map(|k| power[k])
            .sum();
        out.push(10.0 * (e / frames.max(1) as f64 + 1e-3).log10());
        f *= 2f64.powf(1.0 / 3.0);
    }
    out
}

fn timbre_distance(a: &[f64], b: &[f64]) -> f64 {
    let top = a.iter().chain(b).cloned().fold(f64::MIN, f64::max);
    let d: Vec<f64> = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x.max(**y) > top - 40.0)
        .map(|(x, y)| y - x)
        .collect();
    let mean = d.iter().sum::<f64>() / d.len().max(1) as f64;
    d.iter().map(|v| (v - mean).abs()).sum::<f64>() / d.len().max(1) as f64
}

struct Measure {
    level: f64,
    timbre: f64,
}

fn measure(lo: &(Vec<f64>, Vec<f64>), hi: &(Vec<f64>, Vec<f64>)) -> Measure {
    let held = (0.1 * FS) as usize..(0.4 * FS) as usize;
    let mono = |t: &(Vec<f64>, Vec<f64>)| -> Vec<f64> {
        t.0.iter().zip(&t.1).map(|(l, r)| 0.5 * (l + r)).collect()
    };
    let (a, b) = (mono(lo), mono(hi));
    Measure {
        level: db(rms(&b[held.clone()]), rms(&a[held.clone()])),
        timbre: timbre_distance(&bands(&a[held.clone()]), &bands(&b[held.clone()])),
    }
}

const DISTORTIONS: [&str; 6] = ["OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LOFI"];
const AMOUNT_TRAVEL: i32 = sidou_sys::LIVE_SLOT_DETENTS as i32;
const PICK_SETTLE_MS: u64 = 200;
const KNOB_SETTLE_MS: u64 = 400;
const SWITCH_SETTLE_MS: u64 = 200;
const MAX_LEVEL_CHANGE_DB: f64 = 6.0;

const TIMBRE_MIN: f64 = 1.0;

#[test]
fn every_distortion_amount_from_the_edit_layer_is_heard() {
    let Some(fw) = firmware("the distortion Amount test") else {
        return;
    };
    let mut weak = Vec::new();
    for (alg, name) in DISTORTIONS.iter().enumerate().skip(1) {
        let mut m = booted(&fw);
        holding(&mut m, EDIT, |m| {
            m.turn(ALGORITHM, alg as i32);
            m.run_ms(PICK_SETTLE_MS);
            m.turn(AMOUNT, -AMOUNT_TRAVEL);
            m.run_ms(KNOB_SETTLE_MS);
        });
        m.run_ms(SWITCH_SETTLE_MS);
        let lo = take(&mut m);
        holding(&mut m, EDIT, |m| {
            m.turn(AMOUNT, AMOUNT_TRAVEL);
            m.run_ms(KNOB_SETTLE_MS);
        });
        m.run_ms(SWITCH_SETTLE_MS);
        let hi = take(&mut m);
        let r = measure(&lo, &hi);
        println!(
            "{name}: Amount bottom to top: level {:+.1} dB, timbre {:.1} dB",
            r.level, r.timbre
        );
        if r.timbre < TIMBRE_MIN || r.level.abs() > MAX_LEVEL_CHANGE_DB {
            weak.push(format!(
                "{name}: level {:+.1} dB, timbre {:.1} dB",
                r.level, r.timbre
            ));
        }
    }
    assert!(weak.is_empty(), "{weak:#?}");
}
