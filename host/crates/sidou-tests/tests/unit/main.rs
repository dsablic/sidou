// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit tests of the C core through FFI, one module per area of `core/sid`.

mod fx;
mod sid;
mod sid_character;
mod sid_dac;
mod sid_defs;
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

/// |a - b| <= tolerance, like GoogleTest's EXPECT_NEAR (NaN fails).
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

/// The filter's coefficients as the next render takes them: a write to $15-$17 only marks
/// the filter (sid.c, sid_write), and a render recomputes it before its first sample. This
/// one renders no sample, so the chip's voices and output stage are as they were.
pub fn settle(s: &mut sidou_sys::sid) {
    let mut none = [0i16; 1];
    unsafe { sidou_sys::sid_render(s, none.as_mut_ptr(), 0) };
}

/// Within 4 ULPs, like GoogleTest's EXPECT_FLOAT_EQ.
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
        !a.is_nan() && !b.is_nan() && x.abs_diff(y) <= 4,
        "{a} and {b} differ by more than 4 ULPs"
    );
}
