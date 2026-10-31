// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::slots::*;
use crate::{ac_rms, sine, xorshift_noise, FS, FSF, RESIDUAL_MAX};

fn burst_of_5ms_at_300hz(n: usize, at: usize, amp: f64) -> Vec<i16> {
    let mut x = vec![0i16; n];
    let b = sine(300.0, amp, 220);
    x[at..at + b.len()].copy_from_slice(&b);
    x
}

fn peak_near(x: &[i16], at: usize, w: usize) -> i32 {
    x[at.saturating_sub(w)..(at + w).min(x.len())]
        .iter()
        .map(|&v| i32::from(v).abs())
        .max()
        .unwrap_or(0)
}

fn echo_time_in_samples(s: &Slots) -> f64 {
    f64::from(unsafe { s.s.fx.echo.time })
}

#[test]
fn delay_echoes_at_the_time_and_decays_by_the_feedback() {
    let n = 3 * FS as usize;
    let input = burst_of_5ms_at_300hz(n, 1000, 0.25);
    let mut s = Slots::new(1);
    s.set(FX, params(SID_FXSLOT_DELAY, 0.5, 0.5, 0.0, 1.0));
    s.run(&vec![0i16; FS as usize / 10]);
    let out = s.run(&input);
    let (l, r) = (left(&out), right(&out));
    assert_eq!(l, r, "mono delay: both sides alike");
    let t = echo_time_in_samples(&s);
    let min = SID_FX_ECHO_MIN_MS * 1e-3 * FSF;
    let octaves = ((f64::from(SID_SLOT_POOL_LEN) - 4.0) / min).log2();
    let want = min * 2f64.powf(octaves * 0.5);
    println!("delay at A = 0.5: {t:.2} samples (want {want:.2})");
    assert!((t / want - 1.0).abs() < 0.003);
    let p0 = peak_near(&input, 1055, 60);
    let echoes: Vec<i32> = (1..=3)
        .map(|k| peak_near(&l, 1055 + (k as f64 * t) as usize, 60))
        .collect();
    let fb = SID_FX_ECHO_FEEDBACK * 0.5;
    println!("delay echoes: {p0} -> {echoes:?} (feedback {fb})");
    assert!((f64::from(echoes[0]) / f64::from(p0) - 1.0).abs() < 0.02);
    for k in 1..3 {
        let ratio = f64::from(echoes[k]) / f64::from(echoes[k - 1]);
        assert!((ratio / fb - 1.0).abs() < 0.03, "repeat {k}: ratio {ratio}");
    }
    assert!(peak_near(&l, 1055 + (1.5 * t) as usize, 200) < 3);
}

#[test]
fn ping_pong_alternates_sides() {
    let n = 2 * FS as usize;
    let input = burst_of_5ms_at_300hz(n, 1000, 0.25);
    let mut s = Slots::new(1);
    s.set(FX, params(SID_FXSLOT_PINGPONG, 0.6, 0.3, 0.0, 1.0));
    s.run(&vec![0i16; FS as usize / 10]);
    let out = s.run(&input);
    let (l, r) = (left(&out), right(&out));
    let t = echo_time_in_samples(&s);
    let at = |k: f64| 1055 + (k * t) as usize;
    let (l1, r1, l3, r2) = (
        peak_near(&l, at(1.0), 60),
        peak_near(&r, at(1.0), 60),
        peak_near(&l, at(3.0), 60),
        peak_near(&r, at(2.0), 60),
    );
    println!(
        "ping-pong at T = {t:.0}: L1 {l1}, R1 {r1}, R2 {r2}, L2 {}, L3 {l3}",
        peak_near(&l, at(2.0), 60)
    );
    assert!(l1 > 3000 && r1 < 3, "first echo left only");
    assert!(
        r2 > 3000 && peak_near(&l, at(2.0), 60) < 3,
        "second echo right only"
    );
    assert!(l3 > 1000, "third echo back left");
}

