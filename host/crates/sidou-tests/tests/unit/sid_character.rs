// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The chip's character (core/sid/sid_character.c): the profiles at zero character are their
//! settings exactly, each control moves the sound the way it says and measurably, a sweep of
//! any control does not click, and the settings clamp. The comparison with reSIDfp at each
//! profile's settings is tests/profiles.rs.

use sidou_sys::*;

const FS: u32 = 44100;
static EXPERT_REFERENCE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn chip(model: sid_model) -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, FS) };
    s
}

/// A character: profile as live sends it (sid_character's profile is 1 + the id).
fn character(profile: u32, age: u8, dac: u8, thump: u8, bleed: u8) -> sid_character {
    sid_character {
        profile: (profile + 1) as u8,
        age,
        dac,
        thump,
        bleed,
        ..Default::default()
    }
}

fn model_of(profile: u32) -> sid_model {
    unsafe { (*sid_chip_profile_get(profile)).model as sid_model }
}

fn write(s: &mut sid, regs: &[(u8, u8)]) {
    for &(r, v) in regs {
        unsafe { sid_write(s, r, v) };
    }
}

fn render(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render(s, out.as_mut_ptr(), n as u32) };
    out
}

/// A held chord of saws (and voice 3 a pulse) through the low-pass at FC 1024, resonance 8:
/// the filter, the DACs and the output stage all in play.
const CHORD: [(u8, u8); 22] = [
    (0x15, 0x00),
    (0x16, 0x80),
    (0x17, 0x87),
    (0x18, 0x1f),
    (0x00, 0x00),
    (0x01, 0x11),
    (0x05, 0x00),
    (0x06, 0xf0),
    (0x04, 0x21),
    (0x07, 0x00),
    (0x08, 0x16),
    (0x0c, 0x00),
    (0x0d, 0xf0),
    (0x0b, 0x21),
    (0x0e, 0x00),
    (0x0f, 0x1c),
    (0x10, 0x00),
    (0x11, 0x08),
    (0x13, 0x00),
    (0x14, 0xf0),
    (0x12, 0x41),
    (0x18, 0x1f),
];

/// A chip with the character applied after sid_init, as a reset gives it (live_sid_sink,
/// prepare_spare), then the chord.
fn with_character(c: &sid_character, dac: &mut sid_dac) -> Box<sid> {
    let model = if c.profile == 0 {
        SID_MODEL_6581
    } else {
        model_of(u32::from(c.profile) - 1)
    };
    let mut s = chip(model);
    unsafe { sid_character_apply(&mut *s, c, dac) };
    write(&mut s, &CHORD);
    crate::settle(&mut s);
    s
}

fn rms(x: &[i16]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// The energy above `hz` against the total: a first difference, a crude high-pass that is
/// enough to rank brightness.
fn brightness(x: &[i16]) -> f64 {
    let d: Vec<i16> = x.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();
    rms(&d) / rms(x)
}

#[test]
fn expert_dac_edits_are_independent_and_macro_changes_release_them() {
    let mut c = sid_character::default();
    unsafe {
        sid_character_init(&mut c);
        sid_character_expert_set(&mut c, SID_MODEL_6581, SID_EXPERT_DAC_KINK, 127);
    }
    let s = chip(SID_MODEL_6581);
    let mut d = Box::<sid_dac>::default();
    let mut reference = Box::<sid_dac>::default();
    unsafe {
        sid_character_build_dac(&*s, &c, &mut *d);
        sid_dac_build(&mut *reference, 1, 2.6, SID_DAC_LEAKAGE_6581 as f32);
    }
    assert_eq!(d.wave, reference.wave);
    c.dac = 127;
    unsafe { sid_character_macro_changed(&mut c, 2) };
    unsafe {
        sid_character_build_dac(&*s, &c, &mut *d);
        sid_dac_build(
            &mut *reference,
            1,
            (SID_DAC_KINK_6581 as f32) + (SID_CHARACTER_KINK as f32),
            (SID_DAC_LEAKAGE_6581 as f32) * (SID_CHARACTER_LEAK as f32),
        );
    }
    assert_eq!(d.wave, reference.wave);
}

#[test]
fn expert_constants_change_the_stage_without_resetting_held_voices() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, &CHORD);
    render(&mut s, 5000);
    let before: Vec<_> = s
        .osc
        .iter()
        .zip(s.env.iter())
        .map(|(o, e)| (o.acc, o.control, e.counter))
        .collect();
    let mut c = sid_character::default();
    unsafe {
        sid_character_init(&mut c);
        sid_character_expert_set(&mut c, SID_MODEL_6581, SID_EXPERT_NONLINEARITY, 0);
        sid_character_expert_set(&mut c, SID_MODEL_6581, SID_EXPERT_FILTER_BIAS, 0);
        sid_character_expert_set(&mut c, SID_MODEL_6581, SID_EXPERT_EXTERNAL_HP, 127);
        sid_character_expert_set(&mut c, SID_MODEL_6581, SID_EXPERT_EXTERNAL_LP, 0);
    }
    let mut stage = sid_character_stage::default();
    unsafe {
        sid_character_stage_for(&*s, &c, std::ptr::null(), &mut stage);
        sid_character_commit(&mut *s, &stage);
    }
    assert_eq!((s.nl.level, s.dc_in), (0.0, 0.0));
    assert!((s.dc_r - (1.0 - 2.0 * std::f32::consts::PI * 100.0 / FS as f32)).abs() < 1e-6);
    assert!(s.out_c < 0.15);
    let after: Vec<_> = s
        .osc
        .iter()
        .zip(s.env.iter())
        .map(|(o, e)| (o.acc, o.control, e.counter))
        .collect();
    assert_eq!(after, before);
    write(&mut s, &[(0x17, 0x87)]);
    assert_eq!(s.dc_in, 0.0, "routing writes must retain the expert bias");
}

#[test]
fn expert_strength_recomputes_the_fast_combined_oscillators_derived_mode() {
    let mut actual = chip(SID_MODEL_6581);
    let mut expected = chip(SID_MODEL_6581);
    for s in [&mut actual, &mut expected] {
        write(s, &CHORD);
        write(s, &[(0x04, 0x31)]);
        render(s, 10000);
    }
    assert_eq!((actual.osc[0].slow, actual.osc[0].pull_low), (0, 1));
    let before = actual.osc[0].acc;
    let mut c = sid_character::default();
    let mut stage = sid_character_stage::default();
    unsafe {
        sid_character_expert_set(&mut c, actual.model, SID_EXPERT_COMBINED, 127);
        sid_character_stage_for(&*actual, &c, actual.dac, &mut stage);
        sid_character_commit(&mut *actual, &stage);
        sid_set_combined_profile(&mut *expected, SID_PROFILE_STRONG as i32);
    }
    assert_eq!(actual.osc[0].acc, before);
    assert_eq!((actual.osc[0].slow, actual.osc[0].pull_low), (1, 0));
    assert_eq!(render(&mut actual, 20000), render(&mut expected, 20000));
}

