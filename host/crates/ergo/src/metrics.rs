// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The harness's metric functions: pure arithmetic with no emulator in it, so they are
//! unit-tested (the tests at the end).

/// The screen's width in pixels and in millimetres (the FM-1's lit area, measured by the
/// user: assets/prototypes/screen-readability.html's default).
pub const SCREEN_PX: f64 = 240.0;
pub const SCREEN_MM: f64 = 23.4;

/// Pixel pitch in millimetres.
pub fn mm_per_px(screen_mm: f64) -> f64 {
    screen_mm / SCREEN_PX
}

/// The physical height of a glyph cell drawn at `scale` times the 8-pixel font.
pub fn glyph_mm(scale: u32, screen_mm: f64) -> f64 {
    8.0 * f64::from(scale) * mm_per_px(screen_mm)
}

/// sRGB channel (0..255) to linear light (WCAG 2.x).
fn linear(c: u8) -> f64 {
    let s = f64::from(c) / 255.0;
    if s <= 0.03928 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance.
pub fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// WCAG contrast ratio, 1..21, of two colours in either order.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The interval between two frequencies in cents (1200 to the octave).
pub fn cents(from_hz: f64, to_hz: f64) -> f64 {
    1200.0 * (to_hz / from_hz).log2()
}

/// A power or amplitude ratio in decibels (amplitude: 20 log10).
pub fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

/// The unit a parameter's steps are measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Frequency or cutoff: cents of the Hz value.
    Cents,
    /// Level: dB of a linear gain.
    Db,
    /// Everything else: percent of the parameter's full range.
    Percent,
}

impl Unit {
    pub fn name(self) -> &'static str {
        match self {
            Unit::Cents => "cents",
            Unit::Db => "dB",
            Unit::Percent => "%",
        }
    }
}

/// The step from value `a` to `b` in musical units. For Cents and Db the values are the
/// physical quantity (Hz, linear gain); for Percent they are raw values and `full` is the span
/// that is 100%.
pub fn step_size(unit: Unit, a: f64, b: f64, full: f64) -> f64 {
    match unit {
        Unit::Cents => cents(a.max(1e-9), b.max(1e-9)),
        Unit::Db => db(b.max(1e-12) / a.max(1e-12)),
        Unit::Percent => 100.0 * (b - a) / full,
    }
}

/// Steps in musical units along a series of values.
pub fn steps(unit: Unit, vals: &[f64], full: f64) -> Vec<f64> {
    vals.windows(2)
        .map(|w| step_size(unit, w[0], w[1], full))
        .collect()
}

/// How a series of steps behaves, for the knob rules.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepStats {
    pub n: usize,
    pub median_abs: f64,
    pub max_abs: f64,
    pub min_abs: f64,
    /// Standard deviation over the mean of |step| (0 for a perfectly even curve).
    pub cv: f64,
    /// Steps whose sign is against the sweep's overall direction.
    pub reversals: usize,
    /// The longest run of consecutive steps with |step| below the threshold given.
    pub longest_dead_run: usize,
}

pub fn step_stats(steps: &[f64], dead_below: f64) -> StepStats {
    if steps.is_empty() {
        return StepStats::default();
    }
    let mut abs: Vec<f64> = steps.iter().map(|s| s.abs()).collect();
    let mean = abs.iter().sum::<f64>() / abs.len() as f64;
    let var = abs.iter().map(|a| (a - mean) * (a - mean)).sum::<f64>() / abs.len() as f64;
    let net: f64 = steps.iter().sum();
    let dir = if net >= 0.0 { 1.0 } else { -1.0 };
    let reversals = steps.iter().filter(|s| **s * dir < 0.0).count();
    let (mut run, mut longest) = (0usize, 0usize);
    for a in &abs {
        if *a < dead_below {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    abs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    StepStats {
        n: abs.len(),
        median_abs: abs[abs.len() / 2],
        max_abs: abs[abs.len() - 1],
        min_abs: abs[0],
        cv: if mean > 0.0 { var.sqrt() / mean } else { 0.0 },
        reversals,
        longest_dead_run: longest,
    }
}

/// One action of an input sequence, for the effort count. `hands` is how many hands the
/// control takes while it is held or turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effort {
    Press,
    /// A button held for a time (alone).
    Hold,
    /// An encoder turned by a number of detents.
    Turn(u32),
    /// A button held while an encoder turns, or a second button is pressed (two hands).
    HoldTurn(u32),
    /// A button held while a note key is pressed (GLO + key): two hands.
    HoldKey,
    /// A note key (a play action, not navigation).
    Note,
    /// MIDI in: not the panel (no hand on it).
    Midi,
    /// Waiting: no action.
    Wait,
}

/// The counts of a sequence's actions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffortCount {
    pub presses: u32,
    pub holds: u32,
    /// Distinct turns (each continuous run of one encoder is one action).
    pub turns: u32,
    pub detents: u32,
    pub notes: u32,
    /// The actions that count against the limit: presses, holds and turns, not detents.
    pub actions: u32,
    pub two_handed: u32,
}

