// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect slots' controls from LIVE's EDIT layer, end to end on the emulated firmware: SAW
//! LEAD, an algorithm picked with EDIT + ALGORITHM, then EDIT + K1..K4 turned from the bottom to
//! the top, and the DAC's output of a played note measured at both ends.

mod common;

use common::{build_dir, Firmware, Machine, FS};

const EDIT: u8 = 6;
const SCL: u8 = 3;
const ALGORITHM: u8 = 1;
const K1: u8 = 2;
const A4_KEY: u8 = 30;

fn tap(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// A note of SAW LEAD, 400 ms held and 600 ms after: both channels.
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

/// Third-octave band levels (dB) of x, averaged over 4096-sample Hann frames.
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

/// The mean difference (dB) of two spectra over the bands within 40 dB of either's loudest.
fn spectral_distance(a: &[f64], b: &[f64]) -> f64 {
    let top = a.iter().chain(b).cloned().fold(f64::MIN, f64::max);
    let d: Vec<f64> = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x.max(**y) > top - 40.0)
        .map(|(x, y)| (x - y).abs())
        .collect();
    d.iter().sum::<f64>() / d.len().max(1) as f64
}

/// The spectra's difference in shape: as spectral_distance, the level difference taken out.
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

/// The held part's level swing: the 10 ms levels' spread over their mean.
fn swing(x: &[f64]) -> f64 {
    let w = (FS / 100.0) as usize;
    let lv: Vec<f64> = x.chunks(w).map(rms).collect();
    let mean = lv.iter().sum::<f64>() / lv.len() as f64;
    let var = lv.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / lv.len() as f64;
    var.sqrt() / mean.max(1e-9)
}

struct Measure {
    level: f64,
    spectrum: f64,
    timbre: f64,
    swing: (f64, f64),
    tail: (f64, f64),
    side: (f64, f64),
}

fn measure(lo: &(Vec<f64>, Vec<f64>), hi: &(Vec<f64>, Vec<f64>)) -> Measure {
    let held = (0.1 * FS) as usize..(0.4 * FS) as usize;
    let tail = (0.6 * FS) as usize..(1.0 * FS) as usize - 200;
    let mono = |t: &(Vec<f64>, Vec<f64>)| -> Vec<f64> {
        t.0.iter().zip(&t.1).map(|(l, r)| 0.5 * (l + r)).collect()
    };
    let side = |t: &(Vec<f64>, Vec<f64>)| -> f64 {
        let s: Vec<f64> = t.0.iter().zip(&t.1).map(|(l, r)| 0.5 * (l - r)).collect();
        db(rms(&s[held.clone()]), rms(&mono(t)[held.clone()]))
    };
    let (a, b) = (mono(lo), mono(hi));
    Measure {
        level: db(rms(&b[held.clone()]), rms(&a[held.clone()])),
        spectrum: spectral_distance(&bands(&a[held.clone()]), &bands(&b[held.clone()])),
        timbre: timbre_distance(&bands(&a[held.clone()]), &bands(&b[held.clone()])),
        swing: (swing(&a[held.clone()]), swing(&b[held.clone()])),
        tail: (
            db(rms(&a[tail.clone()]), rms(&a[held.clone()])),
            db(rms(&b[tail.clone()]), rms(&b[held.clone()])),
        ),
        side: (side(lo), side(hi)),
    }
}

const NAMES: [[&str; 7]; 3] = [
    ["OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LOFI", ""],
    [
        "OFF", "CHORUS", "FLANGER", "PHASER", "TREMOLO", "AUTOPAN", "WOW",
    ],
    ["OFF", "DELAY", "PINGPONG", "REVERB L", "REVERB P", "", ""],
];
const FIELDS: [&str; 4] = ["AMOUNT", "A", "B", "MIX"];