#[test]
fn expert_voice_dc_is_baked_into_every_dac_entry_without_changing_the_envelope() {
    let s = chip(SID_MODEL_6581);
    let mut c = sid_character::default();
    let mut baseline = Box::<sid_dac>::default();
    let mut edited = Box::<sid_dac>::default();
    unsafe {
        sid_character_init(&mut c);
        sid_character_build_dac(&*s, &c, &mut *baseline);
        sid_character_expert_set(&mut c, s.model, SID_EXPERT_VOICE_DC, 0);
        sid_character_build_dac(&*s, &c, &mut *edited);
    }
    assert_eq!(baseline.env, edited.env);
    for (a, b) in baseline.wave.iter().zip(edited.wave.iter()) {
        assert!((a - b - 2.0 * (0.2143 / 1.5)).abs() < 5e-7);
    }
}

#[test]
fn expert_dac_ramp_builds_offline_and_preserves_gates() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, &CHORD);
    render(&mut s, 5000);
    let c = sid_character::default();
    let mut ramp = sid_character_ramp::default();
    unsafe {
        sid_character_ramp_reset(&mut ramp, &c, std::ptr::null());
        sid_character_expert_set(&mut ramp.target, s.model, SID_EXPERT_DAC_KINK, 127);
    }
    let mut tables = [Box::<sid_dac>::default(), Box::<sid_dac>::default()];
    for _ in 0..600 {
        let index = usize::from(std::ptr::eq(s.dac, &*tables[0]));
        let mut st = sid_character_stage::default();
        let acc = s.osc[0].acc;
        let updated =
            unsafe { sid_character_ramp_tick(&mut ramp, &*s, &mut *tables[index], &mut st) };
        assert_eq!(
            s.osc[0].acc, acc,
            "producer must not advance/reset the oscillator"
        );
        if updated != 0 {
            unsafe { sid_character_commit(&mut *s, &st) };
        }
        assert!(s.osc.iter().all(|o| o.control & 1 != 0));
        render(&mut s, 441);
    }
    let mut target = Box::<sid_dac>::default();
    unsafe { sid_character_build_dac(&*s, &ramp.target, &mut *target) };
    assert_eq!(unsafe { &*s.dac }.wave, target.wave);
}

#[test]
fn preparing_with_leased_rows_does_not_overwrite_the_spares_own_backing() {
    let mut spare = chip(SID_MODEL_6581);
    let mut backing = chip(SID_MODEL_6581);
    spare.cutoff_g.fill(123.0);
    spare.cutoff_x_to.fill(456.0);
    let rows = unsafe {
        [
            sid_filter_backing(&mut *backing, 0),
            sid_filter_backing(&mut *backing, 1),
        ]
    };
    let mut prep = sid_character_prep::default();
    let c = character(SID_CHIP_6581_R2, 0, 0, 0, 0);
    unsafe {
        sid_character_prep_start(
            &mut prep,
            &mut *spare,
            SID_MODEL_6581,
            SID_CLOCK_PAL,
            FS,
            &c,
            0,
        );
        sid_character_prep_rows(&mut prep, rows.as_ptr());
        while sid_character_prep_step(&mut prep, std::ptr::null_mut()) == 0 {}
    }
    assert!(spare.cutoff_g.iter().all(|&v| v == 123.0));
    assert!(spare.cutoff_x_to.iter().all(|&v| v == 456.0));
    assert_eq!(spare.filter_rows[0].cutoff, rows[0].cutoff);
    assert!(backing.cutoff_g.iter().any(|&v| v != 123.0));
}

#[test]
fn independent_expert_filter_coordinates_move_cutoff_monotonically() {
    for (model, index, increasing) in [
        (SID_MODEL_6581, SID_EXPERT_FILTER_6581_CURVE, true),
        (SID_MODEL_6581, SID_EXPERT_FILTER_6581_RANGE, true),
        (SID_MODEL_8580, SID_EXPERT_FILTER_8580_CURVE, false),
    ] {
        let mut s = chip(model);
        let rows = unsafe {
            [
                sid_filter_backing(&mut *s, 0),
                sid_filter_backing(&mut *s, 1),
            ]
        };
        let mut c = sid_character::default();
        let mut previous = None;
        for value in 0..=127 {
            unsafe {
                sid_character_expert_set(&mut c, model, index, value);
                let mut job = sid_character_filter_job::default();
                sid_character_filter_job_start(&mut job, &*s, &c, rows.as_ptr());
                while sid_character_filter_job_step(&mut job) == 0 {}
            }
            let cutoff = s.cutoff_g[1024];
            if let Some(previous) = previous {
                assert!(
                    if increasing {
                        cutoff > previous
                    } else {
                        cutoff < previous
                    },
                    "model {model} index {index} value {value}: {cutoff} after {previous}"
                );
            }
            previous = Some(cutoff);
        }
    }
}

#[test]
fn expert_filter_endpoints_reach_the_offline_reference_cutoff() {
    use sidou_tablegen::filter_fit::{self, CurveSetting};
    let _reference_guard = EXPERT_REFERENCE_LOCK.lock().unwrap();

    for (model, index) in [
        (SID_MODEL_6581, SID_EXPERT_FILTER_6581_CURVE),
        (SID_MODEL_6581, SID_EXPERT_FILTER_6581_RANGE),
        (SID_MODEL_8580, SID_EXPERT_FILTER_8580_CURVE),
    ] {
        for value in [0, 127] {
            let mut c = sid_character::default();
            let mut s = chip(model);
            unsafe {
                sid_character_expert_set(&mut c, model, index, value);
                sid_character_filter(&mut *s, &c);
            }
            let curve_index = if model == SID_MODEL_6581 {
                SID_EXPERT_FILTER_6581_CURVE
            } else {
                SID_EXPERT_FILTER_8580_CURVE
            };
            let setting = CurveSetting {
                model: if model == SID_MODEL_6581 {
                    residfp_sys::Model::Mos6581
                } else {
                    residfp_sys::Model::Csg8580
                },
                curve: unsafe { sid_character_expert_value(&c, model, curve_index) } as f64,
                range6581: Some(unsafe {
                    sid_character_expert_value(&c, model, SID_EXPERT_FILTER_6581_RANGE)
                } as f64),
            };
            filter_fit::apply_range(&setting);
            let direct = filter_fit::direct_spectra(&setting);
            let response = filter_fit::measure_modes(&setting, &direct, 1024, 8);
            let expected = filter_fit::fit(&response, None).f0.clamp(30.0, 19845.0);
            let actual = f64::from(s.cutoff_g[1024]).atan() * f64::from(FS) / std::f64::consts::PI;
            assert!(
                (actual / expected - 1.0).abs() < 1e-4,
                "model {model} index {index} value {value}: {actual} vs reference {expected}"
            );
        }
    }
}