#[test]
fn delay_feedback_is_stable_at_the_top() {
    let mut s = Slots::new(1);
    s.set(FX, params(SID_FXSLOT_DELAY, 1.0, 0.1, 0.0, 1.0));
    let loud = s.run(&xorshift_noise(5 * FS as usize, 5));
    let pk = loud.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    let out = s.run(&vec![0i16; 15 * FS as usize]);
    let last = &out[out.len() - 2 * FS as usize..];
    let tail = last.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    println!("delay at full feedback: peak {pk} on noise, {tail} 13 s after it stops");
    assert_eq!(tail, 0, "does not die away to silence: {tail}");
}

#[test]
fn switching_clears_the_pool_first() {
    let mut s = Slots::new(1);
    s.set(FX, params(SID_FXSLOT_DELAY, 0.9, 0.6, 0.0, 1.0));
    s.run(&xorshift_noise(FS as usize, 1));
    s.set(FX, params(SID_FXSLOT_REVERB_LUSH, 0.9, 0.0, 0.0, 1.0));
    s.run(&xorshift_noise(FS as usize / 2, 2));
    s.set(FX, params(SID_FXSLOT_DELAY, 0.9, 0.6, 0.0, 1.0));
    let quiet = vec![0i16; 3 * FS as usize];
    let out = s.run(&quiet);
    let after = &out[2 * (FS as usize / 10)..];
    let pk = after.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    assert_eq!(pk, 0, "the new delay replays old pool contents ({pk})");
}

fn rt60(l: &[i16], r: &[i16]) -> f64 {
    let e: Vec<f64> = l
        .iter()
        .zip(r)
        .map(|(&a, &b)| f64::from(a).powi(2) + f64::from(b).powi(2))
        .collect();
    let mut tail = vec![0.0; e.len() + 1];
    for i in (0..e.len()).rev() {
        tail[i] = tail[i + 1] + e[i];
    }
    let db = |i: usize| 10.0 * (tail[i] / tail[0]).log10();
    let t5 = (0..e.len()).find(|&i| db(i) < -5.0).unwrap();
    let t25 = (0..e.len()).find(|&i| db(i) < -25.0).unwrap();
    3.0 * (t25 - t5) as f64 / FSF
}

fn reverb_response(
    alg: u32,
    amount: f32,
    damping: f32,
    b: f32,
    seconds: usize,
) -> (Vec<i16>, Vec<i16>) {
    let mut s = Slots::new(1);
    s.set(FX, params(alg, amount, damping, b, 1.0));
    s.run(&vec![0i16; FS as usize / 10]);
    let out = s.run(&burst_of_5ms_at_300hz(seconds * FS as usize, 0, 0.5));
    (left(&out), right(&out))
}

#[test]
fn reverbs_decay_longer_with_amount_and_are_wide() {
    for alg in [SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_REVERB_LUSH] {
        let mut last = 0.0;
        for amount in [0.0f32, 0.5, 1.0] {
            let (l, r) = reverb_response(alg, amount, 0.3, 0.5, 12);
            let t = rt60(&l, &r);
            let rho = {
                let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
                for (&x, &y) in l.iter().zip(&r).skip(FS as usize / 10) {
                    let (x, y) = (f64::from(x), f64::from(y));
                    ab += x * y;
                    aa += x * x;
                    bb += y * y;
                }
                ab / (aa * bb).sqrt()
            };
            println!("reverb {alg} amount {amount}: RT60 {t:.2} s, L/R correlation {rho:.3}");
            assert!(
                t > last * 1.2,
                "reverb {alg}: RT60 {t} at {amount} is not above {last}"
            );
            assert!(rho.abs() < 0.3, "reverb {alg}: L/R correlate at {rho}");
            last = t;
        }
        assert!(
            (2.0..=10.0).contains(&last),
            "reverb {alg}: RT60 at full {last} s"
        );
    }
}

fn brightness(x: &[i16]) -> f64 {
    let d: Vec<i16> = x.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
    ac_rms(&d) / ac_rms(x)
}

#[test]
fn reverbs_damping_darkens_the_tail() {
    for alg in [SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_REVERB_LUSH] {
        let tail_brightness = |damping: f32| {
            let mut s = Slots::new(1);
            s.set(FX, params(alg, 0.6, damping, 0.5, 1.0));
            s.run(&vec![0i16; FS as usize / 10]);
            let mut input = xorshift_noise(441, 4);
            input.resize(2 * FS as usize, 0);
            let out = s.run(&input);
            brightness(&left(&out)[FS as usize / 2..])
        };
        let (bright, dark) = (tail_brightness(0.0), tail_brightness(1.0));
        println!("reverb {alg}: tail brightness {bright:.3} undamped, {dark:.3} damped");
        assert!(dark < 0.7 * bright);
    }
}

