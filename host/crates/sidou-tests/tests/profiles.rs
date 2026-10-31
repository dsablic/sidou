// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_tests::below_and_not_nan;
use std::sync::Mutex;

use residfp_sys::{CombinedWaveforms, Model, Sampling, Sid};
use sidou_sys::*;
use sidou_tablegen::filter_fit::{
    apply_range, compare, direct_spectra, fit, in_reach, measure_modes, CurveSetting, Mode,
    Response, FS, MODES,
};
use sidou_tablegen::profiles::{curve_row, ChipProfile, PROFILES};
use sidou_tests::filter_limits::{
    MAX_TRANSFER_DB_6581, MAX_TRANSFER_DB_8580, PASSBAND_GAIN_DB_6581, PASSBAND_GAIN_DB_8580,
};
use sidou_tests::filter_passband::{passband, passband_offset, responses, settled_output, TOP_HZ};
use sidou_tests::register_probe::{first_mismatch, probe, Chip, TimedWrite, CYCLES_PER_FRAME};

static RESIDFP_FILTER_RANGE_IS_PER_PROCESS: Mutex<()> = Mutex::new(());

fn sidou_model(p: &ChipProfile) -> sid_model {
    if p.model == Model::Mos6581 {
        SID_MODEL_6581
    } else {
        SID_MODEL_8580
    }
}

fn profile_id(p: &ChipProfile) -> u32 {
    PROFILES.iter().position(|q| q.name == p.name).unwrap() as u32
}

fn sidou_chip(p: &ChipProfile, rate: u32, dac: &mut sid_dac) -> Box<sid> {
    let mut s = Box::<sid>::default();
    let c = sid_character {
        profile: (profile_id(p) + 1) as u8,
        ..Default::default()
    };
    unsafe {
        sid_init(&mut *s, sidou_model(p), SID_CLOCK_PAL, rate);
        sid_character_apply(&mut *s, &c, dac);
    }
    s
}

#[derive(Clone, Copy)]
enum Target<'a> {
    Profile(&'a ChipProfile),
    Row(Model, u32),
}

impl Target<'_> {
    fn chip(&self, rate: u32, dac: &mut sid_dac) -> Box<sid> {
        match *self {
            Target::Profile(p) => sidou_chip(p, rate, dac),
            Target::Row(model, row) => {
                let mut s = Box::<sid>::default();
                let m = if model == Model::Mos6581 {
                    SID_MODEL_6581
                } else {
                    SID_MODEL_8580
                };
                unsafe {
                    sid_init(&mut *s, m, SID_CLOCK_PAL, rate);
                    sid_set_filter_curve(&mut *s, row);
                    sid_set_filter_nonlinearity(&mut *s, 1.0);
                }
                s
            }
        }
    }
}

fn sidou_output(p: Target, fc: u32, res: u8, mode: Option<Mode>) -> Vec<f64> {
    let mut dac = Box::<sid_dac>::default();
    let mut s = p.chip(FS as u32, &mut dac);
    settled_output(&mut s, fc, res, mode)
}

fn sidou_responses(p: Target, fc: u32, res: u8) -> Vec<(Mode, Response)> {
    responses(|mode| sidou_output(p, fc, res, mode))
}

const SHAPE_8580: [f64; 3] = MAX_TRANSFER_DB_8580;
const SHAPE_6581: [f64; 3] = MAX_TRANSFER_DB_6581;
const GAIN_8580: [(f64, f64); 3] = PASSBAND_GAIN_DB_8580;
const GAIN_6581: [(f64, f64); 3] = PASSBAND_GAIN_DB_6581;

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

fn transfer_errors(
    name: &str,
    setting: &CurveSetting,
    ours: Target,
) -> ([f64; 3], [(f64, f64); 3]) {
    let _range = RESIDFP_FILTER_RANGE_IS_PER_PROCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    apply_range(setting);
    let direct = direct_spectra(setting);
    let mut shape = [0.0f64; 3];
    let mut gain = [(f64::INFINITY, f64::NEG_INFINITY); 3];
    for fc in [64u32, 160, 256, 384, 512] {
        for res in [0u8, 8, 15] {
            let reference = measure_modes(setting, &direct, fc, res);
            let f0 = fit(&reference, None).f0;
            assert!(in_reach(f0), "{name} FC {fc}: f0 {f0}");
            let ours = sidou_responses(ours, fc, res);
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
    (shape, gain)
}

fn filter_failures(p: &ChipProfile) -> Vec<String> {
    let (limits, gain_limits) = limits(p);
    let (shape, gain) = transfer_errors(p.name, &p.filter, Target::Profile(p));
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
        if !below_and_not_nan(shape[i], limits[i]) {
            failures.push(format!("{} {} shape {:.2} dB", p.name, m.name(), shape[i]));
        }
        if !(below_and_not_nan(min, lo) && below_and_not_nan(hi, max)) {
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
    let s = Target::Profile(p).chip(44100, &mut dac);
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

fn voice3_combined_wave_writes(control: u8, pw: u16) -> Vec<TimedWrite> {
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
                let w = voice3_combined_wave_writes(control, pw);
                let a = probe(&mut ours(p), &w, 0x1b, interval, total);
                for (k, &cws) in ALL.iter().enumerate() {
                    let b = probe(&mut theirs(p, cws), &w, 0x1b, interval, total);
                    if cws == p.combined {
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

#[test]
fn the_age_rows_against_residfp_recorded() {
    for model in [Model::Mos6581, Model::Csg8580] {
        let p = PROFILES.iter().find(|p| p.model == model).unwrap();
        let row = curve_row(&p.hot) as u32;
        let name = format!("{model:?} age row");
        let (shape, gain) = transfer_errors(&name, &p.hot, Target::Row(model, row));
        for (i, m) in MODES.iter().enumerate() {
            println!(
                "{name} {}: shape {:.3} dB, passband gain {:.3} to {:.3} dB",
                m.name(),
                shape[i],
                gain[i].0,
                gain[i].1
            );
        }
    }
}