#[test]
fn generated_expert_grid_matches_reference_at_every_curve_range_anchor() {
    use sidou_tablegen::filter_fit::{self, CurveSetting, Mode};
    let _reference_guard = EXPERT_REFERENCE_LOCK.lock().unwrap();
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let curves: &[f64] = if model == SID_MODEL_6581 {
            &[0.0, 0.25, 0.5, 0.75, 1.0]
        } else {
            &[0.0, 0.5, 1.0]
        };
        let ranges: &[f64] = if model == SID_MODEL_6581 {
            &[0.0, 0.3, 19.0 / 39.0, 1.0]
        } else {
            &[19.0 / 39.0]
        };
        for &range in ranges {
            for &curve in curves {
                let setting = CurveSetting {
                    model: if model == SID_MODEL_6581 {
                        residfp_sys::Model::Mos6581
                    } else {
                        residfp_sys::Model::Csg8580
                    },
                    curve,
                    range6581: Some(range),
                };
                filter_fit::apply_range(&setting);
                let direct = filter_fit::direct_spectra(&setting);
                let response = filter_fit::measure_modes(&setting, &direct, 1024, 8);
                let fit = filter_fit::fit(&response, None);
                let expected = if fit.f0 > 19845.0 && fit.rms_db == [0.0; 3] {
                    let bp = &response
                        .iter()
                        .find(|(mode, _)| *mode == Mode::Bp)
                        .unwrap()
                        .1;
                    if bp.db.iter().copied().fold(f64::NEG_INFINITY, f64::max) < -30.0 {
                        30.0
                    } else {
                        19845.0
                    }
                } else {
                    fit.f0.clamp(30.0, 19845.0)
                };
                let actual = unsafe {
                    sid_character_expert_cutoff_hz(model, curve as f32, range as f32, 1024)
                };
                assert!(
                    (f64::from(actual) / expected - 1.0).abs() < 1e-4,
                    "model {model} curve {curve} range {range}: {actual} vs reference {expected}"
                );
            }
        }
    }
}

#[test]
fn expert_filter_crossfades_leased_rows_then_restores_exact_macro_tables() {
    let mut s = chip(SID_MODEL_6581);
    let plain = chip(SID_MODEL_6581);
    let mut banks = [chip(SID_MODEL_6581), chip(SID_MODEL_6581)];
    write(&mut s, &CHORD);
    render(&mut s, 5000);
    let c = sid_character::default();
    let mut ramp = sid_character_ramp::default();
    unsafe {
        sid_character_ramp_reset(&mut ramp, &c, std::ptr::null());
        sid_character_expert_set(&mut ramp.target, s.model, SID_EXPERT_FILTER_6581_CURVE, 127);
    }
    for release in [false, true] {
        if release {
            unsafe { sid_character_macro_changed(&mut ramp.target, 1) };
        }
        for _ in 0..200 {
            let free = banks
                .iter_mut()
                .find(|bank| {
                    bank.cutoff_g.as_ptr() != s.filter_rows[0].cutoff
                        && bank.cutoff_g.as_ptr() != s.filter_rows[1].cutoff
                        && bank.cutoff_x_to.as_ptr() != s.filter_rows[0].cutoff
                        && bank.cutoff_x_to.as_ptr() != s.filter_rows[1].cutoff
                })
                .expect("one unleased row pair");
            let rows = unsafe {
                [
                    sid_filter_backing(&mut **free, 0),
                    sid_filter_backing(&mut **free, 1),
                ]
            };
            let mut st = sid_character_stage::default();
            let acc = s.osc[0].acc;
            let updated = unsafe {
                sid_character_ramp_tick_tables(
                    &mut ramp,
                    &*s,
                    std::ptr::null_mut(),
                    rows.as_ptr(),
                    &mut st,
                )
            };
            if updated != 0 {
                unsafe { sid_character_commit(&mut *s, &st) };
            }
            assert_eq!(s.osc[0].acc, acc);
            assert!(s.osc.iter().all(|o| o.control & 1 != 0));
            render(&mut s, 441);
        }
        assert_eq!(
            (
                ramp.filter_building,
                ramp.filter_fading,
                ramp.filter_restore
            ),
            (0, 0, 0)
        );
        let actual =
            unsafe { std::slice::from_raw_parts(s.filter_rows[0].cutoff, SID_FC_STEPS as usize) };
        if release {
            assert_eq!(actual, plain.cutoff_g);
            assert_eq!(s.filter_t, 0.0);
        } else {
            assert!(unsafe { sid_blended_cutoff(&*s, 1024, s.filter_t) } > plain.cutoff_g[1024]);
            assert_eq!(s.filter_t, 1.0);
        }
    }
}

#[test]
fn disabling_noise_writeback_keeps_the_lfsr_alive_in_combined_waveforms() {
    let mut survivors = Vec::new();
    for value in [SID_EXPERT_DEFAULT, 0] {
        let mut s = chip(SID_MODEL_6581);
        let mut c = sid_character::default();
        unsafe {
            sid_character_expert_set(&mut c, s.model, SID_EXPERT_NOISE_WRITEBACK, value);
            sid_character_apply(&mut *s, &c, std::ptr::null_mut());
        }
        write(&mut s, &[(0x00, 0xff), (0x01, 0xff), (0x04, 0x91)]);
        unsafe { sid_clock(&mut *s, 100000) };
        survivors.push(s.osc[0].sr);
    }
    assert_eq!(
        survivors[0], 0,
        "the physical writeback locks up this combination"
    );
    assert_ne!(
        survivors[1], 0,
        "disabled writeback must preserve shifted noise bits"
    );
    let mut pure = chip(SID_MODEL_6581);
    write(&mut pure, &[(0x00, 0xff), (0x01, 0xff), (0x04, 0x81)]);
    unsafe {
        sid_clock(&mut *pure, 100000);
        sid_noise_settle(&mut pure.osc[0]);
    }
    assert_eq!(
        survivors[1], pure.osc[0].sr,
        "no feedback means the unmodified LFSR sequence"
    );
}

fn freeze_filter_rows(stage: &mut sid_character_stage) -> [Box<[f32]>; 4] {
    let mut data: [Box<[f32]>; 4] = unsafe {
        [
            std::slice::from_raw_parts(stage.filter_rows[0].cutoff, SID_FC_STEPS as usize).into(),
            std::slice::from_raw_parts(stage.filter_rows[0].res, 16).into(),
            std::slice::from_raw_parts(stage.filter_rows[1].cutoff, SID_FC_STEPS as usize).into(),
            std::slice::from_raw_parts(stage.filter_rows[1].res, 16).into(),
        ]
    };
    stage.filter_rows[0].cutoff = data[0].as_mut_ptr();
    stage.filter_rows[0].res = data[1].as_mut_ptr();
    stage.filter_rows[1].cutoff = data[2].as_mut_ptr();
    stage.filter_rows[1].res = data[3].as_mut_ptr();
    data
}