/// Every slot's every algorithm and control, from the panel, at the bottom and the top of the
/// knob: what changes. Printed for the record (`--no-capture`).
fn sweep(slot: u32) {
    let Some(build) = build_dir() else {
        common::skip_message("the effect knobs test");
        return;
    };
    let fw = Firmware::load(&build);
    let counts = [6u32, 7, 5];
    {
        for alg in 1..counts[slot as usize] {
            let mut m = Machine::boot(&fw.image);
            common::run_past_boot(&mut m, &fw);
            for _ in 0..slot {
                tap(&mut m, SCL);
            }
            let dry = take(&mut m);
            let held = &dry.0[(0.1 * FS) as usize..(0.4 * FS) as usize];
            let peak = held.iter().fold(0.0f64, |p, &x| p.max(x.abs()));
            println!(
                "dry note: rms {:.1} dBFS, peak {:.1} dBFS",
                db(rms(held), 32768.0),
                db(peak, 32768.0)
            );
            m.key(EDIT, true);
            m.run_ms(30);
            m.turn(ALGORITHM, alg as i32);
            m.run_ms(200);
            m.key(EDIT, false);
            m.run_ms(300);
            let on = take(&mut m);
            let r = measure(&dry, &on);
            println!(
                "{:<9} ON     level {:+6.1} dB  spectrum {:5.1} dB  timbre {:4.1} dB  swing {:.2}->{:.2}  tail {:+6.1}->{:+6.1} dB  side {:+6.1}->{:+6.1} dB",
                NAMES[slot as usize][alg as usize], r.level, r.spectrum, r.timbre, r.swing.0, r.swing.1, r.tail.0, r.tail.1, r.side.0, r.side.1
            );
            for k in 0..4u8 {
                m.key(EDIT, true);
                m.run_ms(30);
                m.turn(K1 + k, -32);
                m.run_ms(400);
                m.key(EDIT, false);
                m.run_ms(200);
                let lo = take(&mut m);
                m.key(EDIT, true);
                m.run_ms(30);
                m.turn(K1 + k, 32);
                m.run_ms(400);
                m.key(EDIT, false);
                m.run_ms(200);
                let hi = take(&mut m);
                let r = measure(&lo, &hi);
                println!(
                    "{:<9} {:<6} level {:+6.1} dB  spectrum {:5.1} dB  timbre {:4.1} dB  swing {:.2}->{:.2}  tail {:+6.1}->{:+6.1} dB  side {:+6.1}->{:+6.1} dB",
                    NAMES[slot as usize][alg as usize],
                    FIELDS[k as usize],
                    r.level,
                    r.spectrum,
                    r.timbre,
                    r.swing.0,
                    r.swing.1,
                    r.tail.0,
                    r.tail.1,
                    r.side.0,
                    r.side.1,
                );
                // back to the middle, so the next control is heard around the default
                m.key(EDIT, true);
                m.run_ms(30);
                m.turn(K1 + k, -16);
                m.run_ms(300);
                m.key(EDIT, false);
                m.run_ms(100);
            }
        }
    }
}

// The sweeps take minutes each: run them by name (`--run-ignored only`) to measure.

#[test]
#[ignore = "a measurement: minutes of emulated play, printed"]
fn dist_controls_from_the_edit_layer_end_to_end() {
    sweep(0);
}

#[test]
#[ignore = "a measurement: minutes of emulated play, printed"]
fn mod_controls_from_the_edit_layer_end_to_end() {
    sweep(1);
}

#[test]
#[ignore = "a measurement: minutes of emulated play, printed"]
fn fx_controls_from_the_edit_layer_end_to_end() {
    sweep(2);
}

/// The least change of the spectrum's shape (timbre_distance) a distortion's Amount makes from
/// the bottom of the knob to the top: measured 1.5 (CRUSH) to 4.1 dB (LO-FI), TAPE 2.0 (0.2
/// before the slot lifted its input).
const TIMBRE_MIN: f64 = 1.0;

/// Every distortion's Amount from LIVE's EDIT layer, the bottom of the knob against the top, on
/// one SAW LEAD note at the DAC: the sound changes its shape, not mostly its level, and the note
/// never vanishes. Before the slot lifted its input to the algorithms' level (1.0 and 1.1.0):
/// TAPE's shape 0.2 dB, OVERDRIVE +14.5 dB louder, FUZZ +13.3 dB; CRUSH's top chopped the note.
#[test]
fn every_distortion_amount_from_the_edit_layer_is_heard() {
    let Some(build) = build_dir() else {
        common::skip_message("the distortion Amount test");
        return;
    };
    let fw = Firmware::load(&build);
    let mut weak = Vec::new();
    for alg in 1..6i32 {
        let mut m = Machine::boot(&fw.image);
        common::run_past_boot(&mut m, &fw);
        m.key(EDIT, true);
        m.run_ms(30);
        m.turn(ALGORITHM, alg);
        m.run_ms(200);
        m.turn(K1, -32);
        m.run_ms(400);
        m.key(EDIT, false);
        m.run_ms(200);
        let lo = take(&mut m);
        m.key(EDIT, true);
        m.run_ms(30);
        m.turn(K1, 32);
        m.run_ms(400);
        m.key(EDIT, false);
        m.run_ms(200);
        let hi = take(&mut m);
        let r = measure(&lo, &hi);
        let name = NAMES[0][alg as usize];
        println!(
            "{name}: Amount bottom to top: level {:+.1} dB, timbre {:.1} dB",
            r.level, r.timbre
        );
        if r.timbre < TIMBRE_MIN || r.level.abs() > 6.0 {
            weak.push(format!(
                "{name}: level {:+.1} dB, timbre {:.1} dB",
                r.level, r.timbre
            ));
        }
    }
    assert!(weak.is_empty(), "{weak:#?}");
}