pub fn count_effort(seq: &[Effort]) -> EffortCount {
    let mut c = EffortCount::default();
    for e in seq {
        match *e {
            Effort::Press => c.presses += 1,
            Effort::Hold => c.holds += 1,
            Effort::Turn(n) => {
                c.turns += 1;
                c.detents += n;
            }
            Effort::HoldTurn(n) => {
                c.holds += 1;
                c.turns += 1;
                c.detents += n;
                c.two_handed += 1;
            }
            Effort::HoldKey => {
                c.holds += 1;
                c.presses += 1;
                c.two_handed += 1;
            }
            Effort::Note => c.notes += 1,
            Effort::Midi | Effort::Wait => {}
        }
    }
    c.actions = c.presses + c.holds + c.turns;
    c
}

/// RMS of samples as dBFS (16-bit full scale 32768), -120 for silence.
pub fn rms_dbfs(x: &[f32]) -> f64 {
    if x.is_empty() {
        return -120.0;
    }
    let ms = x.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / x.len() as f64;
    (10.0 * (ms / (32768.0 * 32768.0)).max(1e-12).log10()).max(-120.0)
}

/// Spectral centroid in Hz of the first `n` samples (Hann window, mean removed), `fs` Hz.
pub fn spectral_centroid(x: &[f32], fs: f64) -> f64 {
    let n = x.len().min(2048);
    if n < 64 {
        return 0.0;
    }
    let mean = x[..n].iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let v: Vec<f64> = (0..n)
        .map(|i| {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos();
            (f64::from(x[i]) - mean) * w
        })
        .collect();
    let (mut num, mut den) = (0.0, 0.0);
    // A recurrence-free DFT over every second bin: a centroid does not need them all.
    for k in (1..n / 2).step_by(2) {
        let w = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in v.iter().enumerate() {
            let a = w * i as f64;
            re += s * a.cos();
            im -= s * a.sin();
        }
        let m = (re * re + im * im).sqrt();
        num += m * k as f64 * fs / n as f64;
        den += m;
    }
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

/// The magnitude spectrum of the first 2048 samples (Hann window, mean removed) in dB, bins 1..=512
/// (21.5 Hz to 11 kHz at 44.1 kHz): what a timbre looks like, for comparing two sounds.
pub fn log_spectrum(x: &[f32]) -> Vec<f32> {
    let n = x.len().min(2048);
    if n < 256 {
        return Vec::new();
    }
    let mean = x[..n].iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let v: Vec<f64> = (0..n)
        .map(|i| {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos();
            (f64::from(x[i]) - mean) * w
        })
        .collect();
    let bins = (n / 4).min(512);
    (1..=bins)
        .map(|k| {
            let w = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &s) in v.iter().enumerate() {
                let a = w * i as f64;
                re += s * a.cos();
                im -= s * a.sin();
            }
            (20.0 * ((re * re + im * im).sqrt() / n as f64 + 1e-3).log10()) as f32
        })
        .collect()
}

