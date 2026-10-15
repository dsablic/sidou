// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The chip's character (core/sid/sid_character.c): the profiles at zero character are their
//! settings exactly, each control moves the sound the way it says and measurably, a sweep of
//! any control does not click, and the settings clamp. The comparison with reSIDfp at each
//! profile's settings is tests/profiles.rs.

use sidou_sys::*;

const FS: u32 = 44100;

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

/// The largest sample-to-sample step.
fn max_step(x: &[i16]) -> i32 {
    x.windows(2)
        .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
        .max()
        .unwrap_or(0)
}

/// The energy above `hz` against the total: a first difference, a crude high-pass that is
/// enough to rank brightness.
fn brightness(x: &[i16]) -> f64 {
    let d: Vec<i16> = x.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();
    rms(&d) / rms(x)
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
/// The ticks rendered before a jump, and after it: the ramp's 16 and some.
const BEFORE: usize = 30;
const AFTER: usize = 30;

/// One voice of triangle at 110 Hz (frequency 0x0751 at PAL), held at full sustain, and the
/// filter as a control needs it to be heard: age/heat moves the cutoff of the voice routed
/// through the low-pass; the bleed sends the unrouted voice into the high-pass; the DACs and
/// the thump need no filter.
fn triangle(control: usize) -> Vec<(u8, u8)> {
    let (route, mode) = match control {
        0 => (0xf1, 0x1f), // resonance 15, voice 1 into the low-pass
        3 => (0xf0, 0x4f), // nothing routed, the high-pass
        _ => (0x00, 0x0f), // no filter
    };
    vec![
        (0x15, 0x00),
        (0x16, 0x30), // FC 384
        (0x17, route),
        (0x18, mode),
        (0x00, 0x51),
        (0x01, 0x07),
        (0x05, 0x00),
        (0x06, 0xf0),
        (0x04, 0x11),
    ]
}

/// A chip behind the host's sink (live_sid_sink), driven as the live instrument drives it:
/// the profile and a reset, the triangle, then LIVE_REG_CHAR_TICK every tick, with control
/// sent from 0 to `to` before tick JUMP_TICK. With `at_once` the jump skips the ramp
/// (sid_character_change at that tick, as the sinks did before the ramp). The output from a
/// tick before the jump.
fn jump(profile: u32, control: usize, to: u8, at_once: bool, at: usize) -> Vec<i16> {
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
    let mut out = Vec::new();
    for tick in 0..at + AFTER {
        if tick == at && to > 0 {
            sink(LIVE_REG_CHAR_FIRST + 1 + control as u32, to);
            if at_once {
                let c = unsafe { (*t).ramp.target };
                let table = unsafe { &mut *dacs.as_mut_ptr().add(1) };
                unsafe { sid_character_change(&mut *chip, &c, table) };
                unsafe { sid_character_ramp_reset(&mut (*t).ramp, &c, chip.dac) };
            }
        }
        sink(LIVE_REG_CHAR_TICK, 0);
        out.extend(render(&mut chip, TICK));
    }
    out[(at - 1) * TICK..].to_vec()
}

/// A jump of any control from 0 to 127 does not click: the ramp (sid_character_ramp_tick)
/// takes it in steps a tick apart (32 for age, thump and bleed, four tables for the DACs), so
/// the triangle's largest sample step stays within RAMP_STEP_BOUND of its own slope, held at
/// either end. The same jump at once (as the sinks made it before the ramp) steps past the
/// bound for the thump and the bleed, which change the output at once (the output gain, the
/// filter's input); age and the DACs change its shape, and the worn DACs step at once too.
#[test]
fn a_jump_of_any_control_is_ramped_and_does_not_click() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5, SID_CHIP_6581_WORN] {
        for control in 0..4usize {
            let held = |v: u8| {
                let mut c = character(profile, 0, 0, 0, 0);
                match control {
                    0 => c.age = v,
                    1 => c.dac = v,
                    2 => c.thump = v,
                    _ => c.bleed = v,
                }
                let mut dac = Box::<sid_dac>::default();
                let mut s = chip(model_of(profile));
                unsafe { sid_character_apply(&mut *s, &c, &mut *dac) };
                write(&mut s, &triangle(control));
                render(&mut s, 30 * TICK);
                max_step(&render(&mut s, 30 * TICK))
            };
            let slope = held(0).max(held(127));
            // At ten ticks in a row: the triangle (a period of 401 samples, a tick 441) meets
            // the jump at ten phases a tenth of its period apart, its peaks among them.
            let worst = |at_once: bool| {
                (BEFORE..BEFORE + 10)
                    .map(|at| max_step(&jump(profile, control, 127, at_once, at)))
                    .max()
                    .unwrap()
            };
            let (ramped, at_once) = (worst(false), worst(true));
            let bound = RAMP_STEP_BOUND * f64::from(slope);
            println!(
                "profile {profile} control {control}: ramped {ramped}, at once {at_once}, the \
                 triangle's own {slope} (bound {bound:.0})"
            );
            assert!(
                f64::from(ramped) <= bound,
                "profile {profile} control {control}: a step of {ramped} against {slope}"
            );
            if control >= 2 {
                assert!(
                    f64::from(at_once) > bound,
                    "profile {profile} control {control}: the test misses the jump at once"
                );
            }
        }
    }
}

/// How far the largest step of a ramped jump may exceed the signal's own (its slope).
const RAMP_STEP_BOUND: f64 = 1.25;

/// sid_character_change, the host sink's apply, rebuilds the tables whatever they were:
/// the age back to 0 is the profile's own tables again. (sid_character_apply, for a chip
/// fresh from sid_init, keeps sid_init's tables for the default profile at age 0.)
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
