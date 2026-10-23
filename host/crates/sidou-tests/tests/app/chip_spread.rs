// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! CHIP SPREAD (`app/chip_spread.c`): equal-power gains, chip A to the left and chip B to the
//! right, the unity gain that converts as the slots do (their soft limiter included), and the
//! setting's MIDI row and clamp.

use sidou_sys::*;

const N: usize = CHIP_SPREAD_BLOCK as usize;

fn gains(spread: u32) -> (f32, f32) {
    let (mut m, mut s) = (0.0f32, 0.0f32);
    unsafe { chip_spread_gains(spread, &mut m, &mut s) };
    (m, s)
}

/// The slots' float output for int16 frames (sid_slots_process_float with the slots off).
fn floats(lr: &[i16]) -> Vec<f32> {
    lr.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

/// The stage on a block of the slots' float output, the gains held.
fn apply_f(lr: &[f32], side: &[i16], m: f32, k: f32) -> Vec<i16> {
    let mut out = vec![0i16; lr.len()];
    unsafe { chip_spread_apply(out.as_mut_ptr(), lr.as_ptr(), side.as_ptr(), m, m, k, k) };
    out
}

fn apply(lr: &[i16], side: &[i16], m: f32, k: f32) -> Vec<i16> {
    apply_f(&floats(lr), side, m, k)
}

fn side_of(a: &[i16], b: &[i16], fade: chip_mix_fade) -> Vec<i16> {
    let mut side = vec![0i16; N];
    unsafe { chip_spread_side(side.as_mut_ptr(), a.as_ptr(), b.as_ptr(), fade) };
    side
}

/// The mono block a and b make on both sides (the slots off: their output is their input; the
/// sums here fit int16).
fn mono(a: &[i16], b: &[i16]) -> Vec<i16> {
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

/// At unity (mid 1, side 0) the stage converts the slots' float output exactly as the slots
/// convert it themselves (sid_fx_to_i16_limited), past the limiter's knee too: the stage starts
/// and ends without a step. Every level from -2 to 2 of full scale in 1/65536 steps.
#[test]
fn unity_converts_as_the_slots_do() {
    let all: Vec<f32> = (-131_072..131_072).map(|v| v as f32 / 65_536.0).collect();
    let side = vec![12_345i16; N];
    for block in all.chunks(2 * N) {
        let out = apply_f(block, &side, 1.0, 0.0);
        for (&x, &y) in block.iter().zip(&out) {
            assert_eq!(y, unsafe { sid_fx_to_i16_limited(x) }, "{x}");
        }
    }
}

/// SPREAD 100 with the slots off: chip A alone is all on the left at sqrt 2 (+3 dB) and nothing
/// on the right; with chip B in use A is on the left and B on the right, each at sqrt 2 (to the
/// side's halving, 1 LSB). Under the knee nothing is limited.
#[test]
fn full_spread_puts_chip_a_left_and_chip_b_right() {
    let (m, k) = gains(CHIP_SPREAD_MAX);
    let a: Vec<i16> = (0..N as i32).map(|i| (i * 131 - 8_000) as i16).collect();
    let b: Vec<i16> = (0..N as i32).map(|i| (5_000 - i * 77) as i16).collect();
    let zero = vec![0i16; N];
    // A alone: the side is A itself.
    let out = apply(&mono(&a, &zero), &a, m, k);
    for i in 0..N {
        let want = f64::from(a[i]) * std::f64::consts::SQRT_2;
        assert!((f64::from(out[2 * i]) - want).abs() <= 0.5 + 1e-3, "L {i}");
        assert_eq!(out[2 * i + 1], 0, "R {i}");
    }
    // Both: the side halved, its gain doubled.
    let out = apply(&mono(&a, &b), &side_of(&a, &b, CHIP_MIX_FULL), m, 2.0 * k);
    for i in 0..N {
        let (l, r) = (f64::from(out[2 * i]), f64::from(out[2 * i + 1]));
        let sq = std::f64::consts::SQRT_2;
        assert!((l - f64::from(a[i]) * sq).abs() <= 1.5, "L {i}: {l}");
        assert!((r - f64::from(b[i]) * sq).abs() <= 1.5, "R {i}: {r}");
    }
}

/// At every SPREAD a chip's power over both sides is mono's (2 x its own): equal power.
#[test]
fn each_chip_keeps_its_power_at_every_spread() {
    let a: Vec<i16> = (0..N)
        .map(|i| ((i as f64 * 0.3).sin() * 12_000.0) as i16)
        .collect();
    let zero = vec![0i16; N];
    let power = |x: &[i16]| x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>();
    let mono_power = power(&mono(&a, &zero));
    for s in (0..=CHIP_SPREAD_MAX).step_by(10) {
        let (m, k) = gains(s);
        let p = power(&apply(&mono(&a, &zero), &a, m, k));
        assert!(
            (p / mono_power - 1.0).abs() < 1e-3,
            "{s}: {:.5}",
            p / mono_power
        );
    }
}

/// The side's fades are chip_mix's: during B's fade in or out the side is (A - B x the weight)
/// halved, so A + B and A - B agree on how much of B there is.
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

/// A gain change moves over the block: from mono to SPREAD 100 no sample steps by more than the
/// gains' 1/128 of the block allows.
#[test]
fn a_change_ramps_over_the_block() {
    let (m, k) = gains(CHIP_SPREAD_MAX);
    let a = vec![16_000i16; N];
    let lr = floats(&mono(&a, &vec![0i16; N]));
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

/// SPREAD as MIDI sets it: CC 117 by the steps curve (0..127 over 0..100), NRPN 0/4 to the
/// percent, clamped; saved with the settings (the dirty flag). Not a live parameter: learn and
/// MIDI out leave it alone.
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