fn expert_click_residual(model: sid_model, index: u32, at: usize, release_age: bool) -> i32 {
    let mut c = sid_character::default();
    unsafe { sid_character_expert_set(&mut c, model, index, 0) };
    let mut active = chip(model);
    let mut smooth = chip(model);
    let mut initial = Box::<sid_dac>::default();
    let mut reference_initial = Box::<sid_dac>::default();
    let mut banks = [chip(model), chip(model)];
    let mut dacs = [Box::<sid_dac>::default(), Box::<sid_dac>::default()];
    let table = unsafe { sid_character_apply(&mut *active, &c, &mut *initial) };
    unsafe { sid_character_apply(&mut *smooth, &c, &mut *reference_initial) };
    let control = match index {
        SID_EXPERT_FILTER_6581_CURVE
        | SID_EXPERT_FILTER_6581_RANGE
        | SID_EXPERT_FILTER_8580_CURVE
        | SID_EXPERT_NONLINEARITY
        | SID_EXPERT_FILTER_BIAS => 0,
        SID_EXPERT_BLEED => 3,
        _ => 1,
    };
    write(&mut active, &triangle(control));
    write(&mut smooth, &triangle(control));
    render(&mut active, FS as usize);
    render(&mut smooth, FS as usize);
    let mut ramp = sid_character_ramp::default();
    unsafe { sid_character_ramp_reset(&mut ramp, &c, table) };
    let mut first = sid_character_stage::default();
    unsafe { sid_character_stage_for(&*active, &c, active.dac, &mut first) };
    let mut first_dac = Box::new(unsafe { *first.dac });
    first.dac = &mut *first_dac;
    let mut row_ids = vec![[first.filter_rows[0].cutoff, first.filter_rows[1].cutoff]];
    let mut row_snapshots = vec![freeze_filter_rows(&mut first)];
    let mut events = vec![(0usize, first, first_dac)];
    let mut stepped = Vec::new();
    let ticks = at + if release_age { 820 } else { 520 };
    for tick in 0..ticks {
        if tick == at {
            unsafe { sid_character_expert_set(&mut ramp.target, model, index, 127) };
        }
        if release_age && tick == at + 180 {
            unsafe { sid_character_macro_changed(&mut ramp.target, 1) };
        }
        if release_age && tick == at + 240 {
            ramp.target.age = 127;
        }
        if release_age && tick == at + 495 {
            ramp.target.age = 0;
        }
        let bank = banks
            .iter_mut()
            .find(|bank| {
                bank.cutoff_g.as_ptr() != active.filter_rows[0].cutoff
                    && bank.cutoff_g.as_ptr() != active.filter_rows[1].cutoff
                    && bank.cutoff_x_to.as_ptr() != active.filter_rows[0].cutoff
                    && bank.cutoff_x_to.as_ptr() != active.filter_rows[1].cutoff
            })
            .expect("free filter pair");
        let rows = unsafe {
            [
                sid_filter_backing(&mut **bank, 0),
                sid_filter_backing(&mut **bank, 1),
            ]
        };
        let free_dac = usize::from(active.dac == &*dacs[0]);
        let mut stage = sid_character_stage::default();
        let updated = unsafe {
            sid_character_ramp_tick_tables(
                &mut ramp,
                &*active,
                &mut *dacs[free_dac],
                rows.as_ptr(),
                &mut stage,
            )
        };
        if updated != 0 {
            unsafe { sid_character_commit(&mut *active, &stage) };
            let mut snapshot = Box::new(unsafe { *stage.dac });
            stage.dac = &mut *snapshot;
            row_ids.push([stage.filter_rows[0].cutoff, stage.filter_rows[1].cutoff]);
            row_snapshots.push(freeze_filter_rows(&mut stage));
            events.push((tick * TICK, stage, snapshot));
        }
        stepped.extend(render(&mut active, TICK));
    }
    assert_eq!(
        (ramp.building, ramp.filter_building, ramp.filter_fading),
        (0, 0, 0)
    );
    let end = events.last().unwrap();
    let final_stage = end.1;
    let mut final_dac = Box::new(*end.2);
    let mut final_stage = final_stage;
    final_stage.dac = &mut *final_dac;
    row_ids.push(*row_ids.last().unwrap());
    events.push((ticks * TICK, final_stage, final_dac));
    let centers: Vec<usize> = events.windows(2).map(|w| (w[0].0 + w[1].0) / 2).collect();
    let mut between = Box::<sid_dac>::default();
    let mut reference = Vec::with_capacity(stepped.len());
    let mut event = 0usize;
    let dac_control = [
        SID_EXPERT_DAC_KINK,
        SID_EXPERT_DAC_LEAKAGE,
        SID_EXPERT_VOICE_DC,
    ]
    .contains(&index);
    for sample in 0..stepped.len() {
        while event + 1 < centers.len() && sample >= centers[event + 1] {
            event += 1;
        }
        let next = (event + 1).min(centers.len() - 1);
        let t = if next == event || sample < centers[event] {
            0.0
        } else {
            (sample - centers[event]) as f32 / (centers[next] - centers[event]) as f32
        };
        let a = events[event].1;
        let b = events[next].1;
        let mut stage = b;
        stage.nl.level = lerp(a.nl.level, b.nl.level, t);
        stage.nl.level_off = lerp(a.nl.level_off, b.nl.level_off, t);
        stage.nl.load = lerp(a.nl.load, b.nl.load, t);
        stage.nl.floor = lerp(a.nl.floor, b.nl.floor, t);
        stage.thump = lerp(a.thump, b.thump, t);
        stage.bleed = lerp(a.bleed, b.bleed, t);
        stage.output_gain = lerp(a.output_gain, b.output_gain, t);
        let from_t = if row_ids[event] == row_ids[next] {
            a.filter_t
        } else if b.filter_curve == SID_EXPERT_DEFAULT as u8 {
            0.0
        } else {
            b.filter_t
        };
        stage.filter_t = lerp(from_t, b.filter_t, t);
        stage.filter_bias = lerp(a.filter_bias, b.filter_bias, t);
        stage.out_c = lerp(a.out_c, b.out_c, t);
        stage.dc_r = lerp(a.dc_r, b.dc_r, t);
        if dac_control {
            if sample % 16 == 0 {
                let (da, db) = unsafe { (&*a.dac, &*b.dac) };
                for (out, (&a, &b)) in between
                    .wave
                    .iter_mut()
                    .zip(da.wave.iter().zip(db.wave.iter()))
                {
                    *out = lerp(a, b, t);
                }
                for (out, (&a, &b)) in between.env.iter_mut().zip(da.env.iter().zip(db.env.iter()))
                {
                    *out = lerp(a, b, t);
                }
            }
            stage.dac = &*between;
        }
        unsafe { sid_character_commit(&mut *smooth, &stage) };
        reference.extend(render(&mut smooth, 1));
    }
    residual(&stepped, &reference)
}

