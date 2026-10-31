// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;

use sidou_sys::*;
use sidou_tests::signal_metrics::estimate_f0;

use crate::{ac_rms, difference, max_second_diff, CLICK, RAMP, RESIDUAL_MAX, T_END, T_OFF, T_ON};
use crate::{db, peak, sine, thd, tone_amp, xorshift_noise, FIRMWARE_BLOCK, FS, FSF};

struct Fx(Box<sid_fx>);

impl Fx {
    fn new(seed: u32) -> Self {
        let mut fx = Box::<sid_fx>::default();
        unsafe { sid_fx_init(&mut *fx, FS, seed) };
        Self(fx)
    }

    fn set(&mut self, p: sid_fx_params) {
        unsafe { sid_fx_set(&mut *self.0, &p) }
    }

    fn process(&mut self, io: &mut [i16]) {
        let n = u32::try_from(io.len()).expect("block fits in u32");
        unsafe { sid_fx_process(&mut *self.0, io.as_mut_ptr(), n) }
    }

    fn run(&mut self, input: &[i16]) -> Vec<i16> {
        let mut out = input.to_vec();
        for block in out.chunks_mut(FIRMWARE_BLOCK) {
            self.process(block);
        }
        out
    }

    fn bytes(&self) -> Vec<u8> {
        let p = (&*self.0 as *const sid_fx).cast::<u8>();
        unsafe { std::slice::from_raw_parts(p, size_of::<sid_fx>()) }.to_vec()
    }
}

fn off() -> sid_fx_params {
    sid_fx_params::default()
}

fn all_on() -> sid_fx_params {
    sid_fx_params {
        tape_drive: 1.0,
        wow: 1.0,
        flutter: 1.0,
        noise: 1.0,
        crt: 1.0,
        crush: 1.0,
    }
}

fn only(name: &str, v: f32) -> sid_fx_params {
    let mut p = off();
    *field(&mut p, name) = v;
    p
}

fn field<'a>(p: &'a mut sid_fx_params, name: &str) -> &'a mut f32 {
    match name {
        "tape_drive" => &mut p.tape_drive,
        "wow" => &mut p.wow,
        "flutter" => &mut p.flutter,
        "noise" => &mut p.noise,
        "crt" => &mut p.crt,
        "crush" => &mut p.crush,
        _ => panic!("no parameter {name}"),
    }
}

const PARAMS: [&str; 6] = ["tape_drive", "wow", "flutter", "noise", "crt", "crush"];

