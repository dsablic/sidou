// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::{difference, max_second_diff, sine, xorshift_noise, FIRMWARE_BLOCK, FS, FSF, RAMP};
use crate::{CLICK, RESIDUAL_MAX, T_END, T_OFF, T_ON};

pub const DIST: u32 = SID_SLOT_DIST;
pub const MOD: u32 = SID_SLOT_MOD;
pub const FX: u32 = SID_SLOT_FX;

const POOL_GARBAGE: u16 = 0x5a5a;
const LOUDEST_TWO_CHIPS_IN_PHASE: f64 = 29_990.0;

pub struct Slots {
    pub s: Box<sid_slots>,
    pub pool: Box<sid_slot_pool>,
}

impl Slots {
    pub fn new(seed: u32) -> Self {
        let mut pool = Box::<sid_slot_pool>::default();
        unsafe { pool.s16.fill(POOL_GARBAGE as i16) };
        let mut s = Box::<sid_slots>::default();
        unsafe { sid_slots_init(&mut *s, FS, seed, &mut *pool) };
        Self { s, pool }
    }

    pub fn set(&mut self, slot: u32, p: sid_slot_params) {
        unsafe { sid_slots_set(&mut *self.s, slot, &p) }
    }

    pub fn process(&mut self, input: &[i16]) -> Vec<i16> {
        let mut out = vec![0i16; 2 * input.len()];
        let n = u32::try_from(input.len()).expect("block fits in u32");
        unsafe { sid_slots_process(&mut *self.s, input.as_ptr(), out.as_mut_ptr(), n) };
        out
    }

    pub fn run_blocks(&mut self, input: &[i16], block: usize) -> Vec<i16> {
        input.chunks(block).flat_map(|b| self.process(b)).collect()
    }

    pub fn run(&mut self, input: &[i16]) -> Vec<i16> {
        self.run_blocks(input, FIRMWARE_BLOCK)
    }

