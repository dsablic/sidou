// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect slots (`core/fx/fx_slots.h`): exact bypass, click-free switching and
//! parameter jumps, stereo, determinism, and each algorithm's characteristic.

use std::f64::consts::PI;

use sidou_sys::*;

pub const FS: u32 = 44100;
pub const FSF: f64 = FS as f64;
pub const DIST: u32 = SID_SLOT_DIST;
pub const MOD: u32 = SID_SLOT_MOD;
pub const FX: u32 = SID_SLOT_FX;

/// The slots and the pool they borrow (boxed, so the pointer the C side keeps is stable).
pub struct Slots {
    pub s: Box<sid_slots>,
    pub pool: Box<[i16]>,
}

impl Slots {
    pub fn new(seed: u32) -> Self {
        // Garbage in the pool: the FX slot must clear it before use.
        let mut pool = vec![0x5a5au16 as i16; SID_SLOT_POOL_LEN as usize].into_boxed_slice();
        let mut s = Box::<sid_slots>::default();
        unsafe { sid_slots_init(&mut *s, FS, seed, pool.as_mut_ptr()) };
        Self { s, pool }
    }

    pub fn set(&mut self, slot: u32, p: sid_slot_params) {
        unsafe { sid_slots_set(&mut *self.s, slot, &p) }
    }

    /// Stereo out, interleaved.
    pub fn process(&mut self, input: &[i16]) -> Vec<i16> {
        let mut out = vec![0i16; 2 * input.len()];
        let n = u32::try_from(input.len()).expect("block fits in u32");
        unsafe { sid_slots_process(&mut *self.s, input.as_ptr(), out.as_mut_ptr(), n) };
        out
    }

    /// In blocks of `block`.
    pub fn run_blocks(&mut self, input: &[i16], block: usize) -> Vec<i16> {
        input.chunks(block).flat_map(|b| self.process(b)).collect()
    }

    pub fn run(&mut self, input: &[i16]) -> Vec<i16> {
        self.run_blocks(input, 128)
    }

    pub fn bytes(&self) -> Vec<u8> {
        let p = (&*self.s as *const sid_slots).cast::<u8>();
        let mut b = unsafe { std::slice::from_raw_parts(p, size_of::<sid_slots>()) }.to_vec();
        b.extend(self.pool.iter().flat_map(|v| v.to_le_bytes()));
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

pub fn sine(hz: f64, amp: f64, n: usize) -> Vec<i16> {
    (0..n)
        .map(|i| (amp * 32767.0 * (2.0 * PI * hz * i as f64 / FSF).sin()).round() as i16)
        .collect()
}

pub fn test_noise(n: usize, mut s: u32) -> Vec<i16> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 16) as u16 as i16
        })
        .collect()
}

pub fn left(x: &[i16]) -> Vec<i16> {
    x.iter().step_by(2).copied().collect()
}

pub fn right(x: &[i16]) -> Vec<i16> {
    x.iter().skip(1).step_by(2).copied().collect()
}

/// Largest 2nd difference per channel of interleaved stereo.
pub fn max_second_diff_stereo(x: &[i16]) -> i32 {
    [left(x), right(x)]
        .iter()
        .map(|c| {
            c.windows(3)
                .map(|w| (i32::from(w[2]) - 2 * i32::from(w[1]) + i32::from(w[0])).abs())
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap()
}

/// Same, of the difference of two interleaved stereo signals.
pub fn residual(a: &[i16], b: &[i16]) -> i32 {
    let d: Vec<i32> = a
        .iter()
        .zip(b)
        .map(|(x, y)| i32::from(*x) - i32::from(*y))
        .collect();
    let chan = |c: usize| -> i32 {
        let v: Vec<i32> = d.iter().skip(c).step_by(2).copied().collect();
        v.windows(3)
            .map(|w| (w[2] - 2 * w[1] + w[0]).abs())
            .max()
            .unwrap_or(0)
    };
    chan(0).max(chan(1))
}

pub const T_ON: usize = FS as usize / 4;
pub const T_OFF: usize = 3 * FS as usize / 2;
pub const T_END: usize = 3 * FS as usize;
pub const RAMP: usize = FS as usize / 5;
pub const CLICK: i32 = 100;
/// Residual threshold (the coordinator's limit); a CLICK-sized step must exceed it.
pub const RESIDUAL_MAX: i32 = 30;

/// Which control a residual run moves.
#[derive(Clone, Copy, Debug)]
pub enum Control {
    Amount,
    A,
    Mix,
}

/// Click metric for one control of one slot: it jumps from `from` to `to` at T_ON and
/// back at T_OFF (J), against a reference that ramps it over RAMP instead (R), with the
/// slot's other controls at `base`. Both runs trace the same path at different speeds;
/// a 110 Hz input keeps the 2nd difference of J - R to a few LSB, while a click in
/// either shows at full size. `click` adds a synthetic step to J's left channel.
pub fn slot_residual(
    slot: u32,
    base: sid_slot_params,
    control: Control,
    from: f32,
    to: f32,
    click: Option<usize>,
) -> i32 {
    let input = sine(110.0, 0.5, T_END);
    let with = |v: f32| {
        let mut p = base;
        match control {
            Control::Amount => p.amount = v,
            Control::A => p.a = v,
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
    residual(&j, &r)
}

/// Asserts the residual metric for every listed control of an algorithm, and that it
/// would see a CLICK-sized step at a jump.
pub fn assert_click_free(slot: u32, base: sid_slot_params, moves: &[(Control, f32, f32)]) {
    for &(control, from, to) in moves {
        let m = slot_residual(slot, base, control, from, to, None);
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
    let c = slot_residual(slot, base, control, from, to, Some(T_ON + 10));
    assert!(c > RESIDUAL_MAX, "a {CLICK} LSB click shows only {c}");
}

/// Switches `slot` through `algorithms` (each held 0.5 s) on a 110 Hz tone; returns the
/// worst 2nd difference within 60 ms of each switch and the worst in the steady parts.
pub fn switch_steps(slot: u32, algorithms: &[u32], amount: f32, a: f32, b: f32) -> (i32, i32) {
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
    let before = s.bytes();
    let mut input = test_noise(4096, 3);
    input.extend([i16::MIN, i16::MAX, 0, -1, 1]);
    let out = s.process(&input);
    assert_eq!(left(&out), input);
    assert_eq!(right(&out), input);
    assert_eq!(s.bytes(), before, "bypass changed the state");
}

#[test]
fn none_at_full_mix_is_the_identity() {
    let mut s = Slots::new(1);
    for slot in [DIST, MOD, FX] {
        s.set(slot, params(0, 1.0, 1.0, 1.0, 1.0));
    }
    let input = test_noise(FS as usize, 5);
    let out = s.run(&input);
    assert_eq!(left(&out), input);
    assert_eq!(right(&out), input);
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
        let input = test_noise(4096, 9);
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
    let (sw, steady) = switch_steps(
        DIST,
        &[SID_DIST_TAPE, SID_DIST_LOFI, SID_DIST_NONE, SID_DIST_TAPE],
        0.8,
        0.0,
        0.0,
    );
    println!("dist switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady.max(0) + RESIDUAL_MAX, "a switch steps by {sw}");
    let (sw, steady) = switch_steps(
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
    // Each move starts from the others at a setting where it matters. Crush and the
    // noise (Lo-fi B) are steppy or random by nature; their blocks have exact-model
    // click tests in fx.rs.
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