#[test]
fn expert_jumps_stay_within_the_existing_click_residual_bound() {
    for index in 0..SID_EXPERT_COUNT {
        if index == SID_EXPERT_FILTER_8580_CURVE {
            continue;
        }
        for phase in 0..3 {
            let error = expert_click_residual(SID_MODEL_6581, index, BEFORE + phase, false);
            assert!(
                error <= RESIDUAL_MAX,
                "expert {index} phase {phase}: {error} LSB"
            );
        }
    }
}

#[test]
fn age_changes_during_override_release_and_restore_do_not_click() {
    for phase in 0..3 {
        let error = expert_click_residual(
            SID_MODEL_6581,
            SID_EXPERT_FILTER_6581_CURVE,
            BEFORE + phase,
            true,
        );
        assert!(
            error <= RESIDUAL_MAX,
            "AGE during release phase {phase}: {error} LSB"
        );
    }
}

#[test]
fn the_profiles_are_tablegens() {
    let names: Vec<String> = (0..SID_CHIPS)
        .map(|i| unsafe {
            std::ffi::CStr::from_ptr((*sid_chip_profile_get(i)).name)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names,
        ["6581 R2", "6581 R3", "6581 R4AR", "8580 R5", "worn 6581"]
    );
    assert!(unsafe { sid_chip_profile_get(SID_CHIPS) }.is_null());
    for (i, p) in sidou_tablegen::profiles::PROFILES.iter().enumerate() {
        let c = unsafe { &*sid_chip_profile_get(i as u32) };
        let model = match p.model {
            residfp_sys::Model::Mos6581 => SID_MODEL_6581,
            residfp_sys::Model::Csg8580 => SID_MODEL_8580,
        };
        assert_eq!(c.model as sid_model, model, "{}", p.name);
        assert_eq!(
            usize::from(c.curve),
            sidou_tablegen::profiles::curve_row(&p.filter)
        );
        assert_eq!(
            usize::from(c.hot_curve),
            sidou_tablegen::profiles::curve_row(&p.hot)
        );
        assert_eq!(f64::from(c.nonlinearity), p.nonlinearity);
    }
    assert_eq!(
        unsafe { sid_chip_default(SID_MODEL_6581) },
        SID_CHIP_6581_R4AR
    );
    assert_eq!(
        unsafe { sid_chip_default(SID_MODEL_8580) },
        SID_CHIP_8580_R5
    );
}

/// The default profiles at zero character are what sid_init gives, bit for bit: the golden
/// hashes and reSIDfp accuracy are untouched. So is the zero-initialised character (profile
/// 0: the model's default).
#[test]
fn the_default_profiles_at_zero_character_are_sid_init_exactly() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut plain = chip(model);
        write(&mut plain, &CHORD);
        let want = render(&mut plain, 20000);
        let mut dac = Box::<sid_dac>::default();
        let profile = unsafe { sid_chip_default(model) };
        for c in [character(profile, 0, 0, 0, 0), sid_character::default()] {
            let mut s = chip(model);
            unsafe { sid_character_apply(&mut *s, &c, &mut *dac) };
            assert_eq!(s.dac, unsafe {
                sid_dac_default(i32::from(model == SID_MODEL_6581))
            });
            assert_eq!((s.thump, s.bleed), (0.0, 0.0));
            assert_eq!(s.output_gain, plain.output_gain);
            write(&mut s, &CHORD);
            assert_eq!(render(&mut s, 20000), want, "model {model}");
        }
    }
}

/// Each profile at zero character is its settings: its filter row, its combined waveforms,
/// its DACs (built only where they are not the model's default) and its non-linearity.
#[test]
fn each_profile_at_zero_character_is_its_settings() {
    for id in 0..SID_CHIPS {
        let p = unsafe { &*sid_chip_profile_get(id) };
        let model = p.model as sid_model;
        let mut dac = Box::<sid_dac>::default();
        let mut s = chip(model);
        unsafe { sid_character_apply(&mut *s, &character(id, 0, 0, 0, 0), &mut *dac) };
        let mut row = chip(model);
        unsafe { sid_set_filter_curve(&mut *row, u32::from(p.curve)) };
        assert_eq!(s.cutoff_g, row.cutoff_g, "profile {id}");
        assert_eq!(s.res_k, row.res_k, "profile {id}");
        assert_eq!(s.filter_curve, p.curve);
        let mut combined = chip(model);
        unsafe { sid_set_combined_profile(&mut *combined, i32::from(p.combined)) };
        assert_eq!(s.osc[0].pulldown, combined.osc[0].pulldown, "profile {id}");
        let mut nl = chip(model);
        unsafe {
            sid_set_filter_curve(&mut *nl, u32::from(p.curve)); // the floor follows it
            sid_set_filter_nonlinearity(&mut *nl, p.nonlinearity);
        }
        assert_eq!(format!("{:?}", s.nl), format!("{:?}", nl.nl));
        let is6581 = i32::from(model == SID_MODEL_6581);
        if id == SID_CHIP_6581_WORN {
            let mut want = Box::<sid_dac>::default();
            unsafe { sid_dac_build(&mut *want, is6581, p.kink, p.leakage) };
            assert_eq!(s.dac, &*dac as *const sid_dac);
            assert_eq!(dac.wave, want.wave);
            assert_eq!(dac.env, want.env);
        } else {
            assert_eq!(s.dac, unsafe { sid_dac_default(is6581) }, "profile {id}");
        }
    }
}

/// Renders the chord with a character, after a settling time.
fn sound(c: &sid_character, n: usize) -> Vec<i16> {
    let mut dac = Box::<sid_dac>::default();
    let mut s = with_character(c, &mut dac);
    render(&mut s, 4410);
    render(&mut s, n)
}

/// Age/heat: the filter drifts darker (its cutoff falls at every step of the control, and the
/// chord's brightness with it, measured at five) and the 6581's non-linearity rises.
#[test]
fn age_darkens_the_filter_and_raises_the_non_linearity() {
    for profile in 0..SID_CHIPS {
        let model = model_of(profile);
        let mut last: Option<(f32, f32)> = None;
        for age in 0..=127u8 {
            let mut dac = Box::<sid_dac>::default();
            let s = with_character(&character(profile, age, 0, 0, 0), &mut dac);
            let g = s.svf.g; // the chord's FC 1024, blended
            let level = s.nl.level;
            if let Some((g0, l0)) = last {
                assert!(
                    g < g0,
                    "profile {profile} age {age}: the cutoff did not fall"
                );
                if model == SID_MODEL_6581 {
                    assert!(level >= l0, "profile {profile} age {age}");
                }
            }
            last = Some((g, level));
        }
        let mut last = None;
        for age in [0u8, 32, 64, 96, 127] {
            let b = brightness(&sound(&character(profile, age, 0, 0, 0), 8820));
            println!("profile {profile} age {age}: brightness {b:.4}");
            if let Some(b0) = last {
                assert!(b < b0, "profile {profile} age {age}: no darker");
            }
            last = Some(b);
        }
        if model == SID_MODEL_6581 {
            let mut dac = Box::<sid_dac>::default();
            let young = with_character(&character(profile, 0, 0, 0, 0), &mut dac);
            let old = with_character(&character(profile, 127, 0, 0, 0), &mut dac);
            assert!(old.nl.level > young.nl.level, "profile {profile}");
        }
    }
}

