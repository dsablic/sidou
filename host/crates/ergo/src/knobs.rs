// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Knob and encoder feel: each control swept over its whole range a detent at a time, the
//! parameter read from the firmware's live instrument, the step in musical units, and the
//! sound of a held note rendered between steps (spectral centroid and level).

use std::path::Path;

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::{key_of_note, panel, Dev, Params};
use crate::findings::{metric, Chart, ClipRef, Finding, Metric, Series, Sev};
use crate::journeys::{run_script, write_clip, FS};
use crate::metrics::{
    cents, db, log_spectrum, rms_dbfs, spectral_centroid, spectral_distance_db, step_stats, steps,
    Unit,
};

/// Which firmware parameter a control moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P {
    Cutoff,
    Resonance,
    PulseWidth,
    Master,
    Preset,
    Profile,
    /// K3 on a saw or noise sound, and K4: the envelope's decay and release (0..15).
    Decay,
    Release,
    /// K3 on a sync sound: the ratio's semitones from the preset's, -12..+12.
    SyncSemis,
    Character(usize),
    /// LAB's cards the rows above do not cover: an index into `Params::lab` (attack, sustain,
    /// the PWM LFO's depth and rate, the fine tune), and the range its steps are a share of.
    Lab(usize, u16),
    /// slot (0 DIST, 1 MOD, 2 FX), field (1 amount, 2 A, 3 B, 4 mix).
    Slot(usize, usize),
}

impl P {
    pub fn raw(self, p: &Params) -> f64 {
        match self {
            P::Cutoff => f64::from(p.cutoff),
            P::Resonance => f64::from(p.resonance),
            P::PulseWidth => f64::from(p.pulse_width),
            P::Master => f64::from(p.master),
            P::Preset => f64::from(p.preset),
            P::Profile => f64::from(p.profile),
            P::Decay => f64::from(p.decay),
            P::Release => f64::from(p.release),
            // shifted up so that the sweep's raw values are positive
            P::SyncSemis => f64::from(i32::from(p.sync_semis) + 12),
            P::Character(i) => f64::from(p.character[i]),
            // the fine tune is signed (cents): shifted up so the sweep's raw values are positive
            P::Lab(i, _) => f64::from(p.lab[i] + if i == 7 { 50 } else { 0 }),
            P::Slot(s, f) => f64::from(p.slot[s][f]),
        }
    }

    pub fn name(self) -> String {
        match self {
            P::Cutoff => "cutoff".into(),
            P::Resonance => "resonance".into(),
            P::PulseWidth => "pulse width".into(),
            P::Master => "master volume".into(),
            P::Preset => "preset".into(),
            P::Profile => "chip profile".into(),
            P::Decay => "decay".into(),
            P::Release => "release".into(),
            P::SyncSemis => "sync ratio".into(),
            P::Character(i) => ["age", "DAC sloppiness", "thump", "filter bleed"][i].into(),
            P::Lab(i, _) => [
                "attack",
                "sustain",
                "",
                "",
                "",
                "PWM depth",
                "PWM rate",
                "fine tune",
            ]
            .get(i)
            .copied()
            .unwrap_or("LAB card")
            .into(),
            P::Slot(s, f) => format!(
                "{} {}",
                ["DIST", "MOD", "FX"][s],
                ["algorithm", "amount", "A", "B", "mix"][f]
            ),
        }
    }

    /// The physical quantity steps are measured on, and its unit.
    pub fn unit(self) -> Unit {
        match self {
            P::Cutoff => Unit::Cents,
            P::Master => Unit::Db,
            _ => Unit::Percent,
        }
    }

    pub fn full(self) -> f64 {
        match self {
            P::Cutoff => 2047.0,
            P::Resonance => 15.0,
            P::PulseWidth => 4095.0,
            P::Master => 1023.0,
            P::Preset => 3.0,
            P::Profile => 12.0,
            P::Decay | P::Release => 15.0,
            P::SyncSemis => 24.0,
            P::Character(_) | P::Slot(..) => 127.0,
            P::Lab(_, full) => f64::from(full),
        }
    }

