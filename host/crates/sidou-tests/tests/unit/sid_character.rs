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

/// Age/heat: the filter drifts darker (its cutoff table falls, and the chord's brightness
/// with it) and the 6581's non-linearity rises, step by step.
#[test]
fn age_darkens_the_filter_and_raises_the_non_linearity() {
    for profile in 0..SID_CHIPS {
        let model = model_of(profile);
        let mut last: Option<(f32, f64, f32)> = None;
        for age in [0u8, 32, 64, 96, 127] {
            let mut dac = Box::<sid_dac>::default();
            let s = with_character(&character(profile, age, 0, 0, 0), &mut dac);
            let g = s.cutoff_g[1024];
            let b = brightness(&sound(&character(profile, age, 0, 0, 0), 8820));
            let level = s.nl.level;
            println!("profile {profile} age {age}: g {g:.4}, brightness {b:.4}, nl {level:.5}");
            if let Some((g0, b0, l0)) = last {
                assert!(
                    g < g0,
                    "profile {profile} age {age}: the cutoff did not fall"
                );
                assert!(b < b0, "profile {profile} age {age}: no darker");
                if model == SID_MODEL_6581 {
                    assert!(level >= l0, "profile {profile} age {age}");
                }
            }
            last = Some((g, b, level));
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
        for dac_amount in [0u8, 42, 85, 127] {
            let mut dac = Box::<sid_dac>::default();
            let c = character(profile, 0, dac_amount, 0, 0);
            let mut s = with_character(&c, &mut dac);
            // Release every voice (release 0: 6 ms), then listen.
            write(&mut s, &[(0x04, 0x20), (0x0b, 0x20), (0x12, 0x40)]);
            render(&mut s, 4410);
            let leak = rms(&render(&mut s, 8820));
            println!("profile {profile} DAC {dac_amount}: released voices at {leak:.2} rms");
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

/// Output thump: a volume write steps the output, more with more thump; at 0 the 8580 (no
/// voice DC: only its silent voices' DAC offset times the envelope DAC's leakage) barely
/// steps. Measured in silence, the step's peak after the DC blocker.
#[test]
fn thump_steps_the_output_at_a_volume_write() {
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5] {
        let mut last = None;
        for thump in [0u8, 42, 85, 127] {
            let mut dac = Box::<sid_dac>::default();
            let mut s = chip(model_of(profile));
            unsafe { sid_character_apply(&mut *s, &character(profile, 0, 0, thump, 0), &mut *dac) };
            write(&mut s, &[(0x18, 0x00)]);
            render(&mut s, 44100); // the power-on release and the DC blocker settle
            write(&mut s, &[(0x18, 0x0f)]);
            let step = render(&mut s, 2205)
                .iter()
                .map(|&v| i32::from(v).abs())
                .max()
                .unwrap();
            println!("profile {profile} thump {thump}: step {step}");
            if let Some(l) = last {
                assert!(
                    step > l + 100,
                    "profile {profile} thump {thump}: {step} after {l}"
                );
            } else if profile == SID_CHIP_8580_R5 {
                assert!(step < 100, "the 8580 has hardly a step of its own: {step}");
            }
            last = Some(step);
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
        for bleed in [42u8, 85, 127] {
            let wet = render_with(bleed);
            let diff: Vec<i16> = wet
                .iter()
                .zip(&dry)
                .map(|(a, b)| a.wrapping_sub(*b))
                .collect();
            let added = rms(&diff);
            println!("profile {profile} bleed {bleed}: the filter adds {added:.1} rms");
            assert!(added > last + 10.0, "profile {profile} bleed {bleed}");
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

/// A control swept from 0 to 127 a step per block (128 samples), applied between blocks as
/// a sink does, never steps the chord's output by more than it steps held anywhere along
/// the way (every 16 values): no click. Each step is a blend of tables made outside the
/// render.
#[test]
fn sweeping_a_control_does_not_click() {
    const BLOCK: usize = 128;
    for profile in [SID_CHIP_6581_R4AR, SID_CHIP_8580_R5, SID_CHIP_6581_WORN] {
        for control in 0..4usize {
            let at = |v: u8| {
                let mut c = character(profile, 0, 0, 0, 0);
                match control {
                    0 => c.age = v,
                    1 => c.dac = v,
                    2 => c.thump = v,
                    _ => c.bleed = v,
                }
                c
            };
            let held = |v: u8| {
                let mut dac = Box::<sid_dac>::default();
                let mut s = with_character(&at(v), &mut dac);
                render(&mut s, 4410);
                max_step(&render(&mut s, 128 * BLOCK))
            };
            let still = (0..=8u8).map(|i| held((16 * i).min(127))).max().unwrap();
            let mut dac = Box::<sid_dac>::default();
            let mut s = with_character(&at(0), &mut dac);
            render(&mut s, 4410);
            let mut swept = Vec::new();
            for v in 0..=127u8 {
                unsafe { sid_character_change(&mut *s, &at(v), &mut *dac) };
                swept.extend(render(&mut s, BLOCK));
            }
            let sweep = max_step(&swept);
            println!("profile {profile} control {control}: {sweep} swept, {still} held");
            assert!(
                f64::from(sweep) <= 1.05 * f64::from(still),
                "profile {profile} control {control}: a step of {sweep} sweeping, {still} held"
            );
        }
    }
}

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