#[test]
fn reverbs_are_stable_and_never_denormal() {
    for alg in [SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_REVERB_LUSH] {
        let mut s = Slots::new(1);
        s.set(FX, params(alg, 1.0, 0.0, 0.0, 1.0));
        s.run(&xorshift_noise(10 * FS as usize, 9));
        let out = s.run(&vec![0i16; 20 * FS as usize]);
        let last = &out[out.len() - 2 * FS as usize..];
        let pk = last.iter().map(|&v| i32::from(v).abs()).max().unwrap();
        println!("reverb {alg}: peak {pk} LSB 18 s after full-scale noise at full decay");
        assert!(pk < 64, "reverb {alg} does not decay: {pk}");
        let states: Vec<f32> = unsafe {
            if alg == SID_FXSLOT_REVERB_LIGHT {
                vec![s.s.fx.reverb.lp_decay_1, s.s.fx.reverb.lp_decay_2]
            } else {
                let p = &s.s.fx.plate;
                let mut v = vec![p.bw, p.damp_l, p.damp_r, p.tank_l, p.tank_r];
                v.extend(s.pool.f32_.iter().copied());
                v
            }
        };
        for v in states {
            assert!(v.is_finite() && (v == 0.0 || v.abs() > 1e-30), "state {v}");
        }
    }
}

#[test]
fn fx_parameter_jumps_are_click_free() {
    use Control::*;
    let all = [
        (Amount, 0.0, 1.0),
        (A, 0.0, 1.0),
        (B, 0.0, 1.0),
        (Mix, 0.0, 1.0),
    ];
    assert_click_free_at(0.02, FX, params(SID_FXSLOT_DELAY, 0.5, 0.2, 0.5, 1.0), &all);
    assert_click_free_at(
        0.02,
        FX,
        params(SID_FXSLOT_PINGPONG, 0.5, 0.2, 0.5, 1.0),
        &all,
    );
    assert_click_free_at(
        0.1,
        FX,
        params(SID_FXSLOT_REVERB_LIGHT, 0.5, 0.5, 0.5, 1.0),
        &all,
    );
    assert_click_free_at(
        0.1,
        FX,
        params(SID_FXSLOT_REVERB_LUSH, 0.5, 0.5, 0.5, 1.0),
        &all,
    );
}

#[test]
fn reverb_lines_fit_their_rings_at_any_sample_rate() {
    for fs in [8000u32, 32000, 44100, 48000, 96000] {
        let mut rv = sid_fx_reverb::default();
        let mut pl = sid_fx_plate::default();
        unsafe {
            sid_fx_reverb_init(&mut rv, fs);
            sid_fx_plate_init(&mut pl, fs);
        }
        let end_rv = rv.base[9] + rv.length[9];
        let end_pl = pl.base[11] + pl.length[11];
        println!("{fs} Hz: light uses {end_rv} of {SID_FX_REVERB_LEN}, lush {end_pl} of {SID_FX_PLATE_LEN}");
        assert!(end_rv <= SID_FX_REVERB_LEN && end_pl <= SID_FX_PLATE_LEN);
    }
}

#[test]
fn delay_time_glides_without_a_kink_at_musical_level() {
    use Control::*;
    for alg in [SID_FXSLOT_DELAY, SID_FXSLOT_PINGPONG] {
        assert_click_free_at(0.5, FX, params(alg, 0.0, 0.2, 0.0, 1.0), &[(A, 0.0, 1.0)]);
    }
}

#[test]
fn switching_fx_is_click_free() {
    let (sw, steady) = worst_second_diff_at_and_between_switches(
        FX,
        &[
            SID_FXSLOT_DELAY,
            SID_FXSLOT_REVERB_LIGHT,
            SID_FXSLOT_PINGPONG,
            SID_FXSLOT_REVERB_LUSH,
            SID_FXSLOT_DELAY,
        ],
        0.6,
        0.3,
        0.5,
    );
    println!("FX switches: {sw} LSB at a switch, {steady} steady");
    assert!(sw <= steady + RESIDUAL_MAX, "a switch steps by {sw}");
}

