// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The post-chip colour chain (`core/fx`): exact bypass, each effect's characteristic,
//! stability, smoothing and determinism.

use std::f64::consts::PI;

use sidou_sys::*;
use sidou_tests::signal_metrics::estimate_f0;

const FS: u32 = 44100;
const FSF: f64 = FS as f64;

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

    /// Runs `input` through in blocks of 128, as the firmware does.
    fn run(&mut self, input: &[i16]) -> Vec<i16> {
        let mut out = input.to_vec();
        for block in out.chunks_mut(128) {
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

/// `p` with only the field `name` set to `v`.
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

fn sine(hz: f64, amp: f64, n: usize) -> Vec<i16> {
    (0..n)
        .map(|i| (amp * 32767.0 * (2.0 * PI * hz * i as f64 / FSF).sin()).round() as i16)
        .collect()
}

/// Deterministic full-range test noise (xorshift32), independent of the C code.
fn test_noise(n: usize, mut s: u32) -> Vec<i16> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 16) as u16 as i16
        })
        .collect()
}

fn to_f32(x: &[i16]) -> Vec<f32> {
    x.iter().map(|&v| f32::from(v) / 32768.0).collect()
}

/// Amplitude (in LSB) of the component at `hz`, by projection onto sin and cos.
fn tone_amp(x: &[i16], hz: f64) -> f64 {
    let w = 2.0 * PI * hz / FSF;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        re += f64::from(v) * (w * i as f64).cos();
        im += f64::from(v) * (w * i as f64).sin();
    }
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

fn rms(x: &[i16]) -> f64 {
    let mean = x.iter().map(|&v| f64::from(v)).sum::<f64>() / x.len() as f64;
    (x.iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / x.len() as f64)
        .sqrt()
}

fn db(r: f64) -> f64 {
    20.0 * r.log10()
}

fn peak(x: &[i16]) -> i32 {
    x.iter().map(|&v| i32::from(v).abs()).max().unwrap_or(0)
}

fn max_step(x: &[i16]) -> i32 {
    x.windows(2)
        .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
        .max()
        .unwrap_or(0)
}