/// DAC sloppiness: a leakier, more kinked DAC. Measured where it shows most: a voice whose
/// envelope has released to 0 still leaks its waveform through the envelope DAC, more with
/// more leakage.
#[test]
fn dac_sloppiness_leaks_more_of_a_released_voice() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5, SID_CHIP_6581_WORN] {
        let mut last = None;
        for dac_amount in 0..=127u8 {
            let mut dac = Box::<sid_dac>::default();
            let c = character(profile, 0, dac_amount, 0, 0);
            let mut s = with_character(&c, &mut dac);
            // Release every voice (release 0: 6 ms), then listen.
            write(&mut s, &[(0x04, 0x20), (0x0b, 0x20), (0x12, 0x40)]);
            render(&mut s, 4410);
            let leak = rms(&render(&mut s, 8820));
            if dac_amount % 42 == 0 || dac_amount == 127 {
                println!("profile {profile} DAC {dac_amount}: released voices at {leak:.2} rms");
            }
            if let Some(l) = last {
                assert!(
                    leak > l,
                    "profile {profile} DAC {dac_amount}: no more leakage"
                );
            }
            last = Some(leak);
        }
    }
}

/// Output thump: a volume write steps the output, more with more thump, at every step of the
/// control; at 0 the 8580 (no voice DC: only its silent voices' DAC offset times the envelope
/// DAC's leakage) barely steps. Measured in silence after the DC blocker: the step's signed
/// area (the 6581's own step at 0 points the other way, so its size first falls), and its
/// peak.
#[test]
fn thump_steps_the_output_at_a_volume_write() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5] {
        let mut last = None;
        for thump in 0..=127u8 {
            let mut dac = Box::<sid_dac>::default();
            let mut s = chip(model_of(profile));
            unsafe { sid_character_apply(&mut *s, &character(profile, 0, 0, thump, 0), &mut *dac) };
            write(&mut s, &[(0x18, 0x00)]);
            render(&mut s, 44100); // the power-on release and the DC blocker settle
            write(&mut s, &[(0x18, 0x0f)]);
            let after = render(&mut s, 2205);
            let area: i64 = after.iter().map(|&v| i64::from(v)).sum();
            let step = after.iter().map(|&v| i32::from(v).abs()).max().unwrap();
            if thump % 42 == 0 || thump == 127 {
                println!("profile {profile} thump {thump}: step {step}, area {area}");
            }
            if let Some(l) = last {
                assert!(
                    area > l,
                    "profile {profile} thump {thump}: {area} after {l}"
                );
            } else if profile == SID_CHIP_8580_R5 {
                assert!(step < 100, "the 8580 has hardly a step of its own: {step}");
            }
            last = Some(area);
        }
    }
}

/// The thump's headroom: the gain falls by as much as the step, so a signal that just reached
/// full scale, plus the largest step (volume 0 to 15), still does.
#[test]
fn thump_keeps_its_step_inside_full_scale() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let mut dac = Box::<sid_dac>::default();
        let profile = unsafe { sid_chip_default(model) };
        let mut s = chip(model);
        let base = s.output_gain;
        unsafe { sid_character_apply(&mut *s, &character(profile, 0, 0, 127, 0), &mut *dac) };
        let full_scale = 32767.0 / base; // in mix units, before the thump
        let with_step = (full_scale + s.thump) * s.output_gain;
        assert!((with_step - 32767.0).abs() < 0.05, "{with_step}");
        assert_eq!(s.thump, SID_CHARACTER_THUMP as f32);
    }
}

/// Filter bleed: with no voice routed into the filter, the unfiltered voices still reach it,
/// more with more bleed: the band-pass's resonance at 400 Hz, which a saw an octave and a
/// half above does not have, grows.
#[test]
fn bleed_feeds_unfiltered_voices_into_the_filter() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5] {
        let render_with = |bleed: u8| {
            let mut dac = Box::<sid_dac>::default();
            let mut s = chip(model_of(profile));
            unsafe { sid_character_apply(&mut *s, &character(profile, 0, 0, 0, bleed), &mut *dac) };
            write(&mut s, &CHORD);
            // Nothing routed: the band-pass alone, at full resonance.
            write(&mut s, &[(0x17, 0xf0), (0x18, 0x2f)]);
            render(&mut s, 4410);
            render(&mut s, 8820)
        };
        let dry = render_with(0);
        let mut last = 0.0;
        for bleed in 1..=127u8 {
            let wet = render_with(bleed);
            let diff: Vec<i16> = wet
                .iter()
                .zip(&dry)
                .map(|(a, b)| a.wrapping_sub(*b))
                .collect();
            let added = rms(&diff);
            if bleed % 42 == 0 || bleed == 127 {
                println!("profile {profile} bleed {bleed}: the filter adds {added:.1} rms");
            }
            assert!(added > last, "profile {profile} bleed {bleed}");
            last = added;
        }
    }
}

/// Settings out of range clamp: a control past 127 is 127, a profile past the last or of the
/// other model is the chip's model's default.
#[test]
fn the_settings_clamp() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let profile = unsafe { sid_chip_default(model) };
        let mut a = Box::<sid_dac>::default();
        let mut b = Box::<sid_dac>::default();
        let mut s = chip(model);
        let mut t = chip(model);
        unsafe {
            sid_character_apply(&mut *s, &character(profile, 255, 200, 128, 250), &mut *a);
            sid_character_apply(&mut *t, &character(profile, 127, 127, 127, 127), &mut *b);
        }
        assert_eq!(s.cutoff_g, t.cutoff_g);
        assert_eq!(a.wave, b.wave);
        assert_eq!(
            (s.thump, s.bleed, s.output_gain),
            (t.thump, t.bleed, t.output_gain)
        );
        let other = if model == SID_MODEL_6581 {
            SID_CHIP_8580_R5
        } else {
            SID_CHIP_6581_WORN
        };
        for wrong in [
            sid_character {
                profile: 200,
                ..Default::default()
            },
            character(other, 0, 0, 0, 0),
        ] {
            let mut u = chip(model);
            unsafe { sid_character_apply(&mut *u, &wrong, &mut *a) };
            let plain = chip(model);
            assert_eq!(u.cutoff_g, plain.cutoff_g);
            assert_eq!(u.dac, plain.dac);
        }
    }
}

