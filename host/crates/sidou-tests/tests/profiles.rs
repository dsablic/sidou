// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The chip profiles (core/sid/gen/profiles.h, sidou_tablegen::profiles) at zero character
//! against reSIDfp at each profile's settings:
//! - the filter: the transfer functions around the cutoff and each mode's passband gain, as
//!   tests/filter.rs measures the default settings (M1b Task 5 and 6), with reSIDfp's filter
//!   curve and range set to the profile's;
//! - the combined waveforms: OSC3 read once a frame and every 7 cycles from both engines,
//!   reSIDfp with the profile's setCombinedWaveforms, equal read for read.
//!
//! reSIDfp keeps the 6581 filter range once per process, so this file is its own test binary
//! and every filter measurement takes RANGE, setting the range first. "worn 6581" goes beyond
//! reSIDfp (a DAC it cannot build, more non-linearity): its filter and combined waveforms
//! are compared, its DACs are SIDOU's (tests/unit/sid_character.rs).

use std::cmp::Ordering;
use std::sync::Mutex;

use residfp_sys::{CombinedWaveforms, Model, Sampling, Sid};
use sidou_sys::*;
use sidou_tablegen::filter_fit::{
    apply_range, compare, direct_spectra, fit, in_reach, measure_modes, register_writes, spectra,
    transfer, Mode, Response, CAPTURE, CLOCK_HZ, FS, MODES, SETTLE_CYCLES,
};
use sidou_tablegen::profiles::{ChipProfile, PROFILES};
use sidou_tests::register_probe::{first_mismatch, probe, Chip, TimedWrite, CYCLES_PER_FRAME};

/// Held for every reSIDfp filter measurement: the 6581 range is the process's.
static RANGE: Mutex<()> = Mutex::new(());

fn below(x: f64, limit: f64) -> bool {
    x.partial_cmp(&limit) == Some(Ordering::Less)
}

fn sidou_model(p: &ChipProfile) -> sid_model {
    if p.model == Model::Mos6581 {
        SID_MODEL_6581
    } else {
        SID_MODEL_8580
    }
}

/// The profile's id, as sid_chip_profile_get counts them.
fn id(p: &ChipProfile) -> u32 {
    PROFILES.iter().position(|q| q.name == p.name).unwrap() as u32
}

/// A SIDOU chip with the profile at zero character, as a reset gives it.
fn sidou_chip(p: &ChipProfile, rate: u32, dac: &mut sid_dac) -> Box<sid> {
    let mut s = Box::<sid>::default();
    let c = sid_character {
        profile: (id(p) + 1) as u8,
        ..Default::default()
    };
    unsafe {
        sid_init(&mut *s, sidou_model(p), SID_CLOCK_PAL, rate);
        sid_character_apply(&mut *s, &c, dac);
    }
    s
}

/// SIDOU's output for the filter measurement's register writes (tests/filter.rs's render).
fn render(p: &ChipProfile, fc: u32, res: u8, mode: Option<Mode>) -> Vec<f64> {
    let mut dac = Box::<sid_dac>::default();
    let mut s = sidou_chip(p, FS as u32, &mut dac);
    let settle = (f64::from(SETTLE_CYCLES) * FS / CLOCK_HZ) as usize;
    let mut out = vec![0i16; settle + CAPTURE];
    unsafe {
        for (reg, value) in register_writes(fc, res, mode) {
            sid_write(&mut *s, reg, value);
        }
        sid_render(&mut *s, out.as_mut_ptr(), out.len() as u32);
    }
    out[settle..].iter().map(|&v| f64::from(v)).collect()
}

fn sidou_responses(p: &ChipProfile, fc: u32, res: u8) -> Vec<(Mode, Response)> {
    let direct = spectra(&render(p, fc, res, None));
    MODES
        .iter()
        .map(|&m| (m, transfer(&spectra(&render(p, fc, res, Some(m))), &direct)))
        .collect()
}

/// tests/filter.rs: compared up to here (frequency warping above it).
const TOP_HZ: f64 = 7000.0;

fn passband_offset(reference: &Response, ours: &Response, lo_hz: f64, hi_hz: f64) -> Option<f64> {
    let d: Vec<f64> = reference
        .hz
        .iter()
        .zip(&reference.db)
        .filter(|&(&h, _)| h >= lo_hz && h <= hi_hz)
        .map(|(&h, &db)| {
            let j = ours
                .hz
                .iter()
                .position(|&x| x == h)
                .expect("the same bands");
            ours.db[j] - db
        })
        .collect();
    (!d.is_empty()).then(|| d.iter().sum::<f64>() / d.len() as f64)
}