    pub fn bytes(&self) -> Vec<u8> {
        let p = (&*self.s as *const sid_slots).cast::<u8>();
        let mut b = unsafe { std::slice::from_raw_parts(p, size_of::<sid_slots>()) }.to_vec();
        b.extend(
            unsafe { self.pool.s16 }
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        b
    }
}

pub fn params(algorithm: u32, amount: f32, a: f32, b: f32, mix: f32) -> sid_slot_params {
    sid_slot_params {
        algorithm,
        amount,
        a,
        b,
        mix,
    }
}

pub fn off() -> sid_slot_params {
    params(0, 0.0, 0.0, 0.0, 0.0)
}

pub fn left(x: &[i16]) -> Vec<i16> {
    x.iter().step_by(2).copied().collect()
}

pub fn right(x: &[i16]) -> Vec<i16> {
    x.iter().skip(1).step_by(2).copied().collect()
}

fn max_second_diff_per_channel(interleaved: &[i32]) -> i32 {
    (0..2)
        .map(|c| {
            let channel: Vec<i32> = interleaved.iter().skip(c).step_by(2).copied().collect();
            max_second_diff(&channel)
        })
        .max()
        .unwrap()
}

pub fn max_second_diff_stereo(x: &[i16]) -> i32 {
    max_second_diff_per_channel(&x.iter().map(|&v| i32::from(v)).collect::<Vec<_>>())
}

pub fn max_second_diff_of_difference_stereo(a: &[i16], b: &[i16]) -> i32 {
    max_second_diff_per_channel(&difference(a, b))
}

#[derive(Clone, Copy, Debug)]
pub enum Control {
    Amount,
    A,
    B,
    Mix,
}

pub fn slot_residual(
    slot: u32,
    base: sid_slot_params,
    control: Control,
    from: f32,
    to: f32,
    click: Option<usize>,
    amp: f64,
) -> i32 {
    let input = sine(110.0, amp, T_END);
    let with = |v: f32| {
        let mut p = base;
        match control {
            Control::Amount => p.amount = v,
            Control::A => p.a = v,
            Control::B => p.b = v,
            Control::Mix => p.mix = v,
        }
        p
    };
    let run = |ramp: bool| {
        let mut s = Slots::new(1);
        let mut out = Vec::with_capacity(2 * T_END);
        for (b, block) in input.chunks(16).enumerate() {
            let t = b * 16;
            let f = if ramp {
                if t < T_OFF {
                    (t.saturating_sub(T_ON) as f32 / RAMP as f32).min(1.0)
                } else {
                    1.0 - ((t - T_OFF) as f32 / RAMP as f32).min(1.0)
                }
            } else if (T_ON..T_OFF).contains(&t) {
                1.0
            } else {
                0.0
            };
            s.set(slot, with(from + f * (to - from)));
            out.extend(s.process(block));
        }
        out
    };
    let mut j = run(false);
    let r = run(true);
    if let Some(at) = click {
        for v in j.iter_mut().skip(2 * at).step_by(2) {
            *v = v.saturating_add(CLICK as i16);
        }
    }
    max_second_diff_of_difference_stereo(&j, &r)
}

pub fn assert_click_free(slot: u32, base: sid_slot_params, moves: &[(Control, f32, f32)]) {
    assert_click_free_at(0.5, slot, base, moves);
}

pub fn assert_click_free_at(
    amp: f64,
    slot: u32,
    base: sid_slot_params,
    moves: &[(Control, f32, f32)],
) {
    for &(control, from, to) in moves {
        let m = slot_residual(slot, base, control, from, to, None, amp);
        println!(
            "slot {slot} algorithm {}: {control:?} {from} -> {to}: residual {m} LSB",
            base.algorithm
        );
        assert!(
            m <= RESIDUAL_MAX,
            "slot {slot} algorithm {}: {control:?} residual {m} LSB",
            base.algorithm
        );
    }
    let (control, from, to) = moves[0];
    let c = slot_residual(slot, base, control, from, to, Some(T_ON + 10), amp);
    assert!(c > RESIDUAL_MAX, "a {CLICK} LSB click shows only {c}");
}

pub fn worst_second_diff_at_and_between_switches(
    slot: u32,
    algorithms: &[u32],
    amount: f32,
    a: f32,
    b: f32,
) -> (i32, i32) {
    let hold = FS as usize / 2;
    let input = sine(110.0, 0.5, hold * algorithms.len());
    let mut s = Slots::new(1);
    let mut out = Vec::new();
    for (k, &alg) in algorithms.iter().enumerate() {
        s.set(slot, params(alg, amount, a, b, 1.0));
        out.extend(s.run(&input[k * hold..(k + 1) * hold]));
    }
    let window = (0.06 * FSF) as usize;
    let (mut at_switch, mut steady) = (0, 0);
    for k in 0..algorithms.len() {
        let start = k * hold;
        let sw = &out[2 * start..2 * (start + window)];
        let st = &out[2 * (start + window)..2 * (start + hold - 4)];
        at_switch = at_switch.max(max_second_diff_stereo(sw));
        if k > 0 {
            steady = steady.max(max_second_diff_stereo(st));
        }
    }
    (at_switch, steady)
}

#[test]
fn all_off_is_exact_and_touches_nothing() {
    let mut s = Slots::new(1);
    for slot in [DIST, MOD, FX] {
        s.set(slot, off());
    }
    let before = s.bytes();
    let mut input = xorshift_noise(4096, 3);
    input.extend([i16::MIN, i16::MAX, 0, -1, 1]);
    let out = s.process(&input);
    assert_eq!(left(&out), input);
    assert_eq!(right(&out), input);
    assert_eq!(s.bytes(), before, "bypass changed the state");
}

#[test]
fn none_at_full_mix_is_the_identity_up_to_the_output_limiter() {
    let mut s = Slots::new(1);
    for slot in [DIST, MOD, FX] {
        s.set(slot, params(0, 1.0, 1.0, 1.0, 1.0));
    }
    let input = xorshift_noise(FS as usize, 5);
    let out = s.run(&input);
    let limited: Vec<i16> = input
        .iter()
        .map(|&v| unsafe { sid_fx_limit_i32(i32::from(v)) })
        .collect();
    assert!(limited.iter().zip(&input).any(|(a, b)| a != b));
    assert_eq!(left(&out), limited);
    assert_eq!(right(&out), limited);
}

#[test]
fn each_slot_returns_to_exact_bypass() {
    for (slot, alg) in [
        (DIST, SID_DIST_TAPE),
        (DIST, SID_DIST_LOFI),
        (MOD, SID_MOD_WOW),
    ] {
        let mut s = Slots::new(1);
        s.set(slot, params(alg, 1.0, 1.0, 1.0, 1.0));
        s.run(&sine(440.0, 0.5, FS as usize));
        s.set(slot, params(alg, 1.0, 1.0, 1.0, 0.0));
        s.run(&sine(440.0, 0.5, FS as usize));
        let before = s.bytes();
        let input = xorshift_noise(4096, 9);
        let out = s.run(&input);
        assert_eq!(left(&out), input, "slot {slot} algorithm {alg}");
        assert_eq!(right(&out), input);
        assert_eq!(
            s.bytes(),
            before,
            "slot {slot} algorithm {alg} changed state at rest"
        );
    }
}

#[test]
fn block_size_does_not_matter_even_after_rest() {
    let n = FS as usize;
    let input = sine(330.0, 0.6, 4 * n);
    let render = |block: usize| {
        let mut s = Slots::new(7);
        let mut out = Vec::new();
        for (k, part) in input.chunks(n).enumerate() {
            let mix = if k == 1 { 0.0 } else { 1.0 };
            s.set(DIST, params(SID_DIST_LOFI, 0.7, 0.5, 0.5, mix));
            s.set(MOD, params(SID_MOD_WOW, 1.0, 1.0, 0.0, mix));
            out.extend(s.run_blocks(part, block));
        }
        out
    };
    let reference = render(128);
    for block in [1, 77, 1000] {
        assert!(render(block) == reference, "block size {block} differs");
    }
}

#[test]
fn switching_algorithms_is_click_free_in_the_existing_blocks() {
    let (sw, steady) = worst_second_diff_at_and_between_switches(
        DIST,
        &[SID_DIST_TAPE, SID_DIST_LOFI, SID_DIST_NONE, SID_DIST_TAPE],
        0.8,
        0.0,
        0.0,
    );
    println!("dist switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady.max(0) + RESIDUAL_MAX, "a switch steps by {sw}");
    let (sw, steady) = worst_second_diff_at_and_between_switches(
        MOD,
        &[SID_MOD_WOW, SID_MOD_NONE, SID_MOD_WOW],
        1.0,
        1.0,
        0.0,
    );
    println!("mod switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady + RESIDUAL_MAX, "a switch steps by {sw}");
}

#[test]
fn parameter_jumps_are_click_free_in_the_existing_blocks() {
    use Control::*;
    assert_click_free(
        DIST,
        params(SID_DIST_TAPE, 1.0, 0.0, 0.0, 1.0),
        &[(Amount, 0.0, 1.0), (Mix, 0.0, 1.0)],
    );
    assert_click_free(
        DIST,
        params(SID_DIST_LOFI, 1.0, 1.0, 0.0, 1.0),
        &[(Amount, 0.0, 1.0), (A, 0.0, 1.0), (Mix, 0.0, 1.0)],
    );
    assert_click_free(
        MOD,
        params(SID_MOD_WOW, 1.0, 1.0, 0.0, 1.0),
        &[(Amount, 0.0, 1.0), (A, 0.0, 1.0), (Mix, 0.0, 1.0)],
    );
}

#[test]
fn full_feedback_reaches_the_output_limiter_but_never_a_rail() {
    let n = 3 * FS as usize;
    let input: Vec<i16> = (0..n)
        .map(|i| {
            let saw = |hz: f64| 2.0 * ((hz * i as f64 / FSF).fract()) - 1.0;
            (LOUDEST_TWO_CHIPS_IN_PHASE * (0.5 * saw(110.0) + 0.5 * saw(164.8))).round() as i16
        })
        .collect();
    for (slot, alg, name) in [
        (MOD, SID_MOD_FLANGER, "flanger"),
        (MOD, SID_MOD_PHASER, "phaser"),
        (MOD, SID_MOD_AUTOPAN, "auto-pan"),
        (FX, SID_FXSLOT_DELAY, "delay"),
        (FX, SID_FXSLOT_PINGPONG, "ping-pong"),
        (FX, SID_FXSLOT_REVERB_LIGHT, "reverb"),
        (FX, SID_FXSLOT_REVERB_LUSH, "lush"),
    ] {
        let mut s = Slots::new(1);
        s.set(slot, params(alg, 1.0, 1.0, 1.0, 1.0));
        let out = s.run(&input);
        let peak = out.iter().map(|&v| i32::from(v).abs()).max().unwrap();
        let rails = out
            .iter()
            .filter(|&&v| v == i16::MAX || v == i16::MIN)
            .count();
        let limited = out
            .iter()
            .filter(|&&v| i32::from(v).abs() > SID_FX_LIMIT_KNEE as i32)
            .count();
        println!("{name}: peak {peak}, {limited} samples above the knee, {rails} on a rail");
        assert_eq!(rails, 0, "{name}: {rails} samples on a rail");
        assert!(peak <= SID_FX_LIMIT_CEILING as i32, "{name}: peak {peak}");
        assert!(
            limited > 0,
            "{name} never reaches the limiter: no test of it"
        );
    }
}

#[test]
fn the_dry_gain_is_the_product_of_the_slots_dry_paths() {
    let dry = |s: &Slots| unsafe { sid_slots_dry(&*s.s) };
    let input = sine(110.0, 0.3, FS as usize / 2);
    let mut s = Slots::new(1);
    assert_eq!(dry(&s), 1.0);
    s.set(DIST, params(0, 1.0, 1.0, 1.0, 1.0));
    s.run(&input);
    assert_eq!(dry(&s), 1.0, "NONE's wet path is its dry one");
    s.set(DIST, params(SID_DIST_TAPE, 0.5, 0.0, 0.0, 0.5));
    s.set(FX, params(SID_FXSLOT_DELAY, 0.5, 0.5, 0.0, 0.5));
    s.run(&input);
    assert!((dry(&s) - 0.25).abs() < 1e-4, "{}", dry(&s));
    s.set(FX, params(SID_FXSLOT_REVERB_LUSH, 0.5, 0.5, 0.5, 1.0));
    let mut last = dry(&s);
    let mut most = 0.0f32;
    for b in input.chunks(128) {
        s.process(b);
        most = most.max((dry(&s) - last).abs());
        last = dry(&s);
    }
    println!("switching to a fully wet lush: the dry gain steps by {most} a block at most");
    assert_eq!(dry(&s), 0.0, "fully wet");
    assert!(
        most < 0.15,
        "a step of {most}: the stage ramps it over each block"
    );
}

#[test]
fn the_float_output_is_the_int16_one_before_its_conversion() {
    let input = xorshift_noise(FS as usize / 2, 9);
    let none = vec![0i16; 128];
    for p in [off(), params(SID_FXSLOT_REVERB_LUSH, 1.0, 0.5, 0.5, 0.7)] {
        let (mut a, mut b) = (Slots::new(3), Slots::new(3));
        a.set(FX, p);
        b.set(FX, p);
        for block in input.chunks(128) {
            let n = block.len() as u32;
            let mut want = vec![0i16; 2 * block.len()];
            let mut f = vec![0.0f32; 2 * block.len()];
            unsafe {
                sid_slots_process_wide(
                    &mut *a.s,
                    block.as_ptr(),
                    none.as_ptr(),
                    want.as_mut_ptr(),
                    n,
                );
                sid_slots_process_float(&mut *b.s, block.as_ptr(), none.as_ptr(), f.as_mut_ptr(), n)
            };
            let got: Vec<i16> = f
                .iter()
                .map(|&x| unsafe { sid_fx_to_i16_limited(x) })
                .collect();
            assert_eq!(got, want);
        }
    }
}