/// The live instrument's tick (LIVE_TICK_HZ) in samples at the test's rate.
const TICK: usize = (FS / 100) as usize;
/// The ticks rendered before a jump, and after it: the longest ramp's (the DACs': 43 tables,
/// each over SID_DAC_BUILD_STEPS ticks) and some.
const BEFORE: usize = 30;
const AFTER: usize = 190;
/// The click test's reference ramps the control over 0.2 s (TESTING.md, the effect slots'
/// click tests), one step of 127 at a time.
const RAMP: usize = FS as usize / 5;
/// The residual's bound, and a step it must see (the effect slots' RESIDUAL_MAX and CLICK).
const RESIDUAL_MAX: i32 = 30;
const CLICK: i32 = 100;

/// One voice of triangle at 110 Hz (frequency 0x0751 at PAL), held at full sustain, and the
/// filter as a control needs it to be heard: age/heat moves the cutoff of the voice routed
/// through the low-pass at resonance 15, at FC 1536 (lower, where the coefficients' step is
/// small, its jump at once measures 10 to 20 LSB: no click to see); the bleed sends the
/// unrouted voice into the high-pass at FC 384; the DACs and the thump need no filter.
fn triangle(control: usize) -> Vec<(u8, u8)> {
    let (route, mode) = match control {
        0 => (0xf1, 0x1f), // resonance 15, voice 1 into the low-pass
        3 => (0xf0, 0x4f), // nothing routed, the high-pass
        _ => (0x00, 0x0f), // no filter
    };
    vec![
        (0x15, 0x00),
        (0x16, if control == 0 { 0xc0 } else { 0x30 }), // FC 1536 for age/heat, else 384
        (0x17, route),
        (0x18, mode),
        (0x00, 0x51),
        (0x01, 0x07),
        (0x05, 0x00),
        (0x06, 0xf0),
        (0x04, 0x11),
    ]
}

/// How a run takes control from 0 to 127.
#[derive(Clone, Copy)]
enum Jump<'a> {
    /// Sent to the host's sink, which ramps it a tick at a time (sid_character_ramp_tick).
    Ramped,
    /// The same, applied at once (sid_character_change, as the sinks did before the ramp).
    AtOnce,
    /// The reference: the control at value(sample) at every sample, between the settings'
    /// stages (and DAC tables, every 16 samples) by linear interpolation.
    Smooth(&'a dyn Fn(usize) -> f64),
}

/// The character with `control` at v.
fn with(profile: u32, control: usize, v: u8) -> sid_character {
    let mut c = character(profile, 0, 0, 0, 0);
    match control {
        0 => c.age = v,
        1 => c.dac = v,
        2 => c.thump = v,
        _ => c.bleed = v,
    }
    c
}

/// a + t (b - a), field by field.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// A chip behind the host's sink (live_sid_sink), driven as the live instrument drives it:
/// the profile and a reset, the triangle, then LIVE_REG_CHAR_TICK every tick, with control
/// taken from 0 to 127 from tick `at` as `how` says. The output from a tick before, and the
/// sink's ramp's value of the control at each tick.
fn jump(profile: u32, control: usize, how: Jump, at: usize, ticks: usize) -> (Vec<i16>, Vec<u8>) {
    let model = model_of(profile);
    let mut chip = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut target = Box::new(live_sid_target {
        chip: &mut *chip,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: FS,
        dacs: dacs.as_mut_ptr(),
        ..Default::default()
    });
    let t: *mut live_sid_target = &mut *target;
    let sink = move |reg: u32, value: u8| unsafe { live_sid_sink(t.cast(), reg as u8, value) };
    sink(LIVE_REG_CHAR_FIRST, profile as u8);
    sink(LIVE_REG_RESET, model as u8);
    for (r, v) in triangle(control) {
        sink(u32::from(r), v);
    }
    // The reference's stages and DAC tables at each setting, made as they are needed.
    let mut stages: Vec<Option<sid_character_stage>> = vec![None; 128];
    let mut tables: Vec<Option<Box<sid_dac>>> = (0..128).map(|_| None).collect();
    let mut between = Box::<sid_dac>::default();
    let mut stage = |chip: &sid, v: usize, tables: &mut Vec<Option<Box<sid_dac>>>| {
        *stages[v].get_or_insert_with(|| {
            let c = with(profile, control, v as u8);
            let mut st = sid_character_stage::default();
            unsafe {
                let d: *const sid_dac = if sid_character_dac_custom(chip, &c) != 0 {
                    let d = tables[v].get_or_insert_with(Box::default);
                    sid_character_build_dac(chip, &c, &mut **d);
                    &**d
                } else {
                    sid_dac_default(i32::from(model == SID_MODEL_6581))
                };
                sid_character_stage_for(chip, &c, d, &mut st);
            }
            st
        })
    };
    let mut out = Vec::new();
    let mut now = Vec::new();
    for tick in 0..ticks {
        if tick == at && !matches!(how, Jump::Smooth(_)) {
            sink(LIVE_REG_CHAR_FIRST + 1 + control as u32, 127);
            if matches!(how, Jump::AtOnce) {
                let c = unsafe { (*t).ramp.target };
                let table = unsafe { &mut *dacs.as_mut_ptr().add(1) };
                unsafe { sid_character_change(&mut *chip, &c, table) };
                unsafe { sid_character_ramp_reset(&mut (*t).ramp, &c, chip.dac) };
            }
        }
        sink(LIVE_REG_CHAR_TICK, 0);
        let n = unsafe { (*t).ramp.now };
        now.push([n.age, n.dac, n.thump, n.bleed][control]);
        let Jump::Smooth(value) = how else {
            out.extend(render(&mut chip, TICK));
            continue;
        };
        for i in tick * TICK..(tick + 1) * TICK {
            let v = value(i).clamp(0.0, 127.0);
            if v > 0.0 && (control != 1 || i % 16 == 0) {
                let lo = v.floor() as usize;
                let f = (v - lo as f64) as f32;
                let a = stage(&chip, lo, &mut tables);
                let b = if f > 0.0 {
                    stage(&chip, lo + 1, &mut tables)
                } else {
                    a
                };
                let mut st = a;
                st.nl.level = lerp(a.nl.level, b.nl.level, f);
                st.nl.level_off = lerp(a.nl.level_off, b.nl.level_off, f);
                st.nl.load = lerp(a.nl.load, b.nl.load, f);
                st.nl.floor = lerp(a.nl.floor, b.nl.floor, f);
                st.thump = lerp(a.thump, b.thump, f);
                st.bleed = lerp(a.bleed, b.bleed, f);
                st.output_gain = lerp(a.output_gain, b.output_gain, f);
                st.filter_t = lerp(a.filter_t, b.filter_t, f);
                if a.dac != b.dac {
                    let (da, db) = unsafe { (&*a.dac, &*b.dac) };
                    for (x, (p, q)) in between.wave.iter_mut().zip(da.wave.iter().zip(&db.wave)) {
                        *x = lerp(*p, *q, f);
                    }
                    for (x, (p, q)) in between.env.iter_mut().zip(da.env.iter().zip(&db.env)) {
                        *x = lerp(*p, *q, f);
                    }
                    st.dac = &*between;
                }
                unsafe { sid_character_commit(&mut *chip, &st) };
            }
            out.extend(render(&mut chip, 1));
        }
    }
    let mut there = sid_character_stage::default();
    unsafe { sid_character_stage_for(&*chip, &with(profile, control, 127), chip.dac, &mut there) };
    let s = &*chip;
    assert_eq!(
        (s.filter_t, s.thump, s.bleed, s.output_gain),
        (there.filter_t, there.thump, there.bleed, there.output_gain),
        "profile {profile} control {control}: the jump did not get there"
    );
    if control == 1 {
        let mut d = Box::<sid_dac>::default();
        unsafe { sid_character_build_dac(s, &with(profile, 1, 127), &mut *d) };
        assert_eq!(
            unsafe { (*s.dac).wave },
            d.wave,
            "profile {profile}: the DACs"
        );
    }
    (out[(at - 1) * TICK..].to_vec(), now)
}