fn to_f32(x: &[i16]) -> Vec<f32> {
    x.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

fn cycle_frequencies(x: &[i16]) -> Vec<(f64, f64)> {
    let mut crossings = Vec::new();
    for i in 1..x.len() {
        let (a, b) = (f64::from(x[i - 1]), f64::from(x[i]));
        if a < 0.0 && b >= 0.0 {
            crossings.push((i - 1) as f64 + a / (a - b));
        }
    }
    crossings
        .windows(2)
        .map(|c| (c[1] / FSF, FSF / (c[1] - c[0])))
        .collect()
}

fn cents(hz: f64, reference: f64) -> f64 {
    1200.0 * (hz / reference).log2()
}

fn upward_mean_crossings(trace: &[f64], avg: usize) -> usize {
    let smooth: Vec<f64> = trace
        .windows(avg)
        .map(|w| w.iter().sum::<f64>() / avg as f64)
        .collect();
    let mean = smooth.iter().sum::<f64>() / smooth.len() as f64;
    smooth
        .windows(2)
        .filter(|w| w[0] < mean && w[1] >= mean)
        .count()
}

#[test]
fn bypass_is_exact_and_leaves_state_untouched() {
    let mut fx = Fx::new(1);
    fx.set(off());
    let before = fx.bytes();
    let mut input = xorshift_noise(4096, 7);
    input.extend([i16::MIN, i16::MAX, 0, -1, 1]);
    let mut io = input.clone();
    fx.process(&mut io);
    assert_eq!(io, input);
    assert_eq!(fx.bytes(), before, "bypass changed the state");
}

#[test]
fn bypass_is_exact_again_once_everything_has_settled_off() {
    let mut fx = Fx::new(1);
    fx.set(all_on());
    fx.run(&sine(440.0, 0.5, FS as usize));
    fx.set(off());
    fx.run(&sine(440.0, 0.5, FS as usize));
    let cur = fx.0.cur;
    for name in PARAMS {
        let mut c = cur;
        assert_eq!(
            *field(&mut c, name),
            0.0,
            "{name} did not settle to exactly 0"
        );
    }
    let before = fx.bytes();
    let input = xorshift_noise(4096, 9);
    assert_eq!(fx.run(&input), input);
    assert_eq!(fx.bytes(), before);
}

#[test]
fn one_effect_on_leaves_the_others_out_of_the_path() {
    let mut a = Fx::new(5);
    let mut b = Fx::new(5);
    a.set(only("noise", 1.0));
    b.set(only("noise", 1.0));
    let n = FS as usize;
    let hiss = a.run(&vec![0; n]);
    let input = sine(440.0, 0.25, n);
    let both = b.run(&input);
    let mut off_by_one = 0;
    for i in 0..n {
        let d = i32::from(both[i]) - i32::from(input[i]) - i32::from(hiss[i]);
        assert!(d.abs() <= 1, "sample {i} differs by {d}");
        off_by_one += usize::from(d != 0);
    }
    assert!(off_by_one < n / 100, "{off_by_one} samples differ");
}

#[test]
fn tape_distortion_rises_with_drive_and_gain_stays_bounded() {
    let hz = FSF / 100.0;
    let n = FS as usize;
    let input = sine(hz, 0.5, 2 * n);
    let mut last = 0.0;
    for drive in [0.25f32, 0.5, 1.0] {
        let mut fx = Fx::new(1);
        fx.set(only("tape_drive", drive));
        let out = fx.run(&input);
        let t = thd(&out[n..], hz);
        println!("tape drive {drive}: THD {:.2} %", 100.0 * t);
        assert!(
            t > last * 1.5,
            "THD {t} at drive {drive} is not well above {last}"
        );
        last = t;
        assert!(tone_amp(&out[n..], 2.0 * hz) > 1e-3 * tone_amp(&out[n..], hz));
    }
    assert!(last > 0.03, "full drive THD {last} is too subtle");
    assert!(last < 0.4, "full drive THD {last} is not a soft saturation");

    for amp in [0.05, 0.2, 0.3, 0.5, 1.0] {
        let input = sine(hz, amp, 2 * n);
        let mut fx = Fx::new(1);
        fx.set(only("tape_drive", 1.0));
        let out = fx.run(&input);
        let gain = f64::from(peak(&out[n..])) / f64::from(peak(&input[n..]));
        let level = db(ac_rms(&out[n..]) / ac_rms(&input[n..]));
        println!("tape amp {amp}: peak gain {gain:.3}, level {level:.2} dB");
        assert!(gain < 1.25, "peak gain {gain} at amplitude {amp}");
        if amp <= 0.3 {
            assert!(
                level.abs() < 1.5,
                "level changed by {level} dB at amplitude {amp}"
            );
        }
    }
}

#[test]
fn tape_compresses_highs_before_lows() {
    let n = FS as usize;
    let gain = |hz: f64, amp: f64| {
        let input = sine(hz, amp, 2 * n);
        let mut fx = Fx::new(1);
        fx.set(only("tape_drive", 1.0));
        let out = fx.run(&input);
        tone_amp(&out[n..], hz) / tone_amp(&input[n..], hz)
    };
    let low = gain(200.0, 0.3);
    for hz in [3000.0, 6000.0, 8000.0] {
        let high = gain(hz, 0.3);
        println!("tape fundamental gain at 0.3 FS: 200 Hz {low:.3}, {hz} Hz {high:.3}");
        assert!(
            high < low * 0.9,
            "{hz} Hz ({high}) is not compressed more than 200 Hz ({low})"
        );
    }
    let (low7, high7) = (gain(200.0, 0.7), gain(8000.0, 0.7));
    println!("tape fundamental gain at 0.7 FS: 200 Hz {low7:.3}, 8 kHz {high7:.3}");
    assert!(
        high7 >= low7,
        "the hot-signal inversion in fx_tape.h no longer holds"
    );
}

fn cents_of_each_cycle_of_a_1khz_tone(p: sid_fx_params, seconds: usize) -> Vec<(f64, f64)> {
    let n = seconds * FS as usize;
    let mut fx = Fx::new(3);
    fx.set(p);
    let out = fx.run(&sine(1000.0, 0.5, n));
    cycle_frequencies(&out)
        .into_iter()
        .filter(|&(t, _)| t > 0.5)
        .map(|(t, f)| (t, cents(f, 1000.0)))
        .collect()
}

#[test]
fn wow_is_slow_pitch_modulation_of_tens_of_cents() {
    let trace = cents_of_each_cycle_of_a_1khz_tone(only("wow", 1.0), 10);
    let c: Vec<f64> = trace.iter().map(|&(_, c)| c).collect();
    let max = c.iter().fold(0.0f64, |m, &v| m.max(v.abs()));
    let mean = c.iter().sum::<f64>() / c.len() as f64;
    let cycles = upward_mean_crossings(&c, 100);
    println!("wow: peak {max:.1} cents, mean {mean:.2}, {cycles} cycles in 9.5 s");
    assert!(
        (12.0..=40.0).contains(&max),
        "wow peak deviation {max} cents"
    );
    assert!(
        mean.abs() < 4.0,
        "wow shifts the average pitch by {mean} cents"
    );
    assert!((3..=8).contains(&cycles), "{cycles} wow cycles in 9.5 s");
}

#[test]
fn flutter_is_fast_pitch_modulation_of_a_few_cents() {
    let trace = cents_of_each_cycle_of_a_1khz_tone(only("flutter", 1.0), 3);
    let c: Vec<f64> = trace.iter().map(|&(_, c)| c).collect();
    let max = c.iter().fold(0.0f64, |m, &v| m.max(v.abs()));
    let span = trace.last().unwrap().0 - trace[0].0;
    let rate = upward_mean_crossings(&c, 9) as f64 / span;
    println!("flutter: peak {max:.1} cents, {rate:.1} Hz");
    assert!(
        (4.0..=15.0).contains(&max),
        "flutter peak deviation {max} cents"
    );
    assert!((6.0..=10.0).contains(&rate), "flutter rate {rate} Hz");
}

#[test]
fn wow_keeps_the_level() {
    let n = FS as usize;
    let input = sine(1000.0, 0.5, 4 * n);
    let mut fx = Fx::new(3);
    fx.set(sid_fx_params {
        wow: 1.0,
        flutter: 1.0,
        ..off()
    });
    let out = fx.run(&input);
    let level = db(ac_rms(&out[n..]) / ac_rms(&input[n..]));
    assert!(
        level.abs() < 0.2,
        "wow and flutter change the level by {level} dB"
    );
}

fn two_one_pole_cascade_minus_3db_hz(a: f64) -> f64 {
    let b = 1.0 - a;
    let cos_w = (1.0 + b * b - a * a * 2f64.sqrt()) / (2.0 * b);
    cos_w.acos() * FSF / (2.0 * PI)
}

#[test]
fn crt_tone_corner_is_where_the_filter_says() {
    let n = FS as usize;
    let mut corners = Vec::new();
    for amount in [0.5f32, 1.0] {
        let mut fx = Fx::new(1);
        fx.set(only("crt", amount));
        fx.run(&vec![0; n]);
        let fc = two_one_pole_cascade_minus_3db_hz(f64::from(fx.0.crt.a_lp));
        let mut gain = |hz: f64| {
            let input = sine(hz, 0.5, n);
            let out = fx.run(&input);
            db(tone_amp(&out[n / 2..], hz) / tone_amp(&input[n / 2..], hz))
        };
        let at_corner = gain(fc);
        let mid = gain(fc / 4.0);
        let low = gain(40.0);
        println!(
            "crt {amount}: corner {fc:.0} Hz, {at_corner:.2} dB there, {mid:.2} dB at fc/4, \
             {low:.2} dB at 40 Hz"
        );
        assert!(
            (at_corner + 3.0).abs() < 0.3,
            "{at_corner} dB at the corner"
        );
        assert!(mid > -1.0, "{mid} dB at fc/4");
        if amount == 1.0 {
            assert!(
                low < -10.0,
                "the TV-speaker high-pass passes {low} dB at 40 Hz"
            );
        }
        corners.push(fc);
    }
    assert!(
        corners[1] < corners[0],
        "more CRT does not tighten the low-pass"
    );
    let full = corners[1];
    assert!(
        (2000.0..=3500.0).contains(&full),
        "full CRT corner at {full} Hz"
    );
}

#[test]
fn crt_cascade_follows_the_log_mapping() {
    let n = FS as usize;
    for tone in [0.25f32, 0.3, 0.5, 0.75, 1.0] {
        let t = f64::from(tone);
        let f_lp =
            SID_FX_CRT_LP_MIN_HZ * (SID_FX_CRT_LP_MAX_HZ / SID_FX_CRT_LP_MIN_HZ).powf(1.0 - t);
        let f_hp = SID_FX_CRT_HP_MIN_HZ * (SID_FX_CRT_HP_MAX_HZ / SID_FX_CRT_HP_MIN_HZ).powf(t);
        let corner = two_one_pole_cascade_minus_3db_hz(1.0 - (-2.0 * PI * f_lp / FSF).exp());
        let mut fx = Fx::new(1);
        fx.set(only("crt", tone));
        fx.run(&vec![0; n]);
        let mut gain = |hz: f64| {
            let input = sine(hz, 0.5, n);
            let out = fx.run(&input);
            db(tone_amp(&out[n / 2..], hz) / tone_amp(&input[n / 2..], hz))
        };
        let (at_lp, at_hp) = (gain(corner), gain(f_hp));
        println!(
            "crt tone {tone}: low-pass cascade -3 dB at {corner:.0} Hz measures {at_lp:.2} dB, \
             high-pass pole {f_hp:.1} Hz measures {at_hp:.2} dB"
        );
        assert!(
            (at_lp + 3.0).abs() < 0.2,
            "tone {tone}: {at_lp} dB at {corner} Hz"
        );
        assert!(
            (at_hp + 3.0).abs() < 0.3,
            "tone {tone}: {at_hp} dB at {f_hp} Hz"
        );
    }
}

#[test]
fn crt_hum_is_quiet_50_hz_with_harmonics() {
    let n = 2 * FS as usize;
    let mut fx = Fx::new(1);
    fx.set(only("crt", 1.0));
    let out = fx.run(&vec![0; n]);
    let tail = &out[n / 2..];
    let f0 = estimate_f0(&to_f32(tail), FSF, 30.0, 80.0);
    let fund = tone_amp(tail, SID_FX_HUM_HZ);
    let expect = SID_FX_HUM_LEVEL * 32768.0;
    println!("hum: {f0:.2} Hz, {fund:.1} LSB (expected {expect:.1})");
    assert!((f0 - 50.0).abs() < 0.2, "hum at {f0} Hz");
    assert!(
        (fund / expect - 1.0).abs() < 0.1,
        "hum fundamental {fund} LSB"
    );
    assert!(db(fund / 32768.0) < -40.0, "hum is not quiet");
    for k in [2.0, 3.0] {
        assert!(tone_amp(tail, 50.0 * k) > 0.1 * fund, "no harmonic {k}");
    }
}

#[test]
fn noise_has_its_level_and_is_hiss_tilted_up_otherwise_white() {
    let n = 2 * FS as usize;
    let level = |amount: f32| {
        let mut fx = Fx::new(11);
        fx.set(only("noise", amount));
        fx.run(&vec![0; n])[n / 2..].to_vec()
    };
    let full = level(1.0);
    let half = level(0.5);
    let r = ac_rms(&full);
    let expect = SID_FX_NOISE_RMS * 32768.0;
    println!(
        "noise: {r:.1} LSB RMS (expected {expect:.1}), {:.1} dBFS",
        db(r / 32768.0)
    );
    assert!(db(r / expect).abs() < 0.5, "noise RMS {r}");
    let ratio = db(ac_rms(&half) / r);
    assert!((ratio + 6.02).abs() < 0.5, "half noise is {ratio} dB");

    let x: Vec<f64> = full.iter().map(|&v| f64::from(v)).collect();
    let corr = |lag: usize| {
        let num: f64 = x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
        num / x.iter().map(|a| a * a).sum::<f64>()
    };
    let rho1 = corr(1);
    println!("noise: lag-1 correlation {rho1:.3}");
    assert!((-0.5..=-0.35).contains(&rho1), "lag-1 correlation {rho1}");
    for lag in [2, 3, 10, 441] {
        assert!(
            corr(lag).abs() < 0.02,
            "lag-{lag} correlation {}",
            corr(lag)
        );
    }
}

#[test]
fn crush_quantises_and_decimates() {
    let n = FS as usize;
    let input = sine(100.0, 0.9, 2 * n);
    let mut fx = Fx::new(1);
    fx.set(only("crush", 1.0));
    let out = fx.run(&input);
    let tail = &out[n..];
    let mut levels: Vec<i16> = tail.to_vec();
    levels.sort_unstable();
    levels.dedup();
    let bits = SID_FX_CRUSH_MIN_BITS as i32;
    let step = 1i32 << (16 - bits);
    println!("crush: {} levels", levels.len());
    assert!(levels.len() <= (1 << bits) + 1, "{} levels", levels.len());
    assert!(levels.len() >= 1 << (bits - 1), "{} levels", levels.len());
    assert!(
        levels.iter().all(|&v| i32::from(v) % step == 0),
        "{levels:?}"
    );

    let ramp: Vec<i16> = (0..2 * n).map(|i| (i as i32 * 500) as i16).collect();
    let out = fx.run(&ramp);
    let runs = out[n..].windows(2).filter(|w| w[0] != w[1]).count();
    let hold = n as f64 / runs as f64;
    let expect = FSF / SID_FX_CRUSH_MIN_RATE;
    println!("crush: held {hold:.2} samples (expected {expect:.2})");
    assert!((hold / expect - 1.0).abs() < 0.1, "held {hold} samples");

    let mut fx = Fx::new(1);
    fx.set(only("crush", 0.5));
    let out = fx.run(&input);
    let half_step = 1i32 << ((16 - bits) / 2);
    assert!(out[n..].iter().all(|&v| i32::from(v) % half_step == 0));
}

#[test]
fn extreme_settings_stay_stable() {
    let n = 10 * FS as usize;
    let mut input: Vec<i16> = (0..n)
        .map(|i| {
            if (i / 441) % 2 == 0 {
                i16::MAX
            } else {
                i16::MIN
            }
        })
        .collect();
    input[n / 2..].copy_from_slice(&xorshift_noise(n - n / 2, 3));
    let mut fx = Fx::new(1);
    fx.set(sid_fx_params {
        tape_drive: 7.0,
        wow: f32::NAN,
        flutter: -1.0,
        noise: f32::INFINITY,
        crt: 1.0,
        crush: 2.0,
    });
    let t = fx.0.target;
    assert_eq!(
        [t.tape_drive, t.wow, t.flutter, t.noise, t.crt, t.crush],
        [1.0, 0.0, 0.0, 1.0, 1.0, 1.0]
    );
    let mut s = 1u32;
    for block in input.chunks(37) {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let pick = |k: u32| if (s >> k) & 1 == 1 { 1.0 } else { 0.0 };
        fx.set(sid_fx_params {
            tape_drive: pick(10),
            wow: pick(11),
            flutter: pick(12),
            noise: pick(13),
            crt: pick(14),
            crush: pick(15),
        });
        let mut io = block.to_vec();
        fx.process(&mut io);
    }
    let f = &fx.0;
    for v in [
        f.tape.lp_pre,
        f.tape.lp_de,
        f.tape.dc_x,
        f.tape.dc_y,
        f.tape.q,
        f.tape.deemph,
        f.crt.lp1,
        f.crt.lp2,
        f.crt.hp_y,
        f.wow.drift1,
        f.wow.drift2,
    ] {
        assert!(v.is_finite() && v.abs() < 100.0, "state value {v}");
    }
    assert!(f.wow.buf.iter().all(|v| v.is_finite()));
    assert!((0.0..SID_FX_WOW_LEN as f32 - 2.0).contains(&f.wow.delay));

    fx.set(sid_fx_params {
        noise: 0.0,
        crt: 0.0,
        ..all_on()
    });
    let out = fx.run(&vec![0; 2 * FS as usize]);
    assert!(
        out[FS as usize..].iter().all(|&v| v == 0),
        "silence does not stay silent"
    );
}

#[test]
fn smoothing_takes_about_10_ms() {
    let mut fx = Fx::new(1);
    fx.set(only("crt", 1.0));
    fx.run(&vec![0; 441]);
    let c = fx.0.cur.crt;
    assert!(
        (c - 0.632).abs() < 0.02,
        "after 10 ms the CRT amount is {c}"
    );
    for v in [1.0, 0.3, 0.0] {
        fx.set(only("crt", v));
        fx.run(&vec![0; FS as usize / 4]);
        assert_eq!(fx.0.cur.crt, v);
    }
}

#[test]
fn same_seed_same_output_and_block_size_does_not_matter() {
    let n = FS as usize;
    let input = sine(330.0, 0.6, 4 * n);
    let render = |seed: u32, block: usize| {
        let mut fx = Fx::new(seed);
        let mut out = input.clone();
        for (k, part) in out.chunks_mut(n).enumerate() {
            fx.set(if k == 1 { off() } else { all_on() });
            for b in part.chunks_mut(block) {
                fx.process(b);
            }
            if k == 1 {
                assert_eq!(fx.0.active, 0, "not idle before the restart");
            }
        }
        out
    };
    let reference = render(42, 128);
    assert_eq!(render(42, 128), reference);
    for block in [1, 77, 1000] {
        let other = render(42, block);
        let diff = other.iter().zip(&reference).filter(|(a, b)| a != b).count();
        assert_eq!(diff, 0, "block size {block}: {diff} samples differ");
    }
    assert_ne!(render(43, 128), reference);
}

fn to_i16_like_sid_fx(x: f32) -> i16 {
    let mut o = x * 32768.0;
    o += if o >= 0.0 { 0.5 } else { -0.5 };
    o.clamp(-32768.0, 32767.0) as i16
}

fn smooth_residual(name: &str, click: Option<usize>) -> i32 {
    let input = sine(110.0, 0.5, T_END);
    let run = |ramp: bool| {
        let mut fx = Fx::new(1);
        let mut out = Vec::with_capacity(T_END);
        for (b, block) in input.chunks(16).enumerate() {
            let t = b * 16;
            let v = if ramp {
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
            if t >= T_ON {
                fx.set(only(name, v));
            }
            let mut io = block.to_vec();
            fx.process(&mut io);
            out.extend(io);
        }
        out
    };
    let j = run(false);
    let r = run(true);
    let mut diff = difference(&j, &r);
    if let Some(at) = click {
        for d in &mut diff[at..] {
            *d += CLICK;
        }
    }
    max_second_diff(&diff)
}

#[test]
fn parameter_jumps_leave_no_click_in_tape_wow_flutter_and_crt() {
    for name in ["tape_drive", "wow", "flutter", "crt"] {
        let m = smooth_residual(name, None);
        println!("{name}: residual 2nd difference {m} LSB");
        assert!(m <= RESIDUAL_MAX, "{name}: residual {m} LSB");
        for at in [T_ON + 10, T_OFF + 10] {
            let c = smooth_residual(name, Some(at));
            assert!(
                c > RESIDUAL_MAX,
                "{name}: a {CLICK} LSB click shows only {c}"
            );
        }
    }
}

fn per_sample(
    fx: &mut Fx,
    input: &[i16],
    mut schedule: impl FnMut(&mut Fx, usize),
    mut probe: impl FnMut(&Fx, usize, i16, bool),
) {
    for (n, &x) in input.iter().enumerate() {
        schedule(fx, n);
        let before = (fx.0.active, fx.0.crush.phase);
        let mut io = [x];
        fx.process(&mut io);
        let captured = before.0 & 1 == 0 || fx.0.crush.phase < before.1;
        probe(fx, n, io[0], captured);
    }
}

fn jump(name: &'static str) -> impl FnMut(&mut Fx, usize) {
    move |fx, n| {
        if n == T_ON {
            fx.set(only(name, 1.0));
        } else if n == T_OFF {
            fx.set(off());
        }
    }
}

#[test]
fn noise_jumps_leave_no_click() {
    let silence = vec![0i16; T_END];
    let mut h = Vec::new();
    let mut fx = Fx::new(9);
    per_sample(
        &mut fx,
        &silence,
        |fx, n| {
            if n == T_ON {
                fx.0.target.noise = 1.0;
                fx.0.cur.noise = 1.0;
            }
        },
        |_, _, y, _| h.push(i32::from(y)),
    );
    let mut quiet = Vec::new();
    let mut amounts = Vec::new();
    let mut fx = Fx::new(9);
    per_sample(&mut fx, &silence, jump("noise"), |fx, _, y, _| {
        quiet.push(i32::from(y));
        amounts.push(f64::from(fx.0.cur.noise));
    });
    let model = (0..T_END)
        .map(|n| (quiet[n] - (amounts[n] * f64::from(h[n])).round() as i32).abs())
        .max()
        .unwrap();
    let tone = sine(440.0, 0.5, T_END);
    let mut loud = Vec::new();
    let mut fx = Fx::new(9);
    per_sample(&mut fx, &tone, jump("noise"), |_, _, y, _| {
        loud.push(i32::from(y))
    });
    let superposition = (0..T_END)
        .map(|n| (loud[n] - quiet[n] - i32::from(tone[n])).abs())
        .max()
        .unwrap();
    println!("noise: residual vs amount x hiss {model} LSB, superposition {superposition} LSB");
    assert!(model <= 1, "noise: {model} LSB off the smoothed hiss");
    assert!(
        superposition <= 1,
        "noise: the tone is off by {superposition} LSB"
    );
    assert!(amounts[T_ON + 441] > 0.6 && amounts[T_ON + 441] < 0.66);
}

fn crush_model(x: f32, bits: f32) -> f32 {
    let e = bits * (16 - SID_FX_CRUSH_MIN_BITS) as f32;
    let i = e as i32;
    let f = e - i as f32;
    let step = (1u32 << i) as f32 * (1.0 + f * (0.6565 + 0.3435 * f)) * (1.0 / 32768.0);
    let inv = (32768u32 >> i) as f32 * (1.0 - f * (0.6564 - 0.1564 * f));
    let mut v = x * inv;
    v += if v >= 0.0 { 0.5 } else { -0.5 };
    (v as i32) as f32 * step
}

#[test]
fn crush_jumps_leave_no_click() {
    let tone = sine(440.0, 0.5, T_END);
    let mut hold = 0.0f32;
    let mut worst = 0;
    let mut fx = Fx::new(1);
    per_sample(&mut fx, &tone, jump("crush"), |fx, n, y, captured| {
        let bits = fx.0.cur.crush;
        let expect = if bits == 0.0 {
            tone[n]
        } else {
            if captured {
                hold = crush_model(f32::from(tone[n]) / 32768.0, bits);
            }
            to_i16_like_sid_fx(hold)
        };
        worst = worst.max((i32::from(y) - i32::from(expect)).abs());
    });
    println!("crush: residual vs the sample-and-hold model {worst} LSB");
    assert_eq!(worst, 0, "crush: {worst} LSB off the sample-and-hold model");
}

#[test]
fn wow_switched_off_or_changed_does_not_swoop() {
    let n = FS as usize;
    let tone = sine(1000.0, 0.5, 6 * n);
    let mut fx = Fx::new(3);
    fx.set(sid_fx_params {
        wow: 1.0,
        flutter: 1.0,
        ..off()
    });
    let mut out = fx.run(&tone[..3 * n]);
    fx.set(sid_fx_params {
        wow: 0.3,
        flutter: 1.0,
        ..off()
    });
    out.extend(fx.run(&tone[3 * n..4 * n]));
    fx.set(off());
    out.extend(fx.run(&tone[4 * n..]));
    let trace: Vec<(f64, f64)> = cycle_frequencies(&out)
        .into_iter()
        .map(|(t, f)| (t, cents(f, 1000.0)))
        .collect();
    let window = |from: f64, to: f64| -> Vec<f64> {
        trace
            .iter()
            .filter(|&&(t, _)| t >= from && t < to)
            .map(|&(_, c)| c)
            .collect()
    };
    let mean = |c: &[f64]| c.iter().sum::<f64>() / c.len() as f64;
    let max = |c: &[f64]| c.iter().fold(0.0f64, |m, &v| m.max(v.abs()));

    let changed = window(3.0, 3.5);
    let off_c = window(4.0, 4.5);
    let off_far = window(4.25, 6.0);
    let off_wild = off_c.iter().filter(|c| c.abs() > 15.0).count();
    println!(
        "wow 1 -> 0.3: mean {:.2}, peak {:.1} cents; off: mean {:.2}, peak {:.1} cents, \
         {off_wild} cycles over 15 cents, then peak {:.3}",
        mean(&changed),
        max(&changed),
        mean(&off_c),
        max(&off_c),
        max(&off_far)
    );
    assert!(
        mean(&changed).abs() < 6.0,
        "a depth change shifts the pitch"
    );
    assert!(max(&changed) < 30.0, "a depth change swoops");
    assert!(mean(&off_c).abs() < 3.0, "switching off shifts the pitch");
    assert!(off_wild <= 20, "{off_wild} ms over 15 cents switching off");
    assert!(max(&off_far) < 0.5, "not dry 0.25 s after switching off");
}

#[test]
fn blocks_accept_any_amount_on_any_sample() {
    let mut s = 12345u32;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let mut amount = || match rnd() % 4 {
        0 => 0.0f32,
        1 => 1.0,
        _ => (rnd() >> 8) as f32 / 16_777_216.0,
    };
    let mut xs = xorshift_noise(200_000, 77)
        .into_iter()
        .map(|v| f32::from(v) / 32768.0);
    let mut x = || xs.next().unwrap();
    let ok = |y: f32| y.is_finite() && y.abs() < 4.0;

    let mut tape = sid_fx_tape::default();
    let mut crush = sid_fx_crush::default();
    let mut wow = Box::<sid_fx_wow>::default();
    let mut crt = sid_fx_crt::default();
    let mut noise = sid_fx_noise::default();
    unsafe {
        sid_fx_tape_init(&mut tape, FS);
        sid_fx_tape_reset(&mut tape);
        assert!(ok(sid_fx_tape_tick(&mut tape, 0.9, 1.0)));
        sid_fx_crush_init(&mut crush, FS);
        sid_fx_wow_init(&mut *wow, FS, 5);
        sid_fx_crt_init(&mut crt, FS);
        sid_fx_noise_init(&mut noise, 5);
        for i in 0..40_000 {
            let y = sid_fx_tape_tick(&mut tape, x(), amount());
            assert!(ok(y), "tape {y} at sample {i}");
            let slope = 1.0 - tape.amount * (4.0 / 9.0) * tape.bias * tape.bias;
            let full = tape.drive * slope * (1.0 + tape.g);
            assert!((tape.q * full - 1.0).abs() < 1e-4);
            assert!((tape.deemph * (1.0 + tape.g) - 1.0).abs() < 1e-4);
            let y = sid_fx_crush_tick(&mut crush, x(), amount(), amount());
            assert!(ok(y), "crush {y} at sample {i}");
            let y = sid_fx_wow_tick(&mut *wow, x(), amount(), amount());
            assert!(ok(y), "wow {y} at sample {i}");
            assert!(wow.delay >= 0.0 && wow.delay <= (SID_FX_WOW_LEN - 2) as f32);
            let y = sid_fx_crt_tick(&mut crt, x(), amount(), amount());
            assert!(ok(y), "crt {y} at sample {i}");
            let y = sid_fx_noise_tick(&mut noise, x(), amount());
            assert!(ok(y), "noise {y} at sample {i}");
        }
    }
}

struct Aliases {
    at_4k1: f64,
    worst: f64,
    worst_below_15k: f64,
    total: f64,
}

fn tape_aliases_of_a_half_scale_8khz_tone(drive: f32) -> Aliases {
    let n = FS as usize;
    let mut fx = Fx::new(1);
    fx.set(only("tape_drive", drive));
    let out = fx.run(&sine(8000.0, 0.5, 2 * n));
    let t = &out[n..];
    let f1 = tone_amp(t, 8000.0);
    let mut power = 0.0;
    let (mut worst, mut worst_below_15k) = (-300.0f64, -300.0f64);
    for k in 3..=40 {
        let mut f = (8000.0 * k as f64) % FSF;
        if f > FSF / 2.0 {
            f = FSF - f;
        }
        let a = tone_amp(t, f);
        power += a * a;
        worst = worst.max(db(a / f1));
        if f < 15000.0 {
            worst_below_15k = worst_below_15k.max(db(a / f1));
        }
    }
    Aliases {
        at_4k1: db(tone_amp(t, 4100.0) / f1),
        worst,
        worst_below_15k,
        total: db(power.sqrt() / f1),
    }
}

#[test]
fn tape_aliasing_is_held_down() {
    for drive in [0.5, 1.0] {
        let a = tape_aliases_of_a_half_scale_8khz_tone(drive);
        println!(
            "tape drive {drive}: 4.1 kHz {:.1} dBc, worst {:.1} dBc, worst below 15 kHz {:.1} dBc, \
             total {:.1} dBc",
            a.at_4k1, a.worst, a.worst_below_15k, a.total
        );
        assert!(
            a.at_4k1 < -50.0,
            "drive {drive}: 4.1 kHz alias at {} dBc",
            a.at_4k1
        );
        assert!(
            a.worst_below_15k < -40.0,
            "drive {drive}: alias below 15 kHz at {} dBc",
            a.worst_below_15k
        );
        assert!(
            a.total < -14.0,
            "drive {drive}: aliases total {} dBc",
            a.total
        );
    }
}

fn worst_delay_read_errors_on_a_3khz_tone() -> [f64; 4] {
    const LEN: usize = 1024;
    let hz = 3000.0;
    let tone = |t: f64| 0.5 * (2.0 * PI * hz * t / FSF).sin();
    let mut fbuf = [0.0f32; LEN];
    let mut ibuf = [0i16; LEN];
    let mut fd = sid_fx_delay::default();
    let mut id = sid_fx_delay::default();
    let mut err = [0.0f64; 4];
    unsafe {
        sid_fx_delay_init(&mut fd, LEN as u32);
        sid_fx_delay_init(&mut id, LEN as u32);
        for i in 0..2000 {
            let x = tone(i as f64) as f32;
            sid_fx_delay_push(&mut fd, fbuf.as_mut_ptr(), x);
            sid_fx_delay16_push(&mut id, ibuf.as_mut_ptr(), x);
            if i < LEN {
                continue;
            }
            for k in 0..20 {
                let d = 5.0 + k as f32 * 0.05 + (i % 7) as f32;
                let want = tone(i as f64 - f64::from(d));
                let got = [
                    sid_fx_delay_read(&fd, fbuf.as_ptr(), d),
                    sid_fx_delay_read_hermite(&fd, fbuf.as_ptr(), d),
                    sid_fx_delay16_read(&id, ibuf.as_ptr(), d),
                    sid_fx_delay16_read_hermite(&id, ibuf.as_ptr(), d),
                ];
                for (e, g) in err.iter_mut().zip(got) {
                    *e = e.max((f64::from(g) - want).abs());
                }
            }
        }
    }
    err
}

#[test]
fn delay_line_reads_integer_and_fractional_delays() {
    let mut buf = [0.0f32; 16];
    let mut ibuf = [0i16; 16];
    let mut d = sid_fx_delay::default();
    let mut di = sid_fx_delay::default();
    unsafe {
        sid_fx_delay_init(&mut d, 16);
        sid_fx_delay_init(&mut di, 16);
        for i in 0..20 {
            sid_fx_delay_push(&mut d, buf.as_mut_ptr(), i as f32);
            sid_fx_delay16_push(&mut di, ibuf.as_mut_ptr(), i as f32 / 1024.0);
        }
    }
    let read = |delay: f32| unsafe { sid_fx_delay_read(&d, buf.as_ptr(), delay) };
    let hermite = |delay: f32| unsafe { sid_fx_delay_read_hermite(&d, buf.as_ptr(), delay) };
    let read16 = |delay: f32| unsafe { sid_fx_delay16_read(&di, ibuf.as_ptr(), delay) };
    assert_eq!(read(0.0), 19.0);
    assert_eq!(read(3.0), 16.0);
    assert_eq!(read(2.25), 16.75);
    assert_eq!(read(14.0), 5.0);
    assert_eq!(hermite(3.0), 16.0);
    assert_eq!(hermite(2.25), 16.75);
    assert_eq!(hermite(13.0), 6.0);
    assert_eq!(read16(2.5) * 1024.0, 16.5);

    let [lin, her, lin16, her16] = worst_delay_read_errors_on_a_3khz_tone();
    let lsb = 1.0 / 32768.0;
    println!(
        "3 kHz delay read error: linear {:.1} LSB, hermite {:.2} LSB, int16 linear {:.1} LSB, \
         int16 hermite {:.2} LSB",
        lin / lsb,
        her / lsb,
        lin16 / lsb,
        her16 / lsb
    );
    assert!(
        her < lin / 10.0,
        "hermite {her} is not far better than linear {lin}"
    );
    assert!(her < 32.0 * lsb, "hermite error {her}");
    assert!(lin16 < lin + 1.5 * lsb && her16 < her + 1.5 * lsb);
}

#[test]
fn small_signals_stay_flat_at_every_drive() {
    let n = FS as usize;
    for drive in [0.25f32, 0.5, 0.75, 1.0] {
        let mut worst = 0.0f64;
        for hz in [100.0, 1000.0, 4000.0, 8000.0, 12000.0, 16000.0] {
            let input = sine(hz, 0.003, 2 * n);
            let mut fx = Fx::new(1);
            fx.set(only("tape_drive", drive));
            let out = fx.run(&input);
            let g = db(tone_amp(&out[n..], hz) / tone_amp(&input[n..], hz));
            worst = if g.abs() > worst.abs() { g } else { worst };
        }
        println!("tape drive {drive}: small-signal gain within {worst:.3} dB to 16 kHz");
        assert!(worst.abs() < 0.1, "drive {drive}: {worst} dB");
    }
}

#[test]
fn wow_fills_then_crossfades_without_a_corner() {
    let slope = 1e-4f32;
    let mut w = Box::<sid_fx_wow>::default();
    unsafe { sid_fx_wow_init(&mut *w, FS, 5) };
    let centre = w.centre_samples as usize;
    let fade = (SID_FX_WOW_FADE_MS * 0.001 * FSF) as usize;
    let on_until = [
        (0, true),
        (centre + fade / 2, false),
        (centre + fade, true),
        (centre + 3 * fade, false),
    ];
    let total = centre + 5 * fade;
    let mut env = Vec::with_capacity(total);
    for i in 0..total {
        let on = on_until.iter().rev().find(|&&(t, _)| i >= t).unwrap().1;
        let depth = if on { 1e-6 } else { 0.0 };
        let x = i as f32 * slope;
        let y = unsafe { sid_fx_wow_tick(&mut *w, x, depth, depth) };
        if i < centre {
            assert_eq!(y, x, "sample {i}: not dry while the line fills");
        }
        env.push(f64::from(x - y) / (f64::from(slope) * centre as f64));
    }
    let peak = env.iter().fold(0.0f64, |m, &v| m.max(v));
    let bend = env
        .windows(3)
        .map(|e| (e[2] - 2.0 * e[1] + e[0]).abs())
        .fold(0.0f64, f64::max);
    println!("wow crossfade: peak {peak:.3}, worst envelope 2nd difference {bend:.2e}");
    assert!(peak > 0.99, "the fade never completed ({peak})");
    assert!(env[total - 1].abs() < 1e-6, "not back to dry");
    assert!(bend < 3e-4, "the crossfade envelope bends by {bend}");
}
