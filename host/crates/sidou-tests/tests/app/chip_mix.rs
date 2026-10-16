// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Chip B into chip A before the slots (`app/chip_mix.c`): the fades' weights, saturation, and
//! the headroom two chips leave.

use sidou_sys::*;

fn mix(a: &[i16], b: &[i16], fade: chip_mix_fade) -> Vec<i16> {
    let mut out = a.to_vec();
    unsafe { chip_mix(out.as_mut_ptr(), b.as_ptr(), a.len() as u32, fade) };
    out
}

#[test]
fn in_use_the_chips_add() {
    let a: Vec<i16> = (0..128).map(|i| (i * 37 - 2000) as i16).collect();
    let b: Vec<i16> = (0..128).map(|i| (1500 - i * 11) as i16).collect();
    let got = mix(&a, &b, CHIP_MIX_FULL);
    for i in 0..128 {
        assert_eq!(i32::from(got[i]), i32::from(a[i]) + i32::from(b[i]));
    }
}

#[test]
fn the_sum_saturates() {
    let got = mix(&[30000, -30000, 100], &[10000, -10000, -50], CHIP_MIX_FULL);
    assert_eq!(got, [32767, -32768, 50]);
}

/// A fade-in takes (i + 1) / 128 of B at sample i, ending at all of it; a fade-out
/// (127 - i) / 128, ending at none: a DC level of B ramps by at most 1/128 of itself a sample.
#[test]
fn the_fades_ramp_linearly_over_the_block() {
    let a = vec![0i16; 128];
    for dc in [12_800i16, -12_800, 1, -1, 32_767, -32_768] {
        let b = vec![dc; 128];
        let fin = mix(&a, &b, CHIP_MIX_IN);
        let fout = mix(&a, &b, CHIP_MIX_OUT);
        for i in 0..128 {
            let w_in = i as i32 + 1;
            let w_out = 127 - i as i32;
            assert_eq!(
                i32::from(fin[i]),
                (i32::from(dc) * w_in) >> 7,
                "in, {dc} at {i}"
            );
            assert_eq!(
                i32::from(fout[i]),
                (i32::from(dc) * w_out) >> 7,
                "out, {dc} at {i}"
            );
        }
        assert_eq!(fin[127], dc, "the fade-in ends at all of B");
        assert_eq!(fout[127], 0, "the fade-out ends at none of it");
        let step = |v: &[i16]| {
            v.windows(2)
                .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
                .max()
        };
        let most = i32::from(dc).abs() / 128 + 1;
        assert!(
            step(&fin).unwrap() <= most && step(&fout).unwrap() <= most,
            "{dc}"
        );
    }
}

/// The headroom (app/chip_mix.h), measured on every bench scenario on both models: one chip's
/// largest sample after the power-on transient (the run's second half: the first has the
/// DC blocker settling, up to 23,834) is 20,274 (gate_every_frame on the 6581), so two chips that
/// loud and in phase clip at the mix, by up to 3.3 dB; under 16,383 each they never do.
/// Recorded in decision record 0004 and the plan's risks; the bound holds the figure.
#[test]
fn one_chips_loudest_steady_sample_is_as_recorded() {
    let mut worst = (0i32, String::new());
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        for s in 0..WORKLOAD_SCENARIOS {
            let out = workload::render_scenario(1, model, s);
            let steady = out[out.len() / 2..]
                .iter()
                .map(|&v| i32::from(v).abs())
                .max()
                .unwrap_or(0);
            if steady > worst.0 {
                let name = unsafe { std::ffi::CStr::from_ptr(workload_scenario_name(s)) };
                worst = (
                    steady,
                    format!("{} on model {model}", name.to_string_lossy()),
                );
            }
        }
    }
    println!(
        "one chip's loudest steady sample: {} ({}); two in phase: {}",
        worst.0,
        worst.1,
        2 * worst.0
    );
    assert!(worst.0 <= 20_300, "louder than recorded: {worst:?}");
}