/// The ramp's path at its speed, smoothly: from the sink's value of the control at each tick,
/// the line through the middle of each of its risers (from half a step before the first to
/// half a step after the last), in samples.
fn path(now: &[u8]) -> impl Fn(usize) -> f64 {
    let risers: Vec<usize> = (1..now.len()).filter(|&i| now[i] != now[i - 1]).collect();
    let mut points: Vec<(f64, f64)> = risers
        .iter()
        .map(|&i| {
            (
                (i * TICK) as f64,
                (f64::from(now[i]) + f64::from(now[i - 1])) / 2.0,
            )
        })
        .collect();
    let (first, last) = (risers[0], risers[risers.len() - 1]);
    let half = |a: usize, b: usize| (b - a) as f64 * TICK as f64 / 2.0;
    let (h0, h1) = match risers.len() {
        1 => (TICK as f64 / 2.0, TICK as f64 / 2.0),
        n => (
            half(risers[0], risers[1]),
            half(risers[n - 2], risers[n - 1]),
        ),
    };
    points.insert(0, (points[0].0 - h0, f64::from(now[first - 1])));
    points.push((points[points.len() - 1].0 + h1, f64::from(now[last])));
    move |s: usize| {
        let x = s as f64;
        let k = points.partition_point(|p| p.0 <= x);
        if k == 0 {
            return points[0].1;
        }
        if k == points.len() {
            return points[k - 1].1;
        }
        let ((x0, y0), (x1, y1)) = (points[k - 1], points[k]);
        y0 + (y1 - y0) * (x - x0) / (x1 - x0)
    }
}

/// The ticks from the first change of a ramp's value to its last.
fn ramp_ticks(now: &[u8]) -> usize {
    let changes: Vec<usize> = (1..now.len()).filter(|&i| now[i] != now[i - 1]).collect();
    changes[changes.len() - 1] - changes[0] + 1
}

/// The largest 2nd difference of a - b: a step of s in either shows as s.
fn residual(a: &[i16], b: &[i16]) -> i32 {
    let d: Vec<i32> = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| i32::from(x) - i32::from(y))
        .collect();
    d.windows(3)
        .map(|w| (w[2] - 2 * w[1] + w[0]).abs())
        .max()
        .unwrap_or(0)
}

/// A jump of any control from 0 to 127 does not click, by the effect slots' measure
/// (TESTING.md): the run through the sink's ramp against the same move taken a step of 1 at a
/// time over 0.2 s keeps the 2nd difference of their difference within RESIDUAL_MAX, on a
/// triangle the jump meets at ten phases. The measure would see a CLICK-sized step, and sees
/// the same jump at once for every control: age at resonance 15, the DACs, the thump (the 8580's
/// too) and the bleed.
#[test]
fn a_jump_of_any_control_is_ramped_and_does_not_click() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5, SID_CHIP_6581_WORN] {
        for control in 0..4usize {
            let (mut ramped, mut at_once, mut click, mut long) = (0, 0, i32::MAX, usize::MAX);
            // At ten ticks in a row: the triangle (a period of 401 samples, a tick 441) meets
            // the jump at ten phases a tenth of its period apart, its peaks among them.
            for at in BEFORE..BEFORE + 10 {
                let ticks = at + AFTER;
                let (mut j, now) = jump(profile, control, Jump::Ramped, at, ticks);
                long = long.min(ramp_ticks(&now));
                let value = path(&now);
                let (r, _) = jump(profile, control, Jump::Smooth(&value), at, ticks);
                ramped = ramped.max(residual(&j, &r));
                let (a, _) = jump(profile, control, Jump::AtOnce, at, ticks);
                at_once = at_once.max(residual(&a, &r));
                for v in &mut j[TICK + TICK / 2..] {
                    *v = v.saturating_add(CLICK as i16);
                }
                click = click.min(residual(&j, &r));
            }
            println!(
                "profile {profile} control {control}: residual ramped {ramped} LSB, at once \
                 {at_once}, a {CLICK} LSB click {click}; the ramp {long} ticks"
            );
            assert!(
                ramped <= RESIDUAL_MAX,
                "profile {profile} control {control}: the ramped jump's residual is {ramped} LSB"
            );
            assert!(
                at_once > RESIDUAL_MAX,
                "profile {profile} control {control}: the test misses the jump at once ({at_once})"
            );
            assert!(
                click > RESIDUAL_MAX,
                "a {CLICK} LSB click shows only {click}"
            );
            assert!(
                long * TICK >= RAMP,
                "profile {profile} control {control}: a ramp of {long} ticks"
            );
        }
    }
}

/// sid_character_change, the host sink's apply at once, from every control at 127 back to 0:
/// the profile's own chip again. Age moves only the stage's filter_t (the tables, the
/// profile's curve and its hot row, stay), so the first row's tables are sid_init's
/// throughout; the non-linearity, the DACs and the output stage are sid_init's again.
#[test]
fn a_change_back_to_zero_restores_the_profile() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let profile = unsafe { sid_chip_default(model) };
        let mut dac = Box::<sid_dac>::default();
        let mut s = chip(model);
        unsafe {
            sid_character_change(&mut *s, &character(profile, 127, 127, 127, 127), &mut *dac);
            sid_character_change(&mut *s, &character(profile, 0, 0, 0, 0), &mut *dac);
        }
        let plain = chip(model);
        assert_eq!(s.cutoff_g, plain.cutoff_g);
        assert_eq!(s.res_k, plain.res_k);
        assert_eq!(format!("{:?}", s.nl), format!("{:?}", plain.nl));
        assert_eq!(s.dac, plain.dac);
        assert_eq!(
            (s.thump, s.bleed, s.output_gain),
            (0.0, 0.0, plain.output_gain)
        );
    }
}
