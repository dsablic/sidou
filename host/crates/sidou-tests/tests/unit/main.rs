// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;

mod fx;
mod sid;
mod sid_character;
mod sid_dac;
mod sid_env;
mod sid_filter;
mod sid_noise;
mod sid_osc;
mod sid_osc_fuzz;
mod sid_sync;
mod slots;
mod slots_dist;
mod slots_fx;
mod slots_mod;

#[macro_export]
macro_rules! assert_near {
    ($a:expr, $b:expr, $tol:expr) => {
        $crate::assert_near!($a, $b, $tol, "")
    };
    ($a:expr, $b:expr, $tol:expr, $($msg:tt)+) => {{
        let (a, b, tol) = ($a, $b, $tol);
        assert!(
            (a - b).abs() <= tol,
            "{} = {a} is not within {tol} of {} = {b} {}",
            stringify!($a),
            stringify!($b),
            format_args!($($msg)+)
        );
    }};
}

pub const FS: u32 = 44100;
pub const FSF: f64 = FS as f64;
pub const FIRMWARE_BLOCK: usize = 128;
pub const T_ON: usize = FS as usize / 4;
pub const T_OFF: usize = 3 * FS as usize / 2;
pub const T_END: usize = 3 * FS as usize;
pub const RAMP: usize = FS as usize / 5;
pub const CLICK: i32 = 100;
pub const RESIDUAL_MAX: i32 = 30;
pub const RELEASE_6_PERIOD: u32 = 267;
pub const ATTACK_0_FROM_RELOAD: u32 = 9 + 3;
pub const RELEASE_FROM_RELOAD: u32 = 3;
const MAX_ULPS: u32 = 4;

pub struct Lcg(pub u32);

impl Lcg {
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 >> 8
    }
}

pub fn sine(hz: f64, amp: f64, n: usize) -> Vec<i16> {
    (0..n)
        .map(|i| (amp * 32767.0 * (2.0 * PI * hz * i as f64 / FSF).sin()).round() as i16)
        .collect()
}

pub fn xorshift_noise(n: usize, mut s: u32) -> Vec<i16> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 16) as u16 as i16
        })
        .collect()
}

pub fn tone_amp(x: &[i16], hz: f64) -> f64 {
    let w = 2.0 * PI * hz / FSF;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        re += f64::from(v) * (w * i as f64).cos();
        im += f64::from(v) * (w * i as f64).sin();
    }
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

pub fn thd(x: &[i16], hz: f64) -> f64 {
    let harmonics = (2..=10)
        .map(|k| tone_amp(x, hz * f64::from(k)).powi(2))
        .sum::<f64>()
        .sqrt();
    harmonics / tone_amp(x, hz)
}

pub fn ac_rms(x: &[i16]) -> f64 {
    let mean = x.iter().map(|&v| f64::from(v)).sum::<f64>() / x.len() as f64;
    (x.iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / x.len() as f64)
        .sqrt()
}

pub fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

pub fn peak(x: &[i16]) -> i32 {
    x.iter().map(|&v| i32::from(v).abs()).max().unwrap_or(0)
}

pub fn chip(model: sidou_sys::sid_model) -> Box<sidou_sys::sid> {
    let mut s = Box::<sidou_sys::sid>::default();
    unsafe { sidou_sys::sid_init(&mut *s, model, sidou_sys::SID_CLOCK_PAL, FS) };
    s
}

pub fn max_second_diff(x: &[i32]) -> i32 {
    x.windows(3)
        .map(|w| (w[2] - 2 * w[1] + w[0]).abs())
        .max()
        .unwrap_or(0)
}

pub fn difference(a: &[i16], b: &[i16]) -> Vec<i32> {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| i32::from(x) - i32::from(y))
        .collect()
}

pub fn settle_filter_coefficients(s: &mut sidou_sys::sid) {
    let mut none = [0i16; 1];
    unsafe { sidou_sys::sid_render(s, none.as_mut_ptr(), 0) };
}

pub fn assert_float_eq(a: f32, b: f32) {
    fn biased(x: f32) -> u32 {
        let bits = x.to_bits();
        if bits & 0x8000_0000 != 0 {
            !bits + 1
        } else {
            bits | 0x8000_0000
        }
    }
    let (x, y) = (biased(a), biased(b));
    assert!(
        !a.is_nan() && !b.is_nan() && x.abs_diff(y) <= MAX_ULPS,
        "{a} and {b} differ by more than {MAX_ULPS} ULPs"
    );
}