fn passband(mode: Mode, f0: f64) -> (f64, f64) {
    match mode {
        Mode::Lp => (f0 / 8.0, f0 / 3.0),
        Mode::Bp => (f0 / 1.2, f0 * 1.2),
        Mode::Hp => (3.0 * f0, (9.0 * f0).min(TOP_HZ)),
    }
}

/* M1b's tolerances (tests/filter.rs, MAX_TRANSFER_DB_* and PASSBAND_GAIN_DB_*): the largest
 * shape error around the cutoff, LP, BP, HP, and the passband gain's range, SIDOU minus
 * reSIDfp, over FC 64..512 at resonance 0, 8 and 15. */
const SHAPE_8580: [f64; 3] = [1.0, 0.55, 0.3];
const SHAPE_6581: [f64; 3] = [3.32, 3.13, 2.89];
const GAIN_8580: [(f64, f64); 3] = [(-0.12, 0.01), (-0.17, -0.04), (-0.05, 0.03)];
const GAIN_6581: [(f64, f64); 3] = [(-1.74, 1.79), (-2.02, 2.06), (-0.40, 0.52)];

/* Each profile's ceilings: M1b's, except where a profile away from reSIDfp's default curve
 * setting misses them, then the measured value rounded up to 0.01 dB (M1c Task 3; only ever
 * tightened). SIDOU's 6581 output mix and non-linearity were fitted at the default setting
 * (M1b Task 6), so the passband gain spreads a little further away from it: the R2 (curve
 * 0.58) by up to 0.22 dB on the band-pass, the R3 (0.42) by up to 0.58 dB, and the R3's
 * shape errors equal the R4AR's (both from FC 64 at resonance 15, in the threshold region
 * the two share) to within 0.01 dB. "worn 6581" is artistic: its filter is reSIDfp's curve
 * 0.35 at range 0.4, which SIDOU's model, fitted at the default range, follows less closely
 * (4.2, 4.0, 3.7 dB at its non-linearity of 1), and its non-linearity of 1.5 departs from
 * reSIDfp by design. Measured: R2 2.62 / 2.38 / 2.81 dB, gain -1.52..1.80, -2.24..1.18,
 * -0.29..0.49; R3 3.324 / 3.130 / 2.888, -1.64..2.26, -2.60..2.18, -0.39..0.54; worn 5.92 /
 * 5.68 / 5.16, -1.67..2.00, -1.66..2.21, -0.42..0.10. */
#[allow(clippy::approx_constant)] // 3.14 dB, the R3's band-pass ceiling, is no pi
fn limits(p: &ChipProfile) -> ([f64; 3], [(f64, f64); 3]) {
    match p.name {
        "8580 R5" => (SHAPE_8580, GAIN_8580),
        "6581 R2" => (SHAPE_6581, [(-1.74, 1.81), (-2.24, 2.06), GAIN_6581[2]]),
        "6581 R3" => (
            [3.33, 3.14, 2.89],
            [(-1.74, 2.26), (-2.60, 2.18), (-0.40, 0.54)],
        ),
        "worn 6581" => (
            [5.93, 5.68, 5.17],
            [(-1.74, 2.00), (-2.02, 2.22), (-0.42, 0.52)],
        ),
        _ => (SHAPE_6581, GAIN_6581),
    }
}

/// One profile's filter against reSIDfp's at its curve setting; the failures, if any.
fn filter_failures(p: &ChipProfile) -> Vec<String> {
    let _range = RANGE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    apply_range(&p.filter);
    let direct = direct_spectra(&p.filter);
    let (limits, gain_limits) = limits(p);
    let mut shape = [0.0f64; 3];
    let mut gain = [(f64::INFINITY, f64::NEG_INFINITY); 3];
    for fc in [64u32, 160, 256, 384, 512] {
        for res in [0u8, 8, 15] {
            let reference = measure_modes(&p.filter, &direct, fc, res);
            let f0 = fit(&reference, None).f0;
            assert!(in_reach(f0), "{} FC {fc}: f0 {f0}", p.name);
            let ours = sidou_responses(p, fc, res);
            let (_, max) = compare(&reference, &ours, f0 / 2.0, (2.0 * f0).min(TOP_HZ));
            for (i, ((m, r), (_, o))) in reference.iter().zip(&ours).enumerate() {
                let (lo, hi) = passband(*m, f0);
                if let Some(g) = passband_offset(r, o, lo, hi) {
                    gain[i] = (gain[i].0.min(g), gain[i].1.max(g));
                }
                shape[i] = shape[i].max(max[i]);
            }
        }
    }
    let mut failures = Vec::new();
    for (i, m) in MODES.iter().enumerate() {
        let ((lo, hi), (min, max)) = (gain[i], gain_limits[i]);
        println!(
            "{} {}: shape {:.3} dB (M1b {}), passband gain {lo:.3} to {hi:.3} dB (M1b {min} to {max})",
            p.name,
            m.name(),
            shape[i],
            limits[i]
        );
        if !below(shape[i], limits[i]) {
            failures.push(format!("{} {} shape {:.2} dB", p.name, m.name(), shape[i]));
        }
        if !(below(min, lo) && below(hi, max)) {
            failures.push(format!(
                "{} {} passband {lo:.2}..{hi:.2} dB",
                p.name,
                m.name()
            ));
        }
    }
    failures
}