#[test]
fn all_slots_together_are_deterministic_in_any_block_size() {
    let n = FS as usize;
    let input = sine(220.0, 0.4, 3 * n);
    let render = |block: usize| {
        let mut s = Slots::new(11);
        s.set(DIST, params(SID_DIST_FUZZ, 0.4, 0.3, 0.5, 0.8));
        s.set(MOD, params(SID_MOD_CHORUS, 0.7, 0.5, 1.0, 1.0));
        s.set(FX, params(SID_FXSLOT_REVERB_LUSH, 0.7, 0.3, 0.3, 0.5));
        let mut out = s.run_blocks(&input[..n], block);
        for slot in [DIST, MOD, FX] {
            s.set(slot, off());
        }
        out.extend(s.run_blocks(&input[n..2 * n], block));
        s.set(FX, params(SID_FXSLOT_PINGPONG, 0.7, 0.3, 0.3, 0.5));
        out.extend(s.run_blocks(&input[2 * n..], block));
        out
    };
    let reference = render(128);
    for block in [1, 77, 1000] {
        assert!(render(block) == reference, "block size {block} differs");
    }
}

#[test]
fn odd_block_sizes_and_switches_at_mix_0_do_not_change_the_output() {
    let n = 3 * FS as usize;
    let input = sine(220.0, 0.4, n);
    let settings: Vec<(usize, u32, sid_slot_params)> = vec![
        (0, DIST, params(SID_DIST_OVERDRIVE, 0.6, 0.5, 0.3, 0.0)),
        (0, MOD, params(SID_MOD_CHORUS, 0.7, 0.5, 1.0, 0.0)),
        (0, FX, params(SID_FXSLOT_DELAY, 0.5, 0.3, 0.3, 0.0)),
        (5000, DIST, params(SID_DIST_FUZZ, 0.6, 0.5, 0.3, 0.7)),
        (9000, MOD, params(SID_MOD_FLANGER, 0.7, 0.5, 0.6, 0.0)),
        (
            13000,
            FX,
            params(SID_FXSLOT_REVERB_LUSH, 0.5, 0.3, 0.3, 0.6),
        ),
        (20000, DIST, params(SID_DIST_TAPE, 0.6, 0.5, 0.3, 0.0)),
        (26000, MOD, params(SID_MOD_PHASER, 0.7, 0.5, 0.6, 0.8)),
        (40000, FX, params(SID_FXSLOT_PINGPONG, 0.5, 0.3, 0.3, 0.0)),
        (
            60000,
            FX,
            params(SID_FXSLOT_REVERB_LIGHT, 0.5, 0.3, 0.3, 0.5),
        ),
        (90000, DIST, off()),
        (100000, MOD, off()),
    ];
    let render = |blocks: &[usize]| {
        let mut s = Slots::new(3);
        let mut out = Vec::with_capacity(2 * n);
        let mut pos = 0;
        let mut k = 0;
        while pos < n {
            let next = settings
                .iter()
                .map(|e| e.0)
                .filter(|&t| t > pos)
                .min()
                .unwrap_or(n);
            let len = blocks[k % blocks.len()].min(next - pos).min(n - pos);
            for &(t, slot, p) in &settings {
                if t == pos {
                    s.set(slot, p);
                }
            }
            out.extend(s.process(&input[pos..pos + len]));
            pos += len;
            k += 1;
        }
        out
    };
    let reference = render(&[128]);
    let odd: Vec<usize> = (0..65).map(|i| 2 * i + 1).chain([1000]).collect();
    let mut rng = 0x1234_5678u32;
    for trial in 0..4 {
        let blocks: Vec<usize> = (0..500)
            .map(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                odd[rng as usize % odd.len()]
            })
            .collect();
        let out = render(&blocks);
        let diff = out.iter().zip(&reference).filter(|(a, b)| a != b).count();
        assert_eq!(diff, 0, "trial {trial}: {diff} samples differ");
    }
}