/// The RMS difference in dB between two log spectra, over the bins where either is within 60 dB
/// of its own peak (the rest is noise floor). 0 for empty or unequal inputs.
pub fn spectral_distance_db(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let pa = a.iter().cloned().fold(f32::MIN, f32::max) - 60.0;
    let pb = b.iter().cloned().fold(f32::MIN, f32::max) - 60.0;
    let (mut sum, mut n) = (0.0, 0usize);
    for (x, y) in a.iter().zip(b) {
        if *x > pa || *y > pb {
            sum += f64::from(x - y) * f64::from(x - y);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (sum / n as f64).sqrt()
    }
}

/// Mean (DC offset) of the samples as a fraction of full scale.
pub fn dc_fraction(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    x.iter().map(|&s| f64::from(s)).sum::<f64>() / x.len() as f64 / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_of_black_and_white_is_21() {
        assert!((contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([255, 255, 255], [0, 0, 0]) - 21.0).abs() < 1e-9);
    }

    #[test]
    fn contrast_of_equal_colours_is_1() {
        assert!((contrast_ratio([0x6c, 0x6c, 0x6c], [0x6c, 0x6c, 0x6c]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn contrast_matches_a_wcag_reference() {
        // #777 on white is the classic 4.48:1 (just under AA).
        let r = contrast_ratio([0x77, 0x77, 0x77], [255, 255, 255]);
        assert!((r - 4.48).abs() < 0.01, "{r}");
    }

    #[test]
    fn a_glyph_at_two_times_is_about_1_56_mm() {
        assert!((glyph_mm(1, SCREEN_MM) - 0.78).abs() < 0.001);
        assert!((glyph_mm(2, SCREEN_MM) - 1.56).abs() < 0.001);
        assert!((glyph_mm(3, SCREEN_MM) - 2.34).abs() < 0.001);
    }

    #[test]
    fn an_octave_is_1200_cents_and_a_halving_is_minus_6_db() {
        assert!((cents(440.0, 880.0) - 1200.0).abs() < 1e-9);
        assert!((step_size(Unit::Cents, 100.0, 50.0, 0.0) + 1200.0).abs() < 1e-9);
        assert!((step_size(Unit::Db, 1.0, 0.5, 0.0) + 6.0206).abs() < 1e-3);
        assert!((step_size(Unit::Percent, 10.0, 26.0, 160.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn an_even_log_sweep_has_no_spread_and_a_kink_shows() {
        let even: Vec<f64> = (0..20)
            .map(|i| 100.0 * 2f64.powf(f64::from(i) / 12.0))
            .collect();
        let s = step_stats(&steps(Unit::Cents, &even, 0.0), 10.0);
        assert!((s.median_abs - 100.0).abs() < 1e-6 && s.cv < 1e-9 && s.reversals == 0);
        let mut kink = even.clone();
        kink[10] = kink[9];
        let s = step_stats(&steps(Unit::Cents, &kink, 0.0), 10.0);
        assert_eq!(s.longest_dead_run, 1);
        assert!(s.cv > 0.1);
        kink[10] = kink[8];
        let s = step_stats(&steps(Unit::Cents, &kink, 0.0), 10.0);
        assert_eq!(s.reversals, 1);
    }

    #[test]
    fn effort_counts_actions_not_detents() {
        use Effort::*;
        let c = count_effort(&[Press, Turn(5), Wait, HoldTurn(2), Note, Midi]);
        assert_eq!(c.presses, 1);
        assert_eq!(c.holds, 1);
        assert_eq!(c.turns, 2);
        assert_eq!(c.detents, 7);
        assert_eq!(c.actions, 4);
        assert_eq!(c.two_handed, 1);
        assert_eq!(c.notes, 1);
    }

    #[test]
    fn spectral_distance_is_zero_for_equal_sounds_and_grows_with_a_changed_harmonic() {
        let fs = 44100.0;
        let tone = |amp3: f64| -> Vec<f32> {
            (0..2048)
                .map(|i| {
                    let t = f64::from(i) / fs;
                    (8000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                        + amp3 * (2.0 * std::f64::consts::PI * 1320.0 * t).sin())
                        as f32
                })
                .collect()
        };
        let (a, b, c) = (
            log_spectrum(&tone(0.0)),
            log_spectrum(&tone(0.0)),
            log_spectrum(&tone(8000.0)),
        );
        assert_eq!(a.len(), 512);
        assert!(spectral_distance_db(&a, &b) < 1e-9);
        assert!(
            spectral_distance_db(&a, &c) > 3.0,
            "{}",
            spectral_distance_db(&a, &c)
        );
        assert_eq!(spectral_distance_db(&[], &[]), 0.0);
    }

    #[test]
    fn a_sine_has_its_own_frequency_as_centroid_and_known_rms() {
        let fs = 44100.0;
        let x: Vec<f32> = (0..2048)
            .map(|i| {
                10000.0 * (2.0 * std::f64::consts::PI * 1000.0 * f64::from(i) / fs).sin() as f32
            })
            .collect();
        let c = spectral_centroid(&x, fs);
        assert!((c - 1000.0).abs() < 100.0, "{c}");
        let r = rms_dbfs(&x);
        assert!(
            (r - (20.0 * (10000.0 / 2f64.sqrt() / 32768.0).log10())).abs() < 0.1,
            "{r}"
        );
        assert!(dc_fraction(&x).abs() < 0.01);
    }
}
