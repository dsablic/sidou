// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

const N: usize = CHIP_SPREAD_BLOCK as usize;

fn gains(spread: u32) -> (f32, f32) {
    let (mut m, mut s) = (0.0f32, 0.0f32);
    unsafe { chip_spread_gains(spread, &mut m, &mut s) };
    (m, s)
}

fn slots_float_output(lr: &[i16]) -> Vec<f32> {
    lr.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

fn apply_to_float(lr: &[f32], side: &[i16], m: f32, k: f32) -> Vec<i16> {
    let mut out = vec![0i16; lr.len()];
    unsafe { chip_spread_apply(out.as_mut_ptr(), lr.as_ptr(), side.as_ptr(), m, m, k, k) };
    out
}

fn apply(lr: &[i16], side: &[i16], m: f32, k: f32) -> Vec<i16> {
    apply_to_float(&slots_float_output(lr), side, m, k)
}

fn side_of(a: &[i16], b: &[i16], fade: chip_mix_fade) -> Vec<i16> {
    let mut side = vec![0i16; N];
    unsafe { chip_spread_side(side.as_mut_ptr(), a.as_ptr(), b.as_ptr(), fade) };
    side
}

fn mono_frames(a: &[i16], b: &[i16]) -> Vec<i16> {
    let mut m = a.to_vec();
    let mut over = vec![0i16; N];
    let past = unsafe {
        chip_mix(
            m.as_mut_ptr(),
            b.as_ptr(),
            over.as_mut_ptr(),
            N as u32,
            CHIP_MIX_FULL,
        )
    };
    assert_eq!(past, 0);
    m.iter().flat_map(|&v| [v, v]).collect()
}

#[test]
fn the_gains_are_equal_power_from_mono_to_a_quarter_turn() {
    assert_eq!(gains(0), (1.0, 0.0), "0 is mono exactly");
    let mut last = (1.0f32, 0.0f32);
    for s in 0..=CHIP_SPREAD_MAX {
        let (m, k) = gains(s);
        let phi = f64::from(s) / 100.0 * std::f64::consts::FRAC_PI_4;
        assert!((f64::from(m) - phi.cos()).abs() < 2e-7, "{s}: mid {m}");
        assert!((f64::from(k) - phi.sin()).abs() < 2e-7, "{s}: side {k}");
        assert!(m <= last.0 && k >= last.1, "{s}: monotonic");
        last = (m, k);
    }
    assert_eq!(
        gains(CHIP_SPREAD_MAX + 50),
        gains(CHIP_SPREAD_MAX),
        "clamped"
    );
}

#[test]
fn unity_converts_as_the_slots_do() {
    let all: Vec<f32> = (-131_072..131_072).map(|v| v as f32 / 65_536.0).collect();
    let side = vec![12_345i16; N];
    for block in all.chunks(2 * N) {
        let out = apply_to_float(block, &side, 1.0, 0.0);
        for (&x, &y) in block.iter().zip(&out) {
            assert_eq!(y, unsafe { sid_fx_to_i16_limited(x) }, "{x}");
        }
    }
}

#[test]
fn full_spread_puts_chip_a_left_and_chip_b_right() {
    let (m, k) = gains(CHIP_SPREAD_MAX);
    let a: Vec<i16> = (0..N as i32).map(|i| (i * 131 - 8_000) as i16).collect();
    let b: Vec<i16> = (0..N as i32).map(|i| (5_000 - i * 77) as i16).collect();
    let zero = vec![0i16; N];
    let out = apply(&mono_frames(&a, &zero), &a, m, k);
    for i in 0..N {
        let want = f64::from(a[i]) * std::f64::consts::SQRT_2;
        assert!((f64::from(out[2 * i]) - want).abs() <= 0.5 + 1e-3, "L {i}");
        assert_eq!(out[2 * i + 1], 0, "R {i}");
    }
    let out = apply(
        &mono_frames(&a, &b),
        &side_of(&a, &b, CHIP_MIX_FULL),
        m,
        2.0 * k,
    );
    for i in 0..N {
        let (l, r) = (f64::from(out[2 * i]), f64::from(out[2 * i + 1]));
        let sq = std::f64::consts::SQRT_2;
        assert!((l - f64::from(a[i]) * sq).abs() <= 1.5, "L {i}: {l}");
        assert!((r - f64::from(b[i]) * sq).abs() <= 1.5, "R {i}: {r}");
    }
}

#[test]
fn each_chip_keeps_its_power_at_every_spread() {
    let a: Vec<i16> = (0..N)
        .map(|i| ((i as f64 * 0.3).sin() * 12_000.0) as i16)
        .collect();
    let zero = vec![0i16; N];
    let power = |x: &[i16]| x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>();
    let mono_power = power(&mono_frames(&a, &zero));
    for s in (0..=CHIP_SPREAD_MAX).step_by(10) {
        let (m, k) = gains(s);
        let p = power(&apply(&mono_frames(&a, &zero), &a, m, k));
        assert!(
            (p / mono_power - 1.0).abs() < 1e-3,
            "{s}: {:.5}",
            p / mono_power
        );
    }
}

#[test]
fn the_side_fades_b_as_the_mix_does() {
    let a: Vec<i16> = (0..N as i32).map(|i| (i * 50 - 3_000) as i16).collect();
    let b = vec![20_000i16; N];
    for (fade, w) in [
        (
            CHIP_MIX_FULL,
            Box::new(|_: i32| 128) as Box<dyn Fn(i32) -> i32>,
        ),
        (CHIP_MIX_IN, Box::new(|i: i32| i + 1)),
        (CHIP_MIX_OUT, Box::new(|i: i32| 127 - i)),
    ] {
        let side = side_of(&a, &b, fade);
        for i in 0..N {
            let want = (i32::from(a[i]) - ((i32::from(b[i]) * w(i as i32)) >> 7)) >> 1;
            assert_eq!(i32::from(side[i]), want, "{fade} {i}");
        }
    }
}

#[test]
fn a_change_ramps_over_the_block() {
    let (m, k) = gains(CHIP_SPREAD_MAX);
    let a = vec![16_000i16; N];
    let lr = slots_float_output(&mono_frames(&a, &vec![0i16; N]));
    let mut out = vec![0i16; 2 * N];
    unsafe { chip_spread_apply(out.as_mut_ptr(), lr.as_ptr(), a.as_ptr(), 1.0, m, 0.0, k) };
    let left: Vec<i32> = out.iter().step_by(2).map(|&v| i32::from(v)).collect();
    let right: Vec<i32> = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|&v| i32::from(v))
        .collect();
    let most = |x: &[i32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap();
    println!(
        "ramp: left {} .. {}, right {} .. {}",
        left[0],
        left[N - 1],
        right[0],
        right[N - 1]
    );
    assert!(most(&left) <= 60 && most(&right) <= 140);
    assert!((left[N - 1] - 22_627).abs() <= 1 && right[N - 1] == 0);
}

fn glide_step(g: &mut chip_spread_glide, a: u8, b: Option<u8>) -> u32 {
    let level = |x: u8| {
        let mut c = Box::<sid>::default();
        for v in 0..3 {
            c.env[v].counter = x;
        }
        unsafe { chip_spread_level(&*c) }
    };
    unsafe { chip_spread_glide_step(g, level(a), b.map_or(0, level)) }
}

#[test]
fn spread_separates_only_while_both_chips_sound() {
    for (a, b, what) in [
        (200, None, "A alone, B out of use"),
        (200, Some(0), "A alone, B quiet"),
        (0, Some(200), "B alone"),
    ] {
        let mut g = chip_spread_glide::default();
        for _ in 0..100 {
            assert_eq!(glide_step(&mut g, a, b), 0, "{what}");
        }
    }
    let mut g = chip_spread_glide::default();
    let glide = CHIP_SPREAD_GLIDE;
    let secs = f64::from(glide * CHIP_SPREAD_BLOCK) / 44_118.0;
    assert!((0.08..=0.12).contains(&secs), "the glide takes {secs:.3} s");
    for i in 1..=glide + 10 {
        assert_eq!(
            glide_step(&mut g, 200, Some(150)),
            i.min(glide),
            "in, block {i}"
        );
    }
    for i in 1..=CHIP_SPREAD_HOLD - 1 {
        assert_eq!(glide_step(&mut g, 200, Some(0)), glide, "held, block {i}");
    }
    for i in 1..=glide + 10 {
        assert_eq!(
            glide_step(&mut g, 200, Some(0)),
            glide.saturating_sub(i),
            "out, block {i}"
        );
    }
}

#[test]
fn the_sounding_test_does_not_flutter() {
    let between = (CHIP_SPREAD_OFF + CHIP_SPREAD_ON) as u8 / 2;
    let mut g = chip_spread_glide::default();
    for _ in 0..50 {
        assert_eq!(
            glide_step(&mut g, 200, Some(between)),
            0,
            "B quiet to begin with"
        );
    }
    for _ in 0..CHIP_SPREAD_GLIDE {
        glide_step(&mut g, 200, Some(200));
    }
    for _ in 0..200 {
        assert_eq!(
            glide_step(&mut g, 200, Some(between)),
            CHIP_SPREAD_GLIDE,
            "B decays to between: still sounding"
        );
    }
    for _ in 0..20 {
        for _ in 0..CHIP_SPREAD_HOLD - 1 {
            assert_eq!(glide_step(&mut g, 200, Some(0)), CHIP_SPREAD_GLIDE);
        }
        assert_eq!(glide_step(&mut g, 200, Some(200)), CHIP_SPREAD_GLIDE);
    }
    let mut c = Box::<sid>::default();
    c.env[2].counter = CHIP_SPREAD_ON as u8;
    assert_eq!(unsafe { chip_spread_level(&*c) }, CHIP_SPREAD_ON);
}

#[test]
fn the_glide_moves_the_gains_without_a_step() {
    let (m, k) = gains(CHIP_SPREAD_MAX);
    let a = vec![16_000i16; N];
    let b = vec![-16_000i16; N];
    let silent_mid = vec![0.0f32; 2 * N];
    let side = side_of(&a, &b, CHIP_MIX_FULL);
    let mut g = chip_spread_glide::default();
    let (mut m_now, mut k_now) = (1.0f32, 0.0f32);
    let mut left = Vec::new();
    for _ in 0..2 * CHIP_SPREAD_GLIDE {
        let w = glide_step(&mut g, 200, Some(200)) as f32 * (1.0 / CHIP_SPREAD_GLIDE as f32);
        let (m_to, k_to) = (1.0 + w * (m - 1.0), w * k);
        let mut out = vec![0i16; 2 * N];
        unsafe {
            chip_spread_apply(
                out.as_mut_ptr(),
                silent_mid.as_ptr(),
                side.as_ptr(),
                m_now,
                m_to,
                2.0 * k_now,
                2.0 * k_to,
            )
        };
        (m_now, k_now) = (m_to, k_to);
        left.extend(out.iter().step_by(2).map(|&v| i32::from(v)));
    }
    let most = left.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap();
    println!(
        "glide: left {} .. {}, the largest step {most} LSB",
        left[0],
        left[left.len() - 1]
    );
    assert!(most <= 6, "a step of {most} LSB");
    assert!(left[0].abs() <= most, "from the centre");
    assert!((left[left.len() - 1] - 22_627).abs() <= 1);
}

#[test]
fn the_cc_and_the_nrpn_set_spread() {
    let row = unsafe { &*midi_map_for_cc(MIDI_MAP_CC_SPREAD as u8) };
    assert_eq!(u32::from(row.param), MIDI_MAP_SPREAD);
    assert_eq!(row.nrpn, 4, "NRPN 0/4");
    let mut rig = crate::Rig::new(SID_MODEL_6581);
    let l = &mut *rig.live;
    let mut port = Box::<midi_map_port>::default();
    unsafe { midi_map_port_init(&mut *port, l) };
    for (v, want) in [(127u8, 100u8), (64, 50), (0, 0), (13, 10)] {
        l.settings_dirty = 0;
        unsafe { midi_map_cc(&mut *port, 0, MIDI_MAP_CC_SPREAD as u8, v) };
        assert_eq!(l.spread, want, "CC {v}");
        assert_eq!(l.settings_dirty, 1, "CC {v}: saved");
    }
    for (cc, v) in [(99u8, 0u8), (98, 4), (6, 0), (38, 37)] {
        unsafe { midi_map_cc(&mut *port, 3, cc, v) };
    }
    assert_eq!(l.spread, 37, "NRPN 0/4 = 37");
    unsafe { midi_map_cc(&mut *port, 3, 96, 0) };
    assert_eq!(l.spread, 38, "increment");
    unsafe { midi_map_cc(&mut *port, 3, 38, 127) };
    assert_eq!(l.spread, 100, "clamped");
    assert_eq!(
        unsafe { midi_learn_param(MIDI_MAP_SPREAD) },
        0,
        "not learnable"
    );
}
