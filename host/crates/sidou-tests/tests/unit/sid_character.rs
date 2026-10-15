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