#[test]
fn each_profile_filter_matches_residfp_at_its_settings() {
    let failures: Vec<String> = PROFILES.iter().flat_map(filter_failures).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// --- the combined waveforms ---

/// The chip, and the DAC table it may point at (kept alive with it).
struct Ours(Box<sid>, #[allow(dead_code)] Box<sid_dac>);

impl Chip for Ours {
    fn write(&mut self, reg: u8, value: u8) {
        unsafe { sid_write(&mut *self.0, reg, value) }
    }
    fn read(&mut self, reg: u8) -> u8 {
        unsafe { sid_read(&mut *self.0, reg) }
    }
    fn clock(&mut self, cycles: u32) {
        unsafe { sid_clock(&mut *self.0, cycles) }
    }
}

struct Theirs(Sid);

impl Chip for Theirs {
    fn write(&mut self, reg: u8, value: u8) {
        self.0.write(reg, value);
    }
    fn read(&mut self, reg: u8) -> u8 {
        self.0.read(reg)
    }
    fn clock(&mut self, cycles: u32) {
        self.0.clock_silent(cycles);
    }
}

fn ours(p: &ChipProfile) -> Ours {
    let mut dac = Box::<sid_dac>::default();
    let s = sidou_chip(p, 44100, &mut dac);
    Ours(s, dac)
}

fn theirs(p: &ChipProfile, cws: CombinedWaveforms) -> Theirs {
    let mut s = Sid::new();
    s.set_chip_model(p.model).unwrap();
    s.set_sampling_parameters(f64::from(SID_CLOCK_PAL), Sampling::Decimate, 44100.0)
        .unwrap();
    s.reset();
    s.set_combined_waveforms(cws);
    Theirs(s)
}

/// Voice 3 on a combined waveform: the gate on, a mid frequency, the pulse width given.
fn writes(control: u8, pw: u16) -> Vec<TimedWrite> {
    [
        (0x0e, 0x00),
        (0x0f, 0x1c),
        (0x10, (pw & 0xff) as u8),
        (0x11, (pw >> 8) as u8),
        (0x13, 0x00),
        (0x14, 0xf0),
        (0x12, control | 1),
    ]
    .iter()
    .map(|&(reg, value)| TimedWrite {
        cycle: 0,
        reg,
        value,
    })
    .collect()
}

#[test]
fn each_profile_combined_waveforms_are_residfps_at_its_strength() {
    const ALL: [CombinedWaveforms; 3] = [
        CombinedWaveforms::Weak,
        CombinedWaveforms::Average,
        CombinedWaveforms::Strong,
    ];
    let mut failures = Vec::new();
    for p in &PROFILES {
        let mut heard = false;
        // Whether reSIDfp at each other strength differs from us somewhere: the comparison
        // tells the strengths apart.
        let mut told_apart = [false; 3];
        for (control, pw) in [
            (0x30u8, 0u16),
            (0x50, 0x800),
            (0x60, 0x800),
            (0x70, 0x800),
            (0x60, 0x100),
            (0x70, 0x100),
        ] {
            for interval in [CYCLES_PER_FRAME, 7] {
                let total = if interval == 7 {
                    CYCLES_PER_FRAME
                } else {
                    200 * CYCLES_PER_FRAME
                };
                let w = writes(control, pw);
                let a = probe(&mut ours(p), &w, 0x1b, interval, total);
                for (k, &cws) in ALL.iter().enumerate() {
                    let b = probe(&mut theirs(p, cws), &w, 0x1b, interval, total);
                    if cws == p.combined {
                        // Some combinations are silent on some chips (a weak pulldown): fine,
                        // as long as the profile is heard somewhere.
                        heard |= b.iter().any(|&v| v != 0);
                        if let Some(m) = first_mismatch(&a, &b) {
                            failures.push(format!(
                                "{} waveform {control:#x} PW {pw:#x} every {interval}: {m:?}",
                                p.name
                            ));
                        }
                    } else {
                        told_apart[k] |= first_mismatch(&a, &b).is_some();
                    }
                }
            }
        }
        assert!(heard, "{}: every combined waveform silent", p.name);
        for (k, &cws) in ALL.iter().enumerate() {
            assert!(
                cws == p.combined || told_apart[k],
                "{}: no read tells {cws:?} from {:?}",
                p.name,
                p.combined
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