    pub fn phys(self, raw: f64, model: u8) -> f64 {
        match self {
            // The cutoff in Hz as the firmware's own curve gives it (curve 0 of the model).
            P::Cutoff => {
                f64::from(unsafe { sidou_sys::sid_cutoff_hz(u32::from(model), 0, raw as u32) })
            }
            P::Master => {
                f64::from(unsafe { sidou_sys::master_gain_of(raw as u32, 1023) }) / 32768.0
            }
            _ => raw,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub id: String,
    pub control: String,
    pub view: String,
    pub param: P,
    pub setup: String,
    pub hold: Option<u8>,
    /// The encoder, or None for the MASTER pot.
    pub enc: Option<u8>,
    /// Hold a note and render between steps.
    pub audio: bool,
    /// A choice (preset, profile): count its detents, no curve rules.
    pub discrete: bool,
    /// The control acts at a note's start (the thump, the decay): each detent's window is a new
    /// note from its first sample, not the held one.
    pub retrigger: bool,
}

const CHIP_SETUP: &str = "press HOME; turn ALGORITHM 1; wait 3000";

pub fn specs() -> Vec<Spec> {
    let mut v = Vec::new();
    let mut add = |id: &str,
                   control: &str,
                   view: &str,
                   param: P,
                   setup: &str,
                   hold: Option<u8>,
                   enc: Option<u8>,
                   audio: bool,
                   discrete: bool| {
        v.push(Spec {
            id: id.into(),
            control: control.into(),
            view: view.into(),
            param,
            setup: setup.into(),
            hold,
            enc,
            audio,
            discrete,
            retrigger: false,
        });
    };
    add(
        "K1.live",
        "K1",
        "LIVE page",
        P::Cutoff,
        "",
        None,
        Some(panel::K1),
        true,
        false,
    );
    add(
        "K2.live",
        "K2",
        "LIVE page",
        P::Resonance,
        "",
        None,
        Some(panel::K2),
        true,
        false,
    );
    // K3 is the sound's own control: the decay on SAW LEAD (the first preset), the pulse width on
    // PWM BASS, the sync ratio on SYNC LEAD, the decay on NOISE PERC.
    add(
        "K3.live",
        "K3",
        "LIVE page",
        P::Decay,
        "",
        None,
        Some(panel::K3),
        true,
        false,
    );
    add(
        "K3.pwm",
        "K3",
        "LIVE page (PWM BASS)",
        P::PulseWidth,
        "turn PRESETS 1",
        None,
        Some(panel::K3),
        true,
        false,
    );
    add(
        "K3.sync",
        "K3",
        "LIVE page (SYNC LEAD)",
        P::SyncSemis,
        "turn PRESETS 2",
        None,
        Some(panel::K3),
        true,
        false,
    );
    add(
        "K3.perc",
        "K3",
        "LIVE page (NOISE PERC)",
        P::Decay,
        "turn PRESETS 3",
        None,
        Some(panel::K3),
        true,
        false,
    );
    // K4 is the release: heard after the note, so the sweep renders no held note.
    add(
        "K4.live",
        "K4",
        "LIVE page",
        P::Release,
        "",
        None,
        Some(panel::K4),
        false,
        false,
    );
    add(
        "PRESETS.live",
        "PRESETS",
        "every view",
        P::Preset,
        "",
        None,
        Some(panel::PRESETS),
        false,
        true,
    );
    add(
        "MASTER",
        "MASTER",
        "every view",
        P::Master,
        "",
        None,
        None,
        true,
        false,
    );
    // EDIT held: the DIST slot at OVERDRIVE (ALGORITHM twice on its page: TAPE, OVERDRIVE).
    let od = "turn SELECT 1; turn ALGORITHM 2; turn SELECT -1; holdturn EDIT K4 32";
    add(
        "K1.edit",
        "K1",
        "EDIT held (DIST)",
        P::Slot(0, 1),
        od,
        Some(panel::EDIT),
        Some(panel::K1),
        true,
        false,
    );
    add(
        "K2.edit",
        "K2",
        "EDIT held (DIST)",
        P::Slot(0, 2),
        od,
        Some(panel::EDIT),
        Some(panel::K2),
        true,
        false,
    );
    add(
        "K3.edit",
        "K3",
        "EDIT held (DIST)",
        P::Slot(0, 3),
        od,
        Some(panel::EDIT),
        Some(panel::K3),
        true,
        false,
    );
    add(
        "K4.edit",
        "K4",
        "EDIT held (DIST)",
        P::Slot(0, 4),
        od,
        Some(panel::EDIT),
        Some(panel::K4),
        true,
        false,
    );
    for (i, k) in [panel::K1, panel::K2, panel::K3, panel::K4]
        .into_iter()
        .enumerate()
    {
        add(
            &format!("K{}.chip", i + 1),
            &format!("K{}", i + 1),
            "CHIP (chip section)",
            P::Character(i),
            CHIP_SETUP,
            None,
            Some(k),
            true,
            false,
        );
    }
    add(
        "ALGORITHM.chip",
        "ALGORITHM",
        "CHIP (profile)",
        P::Profile,
        CHIP_SETUP,
        None,
        Some(panel::ALGORITHM),
        false,
        true,
    );
    // LAB's cards (docs/reference/controls.md "LAB"): the OSC page on PWM BASS (the preset with a
    // pulse and its PWM LFO), then the envelope's four and the filter's cutoff and resonance.
    // The vibrato's cards and the filter's voices are not swept: a wobble and a routing mask are
    // not what the centroid and level of a held note show.
    // ENV opens LAB at ENVELOPE; SELECT steps to OSC (-1) or FILTER (+1).
    let osc = "turn PRESETS 1; press ENV; wait 600; turn SELECT -1; wait 300".to_string();
    let env = "press ENV; wait 600".to_string();
    let filt = "press ENV; wait 600; turn SELECT 1; wait 300".to_string();
    let mut lab_add = |id: &str, view: &str, param: P, setup: &str, k: u8, audio: bool| {
        v.push(Spec {
            id: id.into(),
            control: format!("K{}", k - panel::K1 + 1),
            view: view.into(),
            param,
            setup: setup.into(),
            hold: None,
            enc: Some(k),
            audio,
            discrete: false,
            retrigger: false,
        });
    };
    lab_add(
        "K1.lab-osc",
        "LAB OSC (PWM BASS)",
        P::PulseWidth,
        &osc,
        panel::K1,
        true,
    );
    lab_add(
        "K2.lab-osc",
        "LAB OSC (PWM BASS)",
        P::Lab(5, 2048),
        &osc,
        panel::K2,
        true,
    );
    lab_add(
        "K3.lab-osc",
        "LAB OSC (PWM BASS)",
        P::Lab(6, 255),
        &osc,
        panel::K3,
        true,
    );
    lab_add(
        "K4.lab-osc",
        "LAB OSC (PWM BASS)",
        P::Lab(7, 100),
        &osc,
        panel::K4,
        true,
    );
    lab_add(
        "K1.lab-env",
        "LAB ENVELOPE",
        P::Lab(0, 15),
        &env,
        panel::K1,
        true,
    );
    lab_add(
        "K2.lab-env",
        "LAB ENVELOPE",
        P::Decay,
        &env,
        panel::K2,
        true,
    );
    lab_add(
        "K3.lab-env",
        "LAB ENVELOPE",
        P::Lab(1, 15),
        &env,
        panel::K3,
        true,
    );
    lab_add(
        "K4.lab-env",
        "LAB ENVELOPE",
        P::Release,
        &env,
        panel::K4,
        false,
    );
    lab_add(
        "K1.lab-filter",
        "LAB FILTER",
        P::Cutoff,
        &filt,
        panel::K1,
        true,
    );
    lab_add(
        "K2.lab-filter",
        "LAB FILTER",
        P::Resonance,
        &filt,
        panel::K2,
        true,
    );
    // The decay on SAW LEAD, the thump and LAB's attack, decay and sustain are heard where a
    // note starts.
    for s in v.iter_mut() {
        s.retrigger = [
            "K3.live",
            "K3.chip",
            "K1.lab-env",
            "K2.lab-env",
            "K3.lab-env",
        ]
        .contains(&s.id.as_str());
    }
    v
}

#[derive(Clone, Debug)]
pub struct Sweep {
    pub skipped: bool,
    pub spec: Spec,
    /// Raw value at each detent, from the bottom.
    pub raw: Vec<f64>,
    pub phys: Vec<f64>,
    pub centroid: Vec<f64>,
    pub rms: Vec<f64>,
    pub pieces: Vec<Vec<f32>>,
    /// The log spectrum of each detent's window (empty without audio).
    pub spectra: Vec<Vec<f32>>,
    /// Detents from the bottom to the top of the range (None: a pot).
    pub detents: Option<usize>,
    /// The value before the sweep moved it (the default, after the setup).
    pub initial: f64,
}

const WINDOW_MS: u64 = 47;

fn control_used(param: P, algorithm: u8) -> bool {
    if let P::Slot(slot, field) = param {
        let mut label = [0; 16];
        unsafe {
            sidou_sys::chip_view_label(
                slot as u32,
                u32::from(algorithm),
                field as u32,
                label.as_mut_ptr(),
            ) != 0
        }
    } else {
        true
    }
}

/// The note a sweep holds (A3).
const NOTE: u8 = 57;
/// A retriggered window's release before the new note: SAW LEAD's release (6) has ended.
const RETRIGGER_GAP_MS: u64 = 600;

fn window(d: &mut Dev, audio: bool, retrigger: bool) -> (f64, f64, Vec<f32>) {
    if !audio {
        return (0.0, -120.0, Vec::new());
    }
    if retrigger {
        d.key_up(key_of_note(NOTE));
        d.run(RETRIGGER_GAP_MS);
        let mark = d.audio_mark();
        d.key_down(key_of_note(NOTE));
        d.run(WINDOW_MS);
        let a = d.audio_since(mark);
        return (spectral_centroid(&a, FS), rms_dbfs(&a), a);
    }
    d.run(40);
    let mark = d.audio_mark();
    d.run(WINDOW_MS);
    let a = d.audio_since(mark);
    (spectral_centroid(&a, FS), rms_dbfs(&a), a)
}

pub fn run_sweep(build: &Path, spec: &Spec) -> Sweep {
    let mut d = Dev::boot(build);
    run_script(&mut d, &spec.setup);
    d.settle();
    if spec.audio {
        d.key_down(key_of_note(NOTE));
        d.run(300);
    }
    if let Some(h) = spec.hold {
        d.key_down(h);
        d.run(30);
    }
    let mut sw = Sweep {
        skipped: false,
        spec: spec.clone(),
        raw: Vec::new(),
        phys: Vec::new(),
        centroid: Vec::new(),
        rms: Vec::new(),
        pieces: Vec::new(),
        spectra: Vec::new(),
        detents: None,
        initial: 0.0,
    };
    if let P::Slot(slot, _) = spec.param {
        if !control_used(spec.param, d.params().slot[slot][0]) {
            sw.skipped = true;
            return sw;
        }
    }
    let read = |d: &mut Dev| {
        let p = d.params();
        (spec.param.raw(&p), p.model)
    };
    let record = |d: &mut Dev, sw: &mut Sweep| {
        let (raw, model) = read(d);
        let (c, r, a) = window(d, spec.audio, spec.retrigger);
        sw.raw.push(raw);
        sw.phys.push(spec.param.phys(raw, model));
        sw.centroid.push(c);
        sw.rms.push(r);
        sw.spectra.push(log_spectrum(&a));
        sw.pieces.push(a);
    };
    sw.initial = read(&mut d).0;
    match spec.enc {
        None => {
            for v in (0..=1023u16).step_by(16) {
                d.m.pot(v);
                d.run(40);
                record(&mut d, &mut sw);
            }
        }
        Some(enc) => {
            // To the bottom: eight detents at a time until the value stops moving.
            let mut stable = 0;
            let mut last = read(&mut d).0;
            for _ in 0..80 {
                d.turn(enc, -8);
                d.run(40);
                let now = read(&mut d).0;
                stable = if now == last { stable + 1 } else { 0 };
                last = now;
                if stable >= 2 {
                    break;
                }
            }
            // Up, a detent at a time, until it stops moving after it has moved.
            let mut same = 0;
            record(&mut d, &mut sw);
            for _ in 0..600 {
                d.turn(enc, 1);
                record(&mut d, &mut sw);
                let n = sw.raw.len();
                same = if sw.raw[n - 1] == sw.raw[n - 2] {
                    same + 1
                } else {
                    0
                };
                if same >= 3 && sw.raw[n - 1] != sw.raw[0] {
                    break;
                }
            }
            // Detents to the top: up to the first one that reached the final value.
            let top = *sw.raw.last().unwrap();
            let first_top = sw.raw.iter().position(|v| *v == top).unwrap();
            sw.detents = Some(first_top);
            sw.raw.truncate(first_top + 1);
            sw.phys.truncate(first_top + 1);
            sw.centroid.truncate(first_top + 1);
            sw.rms.truncate(first_top + 1);
            sw.pieces.truncate(first_top + 1);
            sw.spectra.truncate(first_top + 1);
        }
    }
    sw
}

/// The numbers of one sweep, for the report and the baseline.
#[derive(Clone, Debug, Default)]
pub struct SweepStats {
    pub detents: Option<usize>,
    pub span: f64,
    pub unit: &'static str,
    pub median_step: f64,
    pub max_step: f64,
    pub max_step_at: usize,
    pub cv: f64,
    pub param_reversals: usize,
    pub audible_cents: f64,
    pub audible_db: f64,
    /// The largest timbre difference between the first detent and any other (dB).
    pub audible_spectral_db: f64,
    pub median_audio_pct: f64,
    pub dead_zone: usize,
    pub dead_zone_at: usize,
    pub dead_zone_share: f64,
    /// Where the value stood before the sweep, as a share of the travel (by detent).
    pub default_share: f64,
}

pub fn stats(s: &Sweep, cfg: &Config) -> SweepStats {
    let unit = s.spec.param.unit();
    let full = s.spec.param.full();
    // The MASTER pot's first value is silence: leave it out of the dB steps.
    let skip = usize::from(unit == Unit::Db && s.phys.first() == Some(&0.0));
    let vals = &s.phys[skip..];
    let indexed: Vec<_> = s
        .phys
        .windows(2)
        .enumerate()
        .filter(|(_, w)| s.detents.is_some() || unit != Unit::Db || (w[0] >= 0.01 && w[1] >= 0.01))
        .filter(|(i, _)| *i >= skip)
        .map(|(i, w)| (i, steps(unit, w, full)[0]))
        .collect();
    let st: Vec<_> = indexed.iter().map(|(_, step)| *step).collect();
    let ss = step_stats(&st, 0.0);
    let max_at = st
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
        .map_or(0, |(i, _)| indexed[i].0);
    let span = match unit {
        Unit::Cents => cents(
            vals.iter().cloned().fold(f64::MAX, f64::min).max(1e-9),
            vals.iter().cloned().fold(0.0, f64::max),
        ),
        Unit::Db => db(vals.iter().cloned().fold(0.0, f64::max)
            / vals.iter().cloned().fold(f64::MAX, f64::min).max(1e-9)),
        Unit::Percent => {
            100.0
                * (vals.iter().cloned().fold(0.0, f64::max)
                    - vals.iter().cloned().fold(f64::MAX, f64::min))
                / full
        }
    };
    let default_share = if s.raw.len() > 1 {
        let at = s
            .raw
            .iter()
            .enumerate()
            .min_by(|a, b| {
                (a.1 - s.initial)
                    .abs()
                    .partial_cmp(&(b.1 - s.initial).abs())
                    .unwrap()
            })
            .map_or(0, |(i, _)| i);
        at as f64 / (s.raw.len() - 1) as f64
    } else {
        0.0
    };
    let mut out = SweepStats {
        default_share,
        detents: s.detents,
        span,
        unit: unit.name(),
        median_step: ss.median_abs,
        max_step: ss.max_abs,
        max_step_at: max_at,
        cv: ss.cv,
        param_reversals: ss.reversals,
        ..SweepStats::default()
    };
    if s.spec.audio && s.centroid.len() > 2 {
        let c: Vec<f64> = s.centroid.iter().cloned().filter(|c| *c > 0.0).collect();
        if !c.is_empty() {
            let (lo, hi) = (
                c.iter().cloned().fold(f64::MAX, f64::min),
                c.iter().cloned().fold(0.0, f64::max),
            );
            out.audible_cents = cents(lo, hi);
        }
        let r: Vec<f64> = s.rms.iter().cloned().filter(|r| *r > -100.0).collect();
        if !r.is_empty() {
            out.audible_db = r.iter().cloned().fold(f64::MIN, f64::max)
                - r.iter().cloned().fold(f64::MAX, f64::min);
        }
        if let Some(first) = s.spectra.first() {
            out.audible_spectral_db = s
                .spectra
                .iter()
                .map(|x| spectral_distance_db(first, x))
                .fold(0.0, f64::max);
        }
        let pct: Vec<f64> = s
            .centroid
            .windows(2)
            .filter(|w| w[0] > 0.0 && w[1] > 0.0)
            .map(|w| 100.0 * (w[1] - w[0]).abs() / w[0])
            .collect();
        let mut sorted = pct.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.median_audio_pct = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        // Dead zone: a window of N detents over which neither brightness nor level moved by
        // its JND, though the parameter moved.
        let n = cfg.dead_zone_detents;
        let mut marks = vec![false; s.centroid.len()];
        #[allow(clippy::needless_range_loop)]
        for j in 0..s.centroid.len().saturating_sub(n) {
            let (c0, c1) = (s.centroid[j], s.centroid[j + n]);
            let dc = if c0 > 0.0 {
                100.0 * (c1 - c0).abs() / c0
            } else {
                0.0
            };
            let dr = (s.rms[j + n] - s.rms[j]).abs();
            let ds = match (s.spectra.get(j), s.spectra.get(j + n)) {
                (Some(a), Some(b)) => spectral_distance_db(a, b),
                _ => 0.0,
            };
            let moved = s.raw[j] != s.raw[j + n];
            if moved
                && dc < cfg.dead_zone_centroid_pct
                && dr < cfg.jnd_level_db
                && ds < cfg.jnd_spectral_db
            {
                marks[j] = true;
            }
        }
        let (mut best, mut best_at, mut run, mut run_at) = (0, 0, 0, 0);
        for (i, m) in marks.iter().enumerate() {
            if *m {
                if run == 0 {
                    run_at = i;
                }
                run += 1;
                if run > best {
                    best = run;
                    best_at = run_at;
                }
            } else {
                run = 0;
            }
        }
        if best > 0 {
            out.dead_zone = best + n - 1;
            out.dead_zone_at = best_at;
            out.dead_zone_share =
                out.dead_zone as f64 / s.raw.len().saturating_sub(1).max(1) as f64;
        }
    }
    out
}

fn norm(v: &[f64]) -> Vec<f64> {
    let lo = v.iter().cloned().fold(f64::MAX, f64::min);
    let hi = v.iter().cloned().fold(f64::MIN, f64::max);
    v.iter()
        .map(|x| {
            if hi > lo {
                100.0 * (x - lo) / (hi - lo)
            } else {
                50.0
            }
        })
        .collect()
}

fn chart(s: &Sweep) -> Chart {
    let x: Vec<f64> = (0..s.raw.len()).map(|i| i as f64).collect();
    let unit = s.spec.param.unit();
    let ylab = match unit {
        Unit::Cents => "cutoff, octaves from the lowest (log Hz)",
        Unit::Db => "level, dB from the lowest",
        Unit::Percent => "parameter, % of its travel",
    };
    let pv: Vec<f64> = if unit == Unit::Cents {
        let lo = s.phys.iter().cloned().fold(f64::MAX, f64::min).max(1e-9);
        s.phys.iter().map(|h| (h.max(1e-9) / lo).log2()).collect()
    } else {
        s.raw.clone()
    };
    let mut series = vec![Series {
        name: "parameter (normalised)".into(),
        points: x.iter().cloned().zip(norm(&pv)).collect(),
    }];
    if s.spec.audio {
        series.push(Series {
            name: "brightness: spectral centroid (normalised)".into(),
            points: x.iter().cloned().zip(norm(&s.centroid)).collect(),
        });
        series.push(Series {
            name: "level: RMS (normalised)".into(),
            points: x.iter().cloned().zip(norm(&s.rms)).collect(),
        });
    }
    Chart {
        title: format!(
            "{} {} ({})",
            s.spec.control,
            s.spec.param.name(),
            s.spec.view
        ),
        x_label: if s.detents.is_some() {
            "detent".into()
        } else {
            "pot position (steps of 16)".into()
        },
        y_label: format!("{ylab}; each curve scaled 0-100"),
        series,
    }
}

/// A clip of the sweep: about a dozen detents spread over it, 70 ms each.
fn clip(s: &Sweep, out: &Path, name: &str) -> Option<ClipRef> {
    if !s.spec.audio || s.pieces.iter().all(Vec::is_empty) {
        return None;
    }
    let n = s.pieces.len();
    let picks = 12.min(n);
    let mut x: Vec<f32> = Vec::new();
    for k in 0..picks {
        let i = k * (n - 1) / (picks - 1).max(1);
        x.extend_from_slice(&s.pieces[i]);
    }
    let path = format!("knobs/{name}.wav");
    write_clip(&out.join(&path), &x);
    Some(ClipRef {
        label: "the sweep, 12 settings from lowest to highest, a note held".into(),
        path,
    })
}

pub struct KnobResult {
    pub findings: Vec<Finding>,
    pub metrics: Vec<Metric>,
    pub table: Vec<Value>,
}

/// Judges a sweep: pure over the measured series (unit-tested with synthetic ones).
pub fn judge(s: &Sweep, cfg: &Config, out: &Path) -> (Vec<Finding>, Vec<Metric>, Value) {
    if s.skipped {
        return (
            Vec::new(),
            Vec::new(),
            json!({"id": s.spec.id, "skipped": true,
            "reason": "algorithm does not use this control"}),
        );
    }
    let st = stats(s, cfg);
    let id = &s.spec.id;
    let label = format!(
        "{} {} ({})",
        s.spec.control,
        s.spec.param.name(),
        s.spec.view
    );
    let unit = s.spec.param.unit();
    let mut fs: Vec<Finding> = Vec::new();
    let nums = json!({
        "detents": st.detents, "span": st.span, "unit": st.unit, "median_step": st.median_step,
        "max_step": st.max_step, "max_step_at_detent": st.max_step_at, "step_spread_cv": st.cv,
        "param_reversals": st.param_reversals, "audible_span_cents": st.audible_cents,
        "audible_span_db": st.audible_db, "audible_spectral_db": st.audible_spectral_db, "median_audio_step_pct": st.median_audio_pct,
        "dead_zone_detents": st.dead_zone, "dead_zone_starts_at": st.dead_zone_at,
        "default_at_share_of_travel": st.default_share,
    });
    let word = if s.detents.is_some() {
        "detents"
    } else {
        "steps of the pot"
    };
    let mk = |rule: &str, sev: Sev, title: String, rule_text: String, detail: String| {
        let mut f = Finding::new(format!("KNOB.{rule}.{id}"), sev, "knobs", title, rule_text)
            .detail(detail)
            .numbers(nums.clone())
            .chart(chart(s));
        if let Some(c) = clip(s, out, &format!("{id}-{rule}")) {
            f = f.clip(c);
        }
        f
    };
    let step_unit = unit.name();
    if !s.spec.discrete {
        let (jnd, max_step) = match unit {
            Unit::Cents => (cfg.jnd_cutoff_cents, cfg.max_step_cutoff_cents),
            Unit::Db => (cfg.jnd_level_db, cfg.max_step_db),
            Unit::Percent => (cfg.jnd_percent, cfg.max_step_percent),
        };
        if unit == Unit::Cents && st.span < cfg.min_cutoff_octaves * 1200.0 {
            fs.push(mk(
                "range",
                Sev::High,
                format!(
                    "{label}: the whole travel covers only {:.1} octaves",
                    st.span / 1200.0
                ),
                format!(
                    "A cutoff sweep covers at least {} octaves.",
                    cfg.min_cutoff_octaves
                ),
                format!(
                    "{:.0} cents from the lowest to the highest detent.",
                    st.span
                ),
            ));
        }
        if s.spec.audio
            && st.audible_cents < cfg.min_audible_cents
            && st.audible_db < cfg.min_audible_db
            && st.audible_spectral_db < cfg.min_audible_spectral_db
        {
            fs.push(mk(
                "audible",
                Sev::High,
                format!("{label}: sweeping the whole travel barely changes the sound"),
                format!(
                    "Over its travel a control changes the brightness by at least {} cents, the level by {} dB or the spectrum by {} dB.",
                    cfg.min_audible_cents, cfg.min_audible_db, cfg.min_audible_spectral_db
                ),
                format!("Brightness moves {:.0} cents, level {:.1} dB and the spectrum {:.1} dB over {} detents.", st.audible_cents, st.audible_db, st.audible_spectral_db, s.raw.len() - 1),
            ));
        }
        if st.dead_zone > 0 && st.dead_zone > cfg.dead_zone_detents && st.dead_zone_share >= 0.1 {
            fs.push(mk(
                "dead",
                if st.dead_zone_share >= 0.3 { Sev::High } else { Sev::Medium },
                format!("{label}: a dead zone of {} {word} ({:.0}% of the travel)", st.dead_zone, 100.0 * st.dead_zone_share),
                format!(
                    "No run of {} detents leaves both brightness (under {}%) and level (under {} dB) unchanged.",
                    cfg.dead_zone_detents, cfg.dead_zone_centroid_pct, cfg.jnd_level_db
                ),
                format!("From {word} {} to {} the parameter moves and the sound does not.", st.dead_zone_at, st.dead_zone_at + st.dead_zone),
            ));
        }
        if s.detents.is_some()
            && st.median_step < jnd
            && (!s.spec.audio || st.median_audio_pct < cfg.jnd_centroid_pct)
        {
            fs.push(mk(
                "fine",
                Sev::Low,
                format!("{label}: a detent moves it {:.2} {step_unit}, under the {jnd} {step_unit} just-noticeable step", st.median_step),
                format!("A detent changes the value by at least {jnd} {step_unit} (the JND) or its sound by {}%.", cfg.jnd_centroid_pct),
                format!("Median step {:.3} {step_unit}; median brightness step {:.2}%.", st.median_step, st.median_audio_pct),
            ));
        }
        if st.max_step > max_step {
            fs.push(mk(
                "jump",
                Sev::Medium,
                format!(
                    "{label}: one detent jumps {:.1} {step_unit} (at detent {})",
                    st.max_step, st.max_step_at
                ),
                format!("No detent moves it more than {max_step} {step_unit}."),
                format!("The median step is {:.2} {step_unit}.", st.median_step),
            ));
        }
        if st.param_reversals > 0 {
            fs.push(mk(
                "mono",
                Sev::High,
                format!(
                    "{label}: not monotonic, {} detents go the wrong way",
                    st.param_reversals
                ),
                "Turning one way moves the value one way, every detent.".into(),
                String::new(),
            ));
        }
        if st.cv > cfg.max_step_cv && st.median_step > 0.0 {
            fs.push(mk(
                "uneven",
                Sev::Low,
                format!("{label}: uneven steps (spread {:.2})", st.cv),
                format!(
                    "Step sizes vary by less than {} (std over mean).",
                    cfg.max_step_cv
                ),
                format!(
                    "Steps run from {:.2} to {:.2} {step_unit}.",
                    st.median_step.min(st.max_step),
                    st.max_step
                ),
            ));
        }
    }
    if let Some(n) = st.detents {
        if !s.spec.discrete && n > cfg.max_detents {
            fs.push(mk(
                "detents",
                Sev::Low,
                format!("{label}: {n} detents end to end"),
                format!("A full sweep takes at most {} detents.", cfg.max_detents),
                String::new(),
            ));
        } else if !s.spec.discrete && n < cfg.min_detents {
            fs.push(mk(
                "detents",
                Sev::Low,
                format!("{label}: only {n} detents end to end"),
                format!(
                    "A continuous control has at least {} detents.",
                    cfg.min_detents
                ),
                String::new(),
            ));
        }
    }
    let metrics = vec![
        metric(
            format!("knob.{id}.detents"),
            st.detents.unwrap_or(0) as f64,
            true,
        ),
        metric(format!("knob.{id}.max_step"), st.max_step, true),
        metric(format!("knob.{id}.median_step"), st.median_step, false),
        metric(format!("knob.{id}.audible_cents"), st.audible_cents, false),
        metric(format!("knob.{id}.audible_db"), st.audible_db, false),
        metric(format!("knob.{id}.dead_zone"), st.dead_zone as f64, true),
    ];
    let row = json!({
        "id": id, "control": s.spec.control, "view": s.spec.view, "param": s.spec.param.name(),
        "discrete": s.spec.discrete, "stats": nums,
        "start": s.raw.first(), "end": s.raw.last(),
    });
    let _ = out;
    (fs, metrics, row)
}

/// The measured series, for a reviewer who wants the numbers behind a chart.
fn series_json(s: &Sweep) -> Value {
    json!({
        "id": s.spec.id, "control": s.spec.control, "view": s.spec.view,
        "parameter": s.spec.param.name(), "unit": s.spec.param.unit().name(),
        "detents": s.detents, "initial": s.initial,
        "skipped": s.skipped,
        "raw": s.raw, "physical": s.phys, "centroid_hz": s.centroid, "rms_dbfs": s.rms,
    })
}

pub fn run_all(build: &Path, cfg: &Config, out: &Path, only: Option<&str>) -> KnobResult {
    let list: Vec<Spec> = specs()
        .into_iter()
        .filter(|s| only.is_none_or(|o| s.id.contains(o)))
        .collect();
    let sweeps = crate::par_map(&list, |spec| {
        eprintln!("  sweep {}", spec.id);
        run_sweep(build, spec)
    });
    let mut r = KnobResult {
        findings: Vec::new(),
        metrics: Vec::new(),
        table: Vec::new(),
    };
    for sw in &sweeps {
        let (f, m, row) = judge(sw, cfg, out);
        r.findings.extend(f);
        r.metrics.extend(m);
        r.table.push(row);
        let path = out.join(format!("knobs/{}.json", sw.spec.id));
        std::fs::create_dir_all(path.parent().unwrap()).expect("create knobs/");
        std::fs::write(
            path,
            serde_json::to_string(&series_json(sw)).unwrap() + "\n",
        )
        .expect("write a sweep");
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_four_detent_window_is_not_a_five_detent_dead_zone() {
        let raw: Vec<_> = (0..10).map(f64::from).collect();
        let centroid = vec![
            500.0, 1000.0, 1010.0, 1020.0, 1030.0, 1040.0, 2000.0, 3000.0, 4000.0, 5000.0,
        ];
        let s = sweep(P::Slot(0, 1), raw.clone(), raw, centroid, vec![-20.0; 10]);
        assert_eq!(stats(&s, &Config::default()).dead_zone, 4);
        assert!(!judge(&s, &Config::default(), Path::new("/nonexistent"))
            .0
            .iter()
            .any(|f| f.id == "KNOB.dead.T"));
    }

    #[test]
    fn pots_have_no_fine_detents_and_ignore_steps_below_minus_40_db() {
        let phys = vec![0.0, 0.0001, 0.009, 0.01, 0.011, 0.012];
        let mut s = sweep(
            P::Master,
            vec![0.0, 16.0, 32.0, 48.0, 64.0, 80.0],
            phys,
            vec![],
            vec![],
        );
        s.detents = None;
        let st = stats(&s, &Config::default());
        assert!(st.max_step < 1.0, "{}", st.max_step);
        assert_eq!(st.max_step_at, 3);
        let (findings, _, _) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        assert!(!findings.iter().any(|f| f.id == "KNOB.fine.T"));
        s.detents = Some(5);
        let (findings, _, _) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        assert!(findings.iter().any(|f| f.id == "KNOB.jump.T"));
        let mut s = sweep(
            P::Master,
            vec![100.0, 101.0, 102.0],
            vec![0.1, 0.101, 0.102],
            vec![],
            vec![],
        );
        assert!(judge(&s, &Config::default(), Path::new("/nonexistent"))
            .0
            .iter()
            .any(|f| f.id == "KNOB.fine.T"));
        s.detents = None;
        assert!(!judge(&s, &Config::default(), Path::new("/nonexistent"))
            .0
            .iter()
            .any(|f| f.id == "KNOB.fine.T"));
    }

    #[test]
    fn edit_sweeps_are_wet_and_unused_controls_are_skipped() {
        for s in specs()
            .iter()
            .filter(|s| ["K1.edit", "K2.edit", "K3.edit"].contains(&s.id.as_str()))
        {
            assert!(s.setup.contains("holdturn EDIT K4 32"));
        }
        assert!(!control_used(P::Slot(0, 2), 1));
        assert!(control_used(P::Slot(0, 2), 2));
        let mut s = sweep(P::Slot(0, 2), vec![0.0], vec![0.0], vec![], vec![]);
        s.skipped = true;
        let (findings, metrics, row) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        assert!(findings.is_empty() && metrics.is_empty());
        assert_eq!(row["skipped"], true);
    }

    #[test]
    fn centroid_jitter_below_eight_percent_is_a_dead_zone() {
        let raw: Vec<_> = (0..20).map(f64::from).collect();
        let centroid = (0..20)
            .map(|i| if i % 8 < 4 { 1000.0 } else { 1060.0 })
            .collect();
        let s = sweep(P::Slot(0, 1), raw.clone(), raw, centroid, vec![-20.0; 20]);
        assert!(stats(&s, &Config::default()).dead_zone >= 16);
    }

    fn spec(param: P, audio: bool) -> Spec {
        Spec {
            id: "T".into(),
            control: "K1".into(),
            view: "test".into(),
            param,
            setup: String::new(),
            hold: None,
            enc: Some(2),
            audio,
            discrete: false,
            retrigger: false,
        }
    }

    fn sweep(param: P, raw: Vec<f64>, phys: Vec<f64>, centroid: Vec<f64>, rms: Vec<f64>) -> Sweep {
        let n = raw.len();
        let initial = raw[n / 2];
        Sweep {
            skipped: false,
            initial,
            spec: spec(param, !centroid.is_empty()),
            raw,
            phys,
            centroid,
            rms,
            pieces: vec![vec![]; n],
            spectra: vec![],
            detents: Some(n - 1),
        }
    }

    #[test]
    fn a_cutoff_that_covers_two_octaves_is_too_narrow_and_dead_where_the_sound_is_flat() {
        let n = 40;
        let raw: Vec<f64> = (0..n).map(|i| f64::from(i) * 16.0).collect();
        // 2 octaves of cutoff in cents, evenly.
        let phys: Vec<f64> = (0..n)
            .map(|i| 200.0 * 2f64.powf(2.0 * f64::from(i) / f64::from(n - 1)))
            .collect();
        // The sound is flat for the first 25 detents, then brightens.
        let centroid: Vec<f64> = (0..n)
            .map(|i| {
                if i < 25 {
                    1000.0
                } else {
                    1000.0 + 150.0 * f64::from(i - 25)
                }
            })
            .collect();
        let rms = vec![-20.0; n as usize];
        let s = sweep(P::Cutoff, raw, phys, centroid, rms);
        let (fs, _, _) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        let ids: Vec<&str> = fs.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"KNOB.range.T"), "{ids:?}");
        assert!(ids.contains(&"KNOB.dead.T"), "{ids:?}");
        assert!(!ids.contains(&"KNOB.mono.T"));
    }

    #[test]
    fn a_reversal_a_jump_and_a_fine_step_are_found() {
        let raw: Vec<f64> = vec![0.0, 10.0, 20.0, 15.0, 30.0, 100.0, 110.0];
        let s = sweep(P::Character(0), raw.clone(), raw, vec![], vec![]);
        let (fs, _, _) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        let ids: Vec<String> = fs.iter().map(|f| f.id.clone()).collect();
        assert!(ids.contains(&"KNOB.mono.T".to_string()), "{ids:?}");
        assert!(ids.contains(&"KNOB.jump.T".to_string()), "{ids:?}");
        assert!(
            ids.contains(&"KNOB.detents.T".to_string()),
            "too few detents: {ids:?}"
        );
    }

    #[test]
    fn a_healthy_sweep_has_no_findings() {
        let n = 32;
        let raw: Vec<f64> = (0..=n).map(f64::from).map(|i| i * 4.0).collect();
        // Brightness doubles every 8 detents: audible everywhere.
        let centroid: Vec<f64> = (0..=n)
            .map(|i| 500.0 * 2f64.powf(f64::from(i) / 8.0))
            .collect();
        let rms: Vec<f64> = (0..=n).map(|i| -30.0 + f64::from(i) * 0.3).collect();
        let s = sweep(P::Character(0), raw.clone(), raw, centroid, rms);
        let (fs, _, _) = judge(&s, &Config::default(), Path::new("/nonexistent"));
        assert!(
            fs.is_empty(),
            "{:?}",
            fs.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn master_steps_are_in_decibels() {
        let raw: Vec<f64> = (0..=16).map(|i| f64::from(i) * 64.0).collect();
        let phys: Vec<f64> = raw.iter().map(|r| r / 1023.0).collect();
        let s = sweep(P::Master, raw, phys, vec![], vec![]);
        let st = stats(&s, &Config::default());
        assert_eq!(st.unit, "dB");
        // From 1/16 to 2/16 of the pot is a doubling: 6 dB; the last steps are under 1 dB.
        assert!(st.max_step > 5.9 && st.max_step < 6.1, "{}", st.max_step);
    }
}
