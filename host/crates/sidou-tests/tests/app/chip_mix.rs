// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

fn mix_saturated_and_over(a: &[i16], b: &[i16], fade: chip_mix_fade) -> (Vec<i16>, Vec<i16>) {
    let mut out = a.to_vec();
    let mut over = vec![0x5a5ai16; a.len()];
    let past = unsafe {
        chip_mix(
            out.as_mut_ptr(),
            b.as_ptr(),
            over.as_mut_ptr(),
            a.len() as u32,
            fade,
        )
    };
    if past == 0 {
        assert!(over.iter().all(|&v| v == 0x5a5a), "over left alone");
        over.fill(0);
    }
    (out, over)
}

fn mix_whole(a: &[i16], b: &[i16], fade: chip_mix_fade) -> Vec<i32> {
    let (s, o) = mix_saturated_and_over(a, b, fade);
    s.iter()
        .zip(&o)
        .map(|(&s, &o)| i32::from(s) + i32::from(o))
        .collect()
}

#[test]
fn in_use_the_chips_add() {
    let a: Vec<i16> = (0..128).map(|i| (i * 37 - 2000) as i16).collect();
    let b: Vec<i16> = (0..128).map(|i| (1500 - i * 11) as i16).collect();
    let (got, over) = mix_saturated_and_over(&a, &b, CHIP_MIX_FULL);
    for i in 0..128 {
        assert_eq!(i32::from(got[i]), i32::from(a[i]) + i32::from(b[i]));
        assert_eq!(over[i], 0, "a sum that fits int16 is all in a");
    }
}

fn every_sum() -> (Vec<i32>, Vec<i16>, Vec<i16>) {
    let sums: Vec<i32> = (-65_536..=65_534).collect();
    let a = sums.iter().map(|&v| (v >> 1) as i16).collect();
    let b = sums.iter().map(|&v| (v - (v >> 1)) as i16).collect();
    (sums, a, b)
}

#[test]
fn the_sum_is_kept_whole_past_int16() {
    let (sums, a, b) = every_sum();
    for ((v, a), b) in sums.chunks(128).zip(a.chunks(128)).zip(b.chunks(128)) {
        let (s, o) = mix_saturated_and_over(a, b, CHIP_MIX_FULL);
        for i in 0..v.len() {
            assert_eq!(i32::from(s[i]), v[i].clamp(-32_768, 32_767), "{}", v[i]);
            assert_eq!(i32::from(s[i]) + i32::from(o[i]), v[i], "{}", v[i]);
        }
    }
}

#[test]
fn two_loud_chips_through_the_slots_are_soft_limited_never_on_a_rail() {
    let knee = SID_FX_LIMIT_KNEE as i32;
    let ceiling = SID_FX_LIMIT_CEILING as i32;
    let (sums, a, b) = every_sum();
    let run = |in_path: bool| {
        let mut pool = Box::<sid_slot_pool>::default();
        let mut slots = Box::<sid_slots>::default();
        unsafe { sid_slots_init(&mut *slots, 44_100, 1, &mut *pool) };
        if in_path {
            let none = sid_slot_params {
                algorithm: 0,
                amount: 0.0,
                a: 0.0,
                b: 0.0,
                mix: 1.0,
            };
            unsafe { sid_slots_set(&mut *slots, SID_SLOT_FX, &none) };
        }
        let mut out = Vec::new();
        for (a, b) in a.chunks(128).zip(b.chunks(128)) {
            let (s, o) = mix_saturated_and_over(a, b, CHIP_MIX_FULL);
            let mut lr = vec![0i16; 2 * s.len()];
            unsafe {
                sid_slots_process_wide(
                    &mut *slots,
                    s.as_ptr(),
                    o.as_ptr(),
                    lr.as_mut_ptr(),
                    s.len() as u32,
                )
            };
            assert!(lr.chunks(2).all(|f| f[0] == f[1]), "mono");
            out.extend(lr.iter().step_by(2).map(|&v| i32::from(v)));
        }
        out
    };
    let got = run(false);
    assert_eq!(
        run(true),
        got,
        "a slot in the path at no effect changes nothing"
    );
    let mut last = got[0];
    for (&v, &y) in sums.iter().zip(&got) {
        if v.abs() <= knee {
            assert_eq!(y, v, "below the knee the sum is exact");
        } else {
            assert!(y.abs() > knee && y.abs() <= ceiling, "{v} -> {y}");
            assert_eq!(y.signum(), v.signum());
        }
        assert!(y >= last && y - last <= 1, "{v} -> {y} after {last}");
        last = y;
    }
    let at = |v: i32| got[(v + 65_536) as usize];
    println!("two chips at 20,274 in phase: {}", at(40_548));
    assert!(at(40_548) > 29_000 && at(-40_548) == -at(40_548));
}

#[test]
fn the_fades_ramp_linearly_over_the_block() {
    let a = vec![0i16; 128];
    for dc in [12_800i16, -12_800, 1, -1, 32_767, -32_768] {
        let b = vec![dc; 128];
        let fin = mix_whole(&a, &b, CHIP_MIX_IN);
        let fout = mix_whole(&a, &b, CHIP_MIX_OUT);
        for i in 0..128 {
            let w_in = i as i32 + 1;
            let w_out = 127 - i as i32;
            assert_eq!(fin[i], (i32::from(dc) * w_in) >> 7, "in, {dc} at {i}");
            assert_eq!(fout[i], (i32::from(dc) * w_out) >> 7, "out, {dc} at {i}");
        }
        assert_eq!(fin[127], i32::from(dc), "the fade-in ends at all of B");
        assert_eq!(fout[127], 0, "the fade-out ends at none of it");
        let step = |v: &[i32]| v.windows(2).map(|w| (w[1] - w[0]).abs()).max();
        let most = i32::from(dc).abs() / 128 + 1;
        assert!(
            step(&fin).unwrap() <= most && step(&fout).unwrap() <= most,
            "{dc}"
        );
    }
}

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