/// Total harmonic distortion of a tone at `hz` (harmonics 2 to 10, amplitude ratio).
fn thd(x: &[i16], hz: f64) -> f64 {
    let h: f64 = (2..=10)
        .map(|k| tone_amp(x, hz * k as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    h / tone_amp(x, hz)
}

/// Frequency of each cycle from interpolated upward zero crossings, as (time s, Hz).
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

/// Times the moving average of `trace` crosses its mean upwards.
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
    let mut input = test_noise(4096, 7);
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
    let input = test_noise(4096, 9);
    assert_eq!(fx.run(&input), input);
    assert_eq!(fx.bytes(), before);
}

#[test]
fn one_effect_on_leaves_the_others_out_of_the_path() {
    // Noise alone on silence is only the hiss; on a signal it is signal + hiss.
    let mut a = Fx::new(5);
    let mut b = Fx::new(5);
    a.set(only("noise", 1.0));
    b.set(only("noise", 1.0));
    let n = FS as usize;
    let hiss = a.run(&vec![0; n]);
    let input = sine(440.0, 0.25, n);
    let both = b.run(&input);
    // Rounding the sum can differ by one from summing the rounded parts.
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
    let hz = 441.0; // 100 samples per cycle
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
        // Asymmetric: even harmonics, not just odd.
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
        let level = db(rms(&out[n..]) / rms(&input[n..]));
        println!("tape amp {amp}: peak gain {gain:.3}, level {level:.2} dB");
        assert!(gain < 1.25, "peak gain {gain} at amplitude {amp}");
        // Up to SID-like levels (the chip peaks well below 0.3 full scale) loudness holds;
        // louder input is compressed, which is the point.
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
    let gain = |hz: f64| {
        let input = sine(hz, 0.7, 2 * n);
        let mut fx = Fx::new(1);
        fx.set(only("tape_drive", 1.0));
        let out = fx.run(&input);
        tone_amp(&out[n..], hz) / tone_amp(&input[n..], hz)
    };
    let (low, high) = (gain(200.0), gain(8000.0));
    println!("tape fundamental gain: 200 Hz {low:.3}, 8 kHz {high:.3}");
    assert!(
        high < low * 0.97,
        "highs ({high}) are not compressed more than lows ({low})"
    );
}

/// Pitch deviation in cents of a 1 kHz tone through the chain, one value per cycle.
fn pitch_trace(p: sid_fx_params, seconds: usize) -> Vec<(f64, f64)> {
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
    let trace = pitch_trace(only("wow", 1.0), 10);
    let c: Vec<f64> = trace.iter().map(|&(_, c)| c).collect();
    let max = c.iter().fold(0.0f64, |m, &v| m.max(v.abs()));
    let mean = c.iter().sum::<f64>() / c.len() as f64;
    // About 0.5 Hz: 4 to 5 cycles in the 9.5 s window, give or take the drift.
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
    let trace = pitch_trace(only("flutter", 1.0), 3);
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
    let level = db(rms(&out[n..]) / rms(&input[n..]));
    assert!(
        level.abs() < 0.2,
        "wow and flutter change the level by {level} dB"
    );
}

/// -3 dB frequency of two cascaded one-poles `y += a (x - y)`.
fn cascade_corner(a: f64) -> f64 {
    let b = 1.0 - a;
    let cos_w = (1.0 + b * b - a * a * 2f64.sqrt()) / (2.0 * b);
    cos_w.acos() * FSF / (2.0 * PI)
}

#[test]
fn crt_low_pass_corner_is_where_the_filter_says() {
    let n = FS as usize;
    let mut corners = Vec::new();
    for amount in [0.5f32, 1.0] {
        let mut fx = Fx::new(1);
        fx.set(only("crt", amount));
        fx.run(&vec![0; n]); // settle
        let a = 1.0 - f64::from(amount) * (1.0 - f64::from(fx.0.crt.a_min));
        let fc = cascade_corner(a);
        let mut gain = |hz: f64| {
            let input = sine(hz, 0.5, n);
            let out = fx.run(&input);
            db(tone_amp(&out[n / 2..], hz) / tone_amp(&input[n / 2..], hz))
        };
        let at_corner = gain(fc);
        let low = gain(fc / 8.0);
        println!("crt {amount}: corner {fc:.0} Hz, {at_corner:.2} dB there, {low:.2} dB at fc/8");
        assert!(
            (at_corner + 3.0).abs() < 0.3,
            "{at_corner} dB at the corner"
        );
        assert!(low > -0.5, "{low} dB an octave-ish below");
        corners.push(fc);
    }
    assert!(
        corners[1] < corners[0],
        "more CRT does not tighten the low-pass"
    );
    let full = corners[1];
    assert!(
        (2000.0..=5000.0).contains(&full),
        "full CRT corner at {full} Hz"
    );
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
fn noise_level_and_spectrum() {
    let n = 2 * FS as usize;
    let level = |amount: f32| {
        let mut fx = Fx::new(11);
        fx.set(only("noise", amount));
        fx.run(&vec![0; n])[n / 2..].to_vec()
    };
    let full = level(1.0);
    let half = level(0.5);
    let r = rms(&full);
    let expect = SID_FX_NOISE_RMS * 32768.0;
    println!(
        "noise: {r:.1} LSB RMS (expected {expect:.1}), {:.1} dBFS",
        db(r / 32768.0)
    );
    assert!(db(r / expect).abs() < 0.5, "noise RMS {r}");
    let ratio = db(rms(&half) / r);
    assert!((ratio + 6.02).abs() < 0.5, "half noise is {ratio} dB");

    let x: Vec<f64> = full.iter().map(|&v| f64::from(v)).collect();
    let corr = |lag: usize| {
        let num: f64 = x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
        num / x.iter().map(|a| a * a).sum::<f64>()
    };
    // Shaped hiss: tilted up, so the lag-1 correlation is clearly negative...
    let rho1 = corr(1);
    println!("noise: lag-1 correlation {rho1:.3}");
    assert!((-0.5..=-0.35).contains(&rho1), "lag-1 correlation {rho1}");
    // ...and otherwise white.
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

    // A fast ramp moves more than one level between captures, so every capture shows.
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
    input[n / 2..].copy_from_slice(&test_noise(n - n / 2, 3));
    let mut fx = Fx::new(1);
    // Out-of-range and NaN targets are clamped.
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
        f.tape.makeup,
        f.tape.deemph,
        f.crt.lp1,
        f.crt.lp2,
        f.wow.delay,
        f.wow.drift1,
        f.wow.drift2,
    ] {
        assert!(v.is_finite() && v.abs() < 100.0, "state value {v}");
    }
    assert!(f.wow.buf.iter().all(|v| v.is_finite()));

    // Everything but the additive noise and hum, all at full: silence in gives silence out.
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
    // It then lands exactly on the target, at 0 or 1 and in between.
    for v in [1.0, 0.3, 0.0] {
        fx.set(only("crt", v));
        fx.run(&vec![0; FS as usize / 4]);
        assert_eq!(fx.0.cur.crt, v);
    }
}

#[test]
fn parameter_jumps_are_zipper_free() {
    let n = FS as usize / 2;
    let input = sine(440.0, 0.5, 4 * n);
    for name in PARAMS {
        let mut fx = Fx::new(1);
        let seg = |fx: &mut Fx, v: f32, k: usize| {
            fx.set(only(name, v));
            fx.run(&input[k * n..(k + 1) * n])
        };
        // Steady off, jump on (transition + steady on), jump off (transition + steady off).
        let a = seg(&mut fx, 0.0, 0);
        let b = seg(&mut fx, 1.0, 1);
        let c = seg(&mut fx, 0.0, 2);
        let d = seg(&mut fx, 0.0, 3);
        let settle = 6000; // > 5 smoothing time constants
        let steady_on = max_step(&b[settle..]);
        let steady_off = max_step(&a).max(max_step(&d));
        let allowed = (f64::from(steady_on.max(steady_off)) * 1.1) as i32 + 32;
        let join = |x: &[i16], y: &[i16]| max_step(&[x[x.len() - 1], y[0]]);
        let on = max_step(&b[..settle]).max(join(&a, &b));
        let offs = max_step(&c[..settle]).max(join(&b, &c)).max(join(&c, &d));
        println!("{name}: steady {steady_off}/{steady_on}, jumps {on}/{offs}, allowed {allowed}");
        assert!(on <= allowed, "{name}: step {on} on the jump on");
        assert!(offs <= allowed, "{name}: step {offs} on the jump off");
    }
}

#[test]
fn same_seed_same_output_and_block_size_does_not_matter() {
    let input = sine(330.0, 0.6, FS as usize);
    let render = |seed: u32, block: usize| {
        let mut fx = Fx::new(seed);
        fx.set(all_on());
        let mut out = input.clone();
        for b in out.chunks_mut(block) {
            fx.process(b);
        }
        out
    };
    let reference = render(42, 128);
    assert_eq!(render(42, 128), reference);
    assert_eq!(render(42, 1), reference);
    assert_eq!(render(42, 77), reference);
    assert_ne!(render(43, 128), reference);
}

#[test]
fn delay_line_reads_integer_and_fractional_delays() {
    let mut buf = [0.0f32; 16];
    let mut d = sid_fx_delay::default();
    unsafe { sid_fx_delay_init(&mut d, 16) };
    for i in 0..20 {
        unsafe { sid_fx_delay_push(&mut d, buf.as_mut_ptr(), i as f32) };
    }
    let read = |delay: f32| unsafe { sid_fx_delay_read(&d, buf.as_ptr(), delay) };
    assert_eq!(read(0.0), 19.0);
    assert_eq!(read(3.0), 16.0);
    assert_eq!(read(2.25), 16.75);
    assert_eq!(read(14.0), 5.0);
}
