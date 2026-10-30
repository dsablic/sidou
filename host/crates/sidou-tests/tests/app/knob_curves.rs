// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The knob walks (app/live_walk.c, live.h "Knob walks"): for every walked parameter, the walk
//! rises with every detent (no dead detent), each detent moves what the ear hears by a step within
//! 0.5x..2x of the median one, a value off the walk moves the way the knob turned, never back, and
//! CC 0..127 reaches every position (and a position's CC comes back as itself). The effect slots'
//! shapes are measured by rendering each algorithm through the slots (core/fx), and the chosen
//! shape of each control is the most even of the three. LAB's cards and LIVE's EDIT layer turn
//! along the walks.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use sidou_sys::*;

use crate::lab_view::{lab_ui, to_page};
use crate::Rig;

const FS: u32 = 44100;

fn walk(w: u32) -> Vec<u32> {
    let n = unsafe { live_walk_count(w) };
    (0..n).map(|i| unsafe { live_walk_at(w, i) }).collect()
}

fn step(w: u32, v: u32, d: i32) -> u32 {
    unsafe { live_walk_step(w, v, d) }
}

/// Each step's size against the median step: (smallest, largest).
fn spread(measure: &[f64]) -> (f64, f64) {
    let mut steps: Vec<f64> = measure.windows(2).map(|p| (p[1] - p[0]).abs()).collect();
    let (lo, hi) = steps
        .iter()
        .fold((f64::MAX, 0.0f64), |(lo, hi), &s| (lo.min(s), hi.max(s)));
    steps.sort_by(f64::total_cmp);
    let n = steps.len();
    let median = if n % 2 == 1 {
        steps[n / 2]
    } else {
        (steps[n / 2 - 1] + steps[n / 2]) / 2.0
    };
    (lo / median, hi / median)
}

fn even((lo, hi): (f64, f64)) -> bool {
    lo >= 0.5 && hi <= 2.0
}

/// How far from even: the larger of the largest step's and the median's over the smallest's.
fn unevenness((lo, hi): (f64, f64)) -> f64 {
    hi.max(1.0 / lo)
}

// --- the parameters' own walks ---

/// (name, parameter, walk, positions, the ear's measure of a stored value, the measure's unit).
/// The measure is taken where it is defined: a value of 0 (off) has no logarithm and is left out.
type OwnEar = fn(u32) -> Option<f64>;

fn log_of(v: f64) -> Option<f64> {
    (v > 0.0).then(|| v.ln())
}

fn own_walks() -> Vec<(&'static str, u32, u32, usize, OwnEar)> {
    vec![
        // log Hz of the phase step (65536 a cycle, 100 ticks a second)
        ("PWM HZ", LIVE_PARAM_PWM_RATE, LIVE_WALK_PWM_RATE, 32, |v| {
            log_of(f64::from(v) * 100.0 / 65536.0)
        }),
        // the square law's own: the swing's square root (no rendered measure: the ear decides)
        (
            "PWM depth",
            LIVE_PARAM_PWM_DEPTH,
            LIVE_WALK_PWM_DEPTH,
            32,
            |v| Some(f64::from(v).sqrt()),
        ),
        // log Hz (0.1 Hz)
        (
            "VIB HZ",
            LIVE_PARAM_VIBRATO_RATE,
            LIVE_WALK_VIB_RATE,
            24,
            |v| log_of(f64::from(v) / 10.0),
        ),
        // cents of the swing, logarithmically (127 = +-99 cents)
        (
            "VIB DEPTH",
            LIVE_PARAM_VIBRATO_DEPTH,
            LIVE_WALK_VIB_DEPTH,
            32,
            |v| log_of(f64::from(v) * 99.0 / 127.0),
        ),
        // log ms (10 ms a tick)
        (
            "VIB DELAY",
            LIVE_PARAM_VIBRATO_DELAY,
            LIVE_WALK_VIB_DELAY,
            24,
            |v| log_of(f64::from(v) * 10.0),
        ),
    ]
}

#[test]
fn each_walk_rises_with_every_detent_and_its_steps_are_even_to_the_ear() {
    let mut report = Vec::new();
    for (name, _, w, n, ear) in own_walks() {
        let t = walk(w);
        assert_eq!(t.len(), n, "{name}: positions");
        assert!(t.windows(2).all(|p| p[0] < p[1]), "{name}: rises: {t:?}");
        let m: Vec<f64> = t.iter().filter_map(|&v| ear(v)).collect();
        let s = spread(&m);
        report.push(format!("{name}: {:.2}..{:.2}", s.0, s.1));
        assert!(
            even(s),
            "{name}: steps {:.2}..{:.2} of the median: {t:?}",
            s.0,
            s.1
        );
    }
    println!("{}", report.join("\n"));
    // The ends of the spec: PWM HZ 0.05..12.75 Hz (the parameter's 1..255), VIB HZ 0.5..12.7 Hz.
    let pr = walk(LIVE_WALK_PWM_RATE);
    assert_eq!(unsafe { live_pwm_rate(pr[0]) }, 1);
    assert_eq!(pr[31], LIVE_PWM_STEP_MAX);
    assert_eq!(walk(LIVE_WALK_VIB_RATE)[0], 5);
    assert_eq!(*walk(LIVE_WALK_VIB_RATE).last().unwrap(), 127);
    assert_eq!(
        *walk(LIVE_WALK_PWM_DEPTH).last().unwrap(),
        LIVE_PWM_DEPTH_MAX
    );
}

/// A detent from each position lands on the next one, both ways, through live_walk_turn on the
/// parameter (PWM HZ's step in live.pwm_rate); clamped at the ends.
#[test]
fn every_detent_moves_to_the_next_position() {
    for (name, p, w, _, _) in own_walks() {
        let t = walk(w);
        let mut rig = Rig::new(SID_MODEL_6581);
        let stored = |rig: &Rig| {
            if p == LIVE_PARAM_PWM_RATE {
                u32::from(rig.live.pwm_rate)
            } else {
                unsafe { live_param_get(&*rig.live, p) }
            }
        };
        // from the first position: PWM HZ's through a turn up from 0 (below the walk)
        unsafe { live_param_set(&mut *rig.live, p, 0) };
        if t[0] > 0 {
            unsafe { live_walk_turn(&mut *rig.live, p, 1) };
        }
        assert_eq!(stored(&rig), t[0], "{name}");
        for (i, &v) in t.iter().enumerate().skip(1) {
            unsafe { live_walk_turn(&mut *rig.live, p, 1) };
            assert_eq!(stored(&rig), v, "{name}: position {i}");
        }
        unsafe { live_walk_turn(&mut *rig.live, p, 1) };
        assert_eq!(stored(&rig), t[t.len() - 1], "{name}: stays at the top");
        for &v in t.iter().rev().skip(1) {
            unsafe { live_walk_turn(&mut *rig.live, p, -1) };
            assert_eq!(stored(&rig), v, "{name}: down");
        }
        unsafe { live_walk_turn(&mut *rig.live, p, -3) };
        assert_eq!(stored(&rig), t[0], "{name}: stays at the bottom");
    }
}

fn every_walk() -> Vec<u32> {
    (0..LIVE_WALKS).collect()
}

/// From any value off a walk the first detent lands on the first position beyond it the turn's way
/// (never back), and with no position that way the value stays; from a position, the next.
#[test]
fn off_the_walk_the_first_detent_moves_the_way_turned() {
    for w in every_walk() {
        let t = walk(w);
        let top = t[t.len() - 1] + 5;
        for v in 0..=top {
            let up = step(w, v, 1);
            let down = step(w, v, -1);
            let above = t.iter().copied().find(|&x| x > v);
            let below = t.iter().copied().rev().find(|&x| x < v);
            assert_eq!(up, above.unwrap_or(v), "walk {w}: {v} up");
            assert_eq!(down, below.unwrap_or(v), "walk {w}: {v} down");
            // and further detents go on along the walk
            if let Some(a) = above {
                let i = t.iter().position(|&x| x == a).unwrap();
                assert_eq!(
                    step(w, v, 3),
                    t[(i + 2).min(t.len() - 1)],
                    "walk {w}: {v} +3"
                );
            }
        }
    }
}

fn row(param: u32) -> *const midi_map_entry {
    let e = unsafe { midi_map_for_param(param) };
    assert!(!e.is_null());
    e
}

/// CC 0..127 reaches every position, through the map (a learned CC on PWM's, which has no CC of
/// its own: midi_map_cc_set, as a learned CC goes), and a position sent out comes back as itself.
#[test]
fn cc_0_to_127_reaches_every_position_and_round_trips() {
    for (name, p, w, _, _) in own_walks() {
        let t = walk(w);
        let e = row(p);
        assert_eq!(u32::from(unsafe { (*e).curve }), MIDI_CURVE_WALK, "{name}");
        let mut rig = Rig::new(SID_MODEL_6581);
        let mut reached = BTreeSet::new();
        let mut last = 0;
        for cc in 0..128u8 {
            unsafe { midi_map_cc_set(&mut *rig.live, e, cc) };
            let v = if p == LIVE_PARAM_PWM_RATE {
                u32::from(rig.live.pwm_rate)
            } else {
                unsafe { live_param_get(&*rig.live, p) }
            };
            assert!(v >= last, "{name}: CC {cc} rises");
            let at = (usize::from(cc) * (t.len() - 1) + 63) / 127;
            assert_eq!(
                v, t[at],
                "{name}: CC {cc} is position round(cc (n - 1) / 127)"
            );
            last = v;
            reached.insert(v);
        }
        assert_eq!(reached, t.iter().copied().collect(), "{name}");
        for &v in &t {
            let param = if p == LIVE_PARAM_PWM_RATE {
                unsafe { live_pwm_rate(v) }
            } else {
                v
            };
            let cc = unsafe { midi_map_to_cc(&*rig.live, e, param) };
            let back = unsafe { midi_map_from_cc(&*rig.live, e, cc) };
            if p == LIVE_PARAM_PWM_RATE {
                // the parameter's 0.05 Hz cannot tell the three slowest positions apart
                assert_eq!(back, param, "{name}: {v} -> CC {cc}");
            } else {
                assert_eq!(back, v, "{name}: {v} -> CC {cc}");
            }
        }
    }
    // The vibrato's own CCs (76, 77, 78) through live_cc land on the positions too.
    let mut rig = Rig::new(SID_MODEL_6581);
    for (cc, p, w) in [
        (
            LIVE_CC_VIBRATO_RATE,
            LIVE_PARAM_VIBRATO_RATE,
            LIVE_WALK_VIB_RATE,
        ),
        (
            LIVE_CC_VIBRATO_DEPTH,
            LIVE_PARAM_VIBRATO_DEPTH,
            LIVE_WALK_VIB_DEPTH,
        ),
        (
            LIVE_CC_VIBRATO_DELAY,
            LIVE_PARAM_VIBRATO_DELAY,
            LIVE_WALK_VIB_DELAY,
        ),
    ] {
        let reached: BTreeSet<u32> = (0..128u8)
            .map(|v| {
                rig.cc(cc, v);
                unsafe { live_param_get(&*rig.live, p) }
            })
            .collect();
        assert_eq!(reached, walk(w).into_iter().collect(), "CC {cc}");
    }
}

/// NRPN keeps the full resolution: any value lands as itself, on the walk or not.
#[test]
fn nrpn_sets_values_off_the_walk() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for (p, v) in [
        (LIVE_PARAM_VIBRATO_DEPTH, 3),
        (LIVE_PARAM_VIBRATO_RATE, 61),
        (LIVE_PARAM_PWM_DEPTH, 1000),
        (LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_MIX, 40),
    ] {
        let e = row(p);
        let nrpn = unsafe { (*e).nrpn };
        let mut port = Box::<midi_map_port>::default();
        unsafe { midi_map_port_init(&mut *port, &mut *rig.live) };
        for (cc, value) in [
            (99u8, (nrpn >> 7) as u8),
            (98, (nrpn & 127) as u8),
            (6, (v >> 7) as u8),
            (38, (v & 127) as u8),
        ] {
            unsafe { midi_map_cc(&mut *port, 0, cc, value) };
        }
        assert_eq!(unsafe { live_param_get(&*rig.live, p) }, v, "parameter {p}");
    }
}

/// The filter LFO keeps its detents: each is one of K1's cutoff detents, the walk K1 itself is even
/// in pitch along, so its depth's every detent adds the same swing in pitch; its CC (STEPS over 33
/// values) reaches every one.
#[test]
fn the_filter_lfo_needs_no_curve() {
    let mut rig = Rig::new(SID_MODEL_6581);
    assert_eq!(
        unsafe { live_walk_of(&*rig.live, LIVE_PARAM_FILTER_LFO) },
        LIVE_WALKS
    );
    let reached: BTreeSet<u32> = (0..128u8)
        .map(|v| {
            rig.cc(LIVE_CC_FILTER_LFO, v);
            unsafe { live_param_get(&*rig.live, LIVE_PARAM_FILTER_LFO) }
        })
        .collect();
    assert_eq!(reached, (0..=LIVE_FILTER_LFO_MAX).collect());
}

// --- the effect slots ---

/// What the ear follows on a slot control, by the algorithm's own law (core/fx headers) or by
/// rendering it through the slots.
#[derive(Clone, Copy, Debug)]
enum Ear {
    /// a law of the control, 0..1: a frequency's or a time's logarithm, a drive's dB, ...
    Law(fn(f64) -> f64),
    /// the rendered level, in dB, of what the control adds to the sound at 0 (the effect's own
    /// part: a drive's harmonics, the hum, the noise, a depth's modulation, the wet signal)
    Adds,
    /// a delay's echoes, in dB each: the second echo over the first, rendered
    Echo,
    /// a reverb's decay, the logarithm of its rendered RT60
    Decay,
}

/// Each slot control's ear and shape: [slot][algorithm][field Amount, A, B, Mix]. The shapes are
/// live_walk.c's; a control an algorithm does not use is linear, its measure the value.
fn ear(slot: u32, alg: u32, field: u32) -> Ear {
    use Ear::*;
    let value: fn(f64) -> f64 = |a| a;
    let log_hz: fn(f64) -> f64 = |a| a; // a frequency or time mapped log by the algorithm
    if field == LIVE_SLOT_MIX {
        return Adds;
    }
    let f = (field - LIVE_SLOT_AMOUNT) as usize;
    match (slot, alg) {
        (0, SID_DIST_TAPE) => [
            Law(|a| 20.0 * (1.0 + 2.0 * a).log10()),
            Law(value),
            Law(value),
        ][f],
        (0, SID_DIST_OVERDRIVE) => [
            Law(|a| 20.0 * (1.0 + 0.5 * a + (24.0 * a.powi(5) - 0.5 * a) * a * a).log10()),
            Law(log_hz),
            Adds,
        ][f],
        (0, SID_DIST_FUZZ) => [Law(|a| 20.0 * (1.0 + 99.0 * a * a).log10()), Adds, Adds][f],
        // bits (6 dB of the noise floor each), and the hold rate's log Hz, linear in Hz
        (0, SID_DIST_CRUSH) => [
            Law(value),
            Law(|a| (1.0 - (1.0 - 4410.0 / 44100.0) * a).ln()),
            Law(value),
        ][f],
        (0, SID_DIST_LOFI) => [Law(log_hz), Adds, Adds][f],
        // depths: their swing, logarithmically (Weber: the vibrato's depth is geometric too)
        (1, SID_MOD_CHORUS) => [Law(|a| a.ln()), Law(log_hz), Adds][f],
        (1, SID_MOD_FLANGER) => [Law(|a| (1.0 + 99.0 * a).ln()), Law(log_hz), Law(value)][f],
        (1, SID_MOD_PHASER) => [
            Law(|a| a.ln()),
            Law(log_hz),
            Law(|a| -20.0 * (1.0 - 0.7 * a).log10()),
        ][f],
        (1, SID_MOD_TREMOLO) | (1, SID_MOD_AUTOPAN) => [Adds, Law(log_hz), Law(value)][f],
        (1, SID_MOD_WOW) => [Law(|a| a.ln()), Law(|a| a.ln()), Law(value)][f],
        (2, SID_FXSLOT_DELAY) | (2, SID_FXSLOT_PINGPONG) => [Echo, Law(log_hz), Law(log_hz)][f],
        // damping: the loop low-pass's log Hz (nearly even in its coefficient); diffusion and
        // bandwidth, blends
        (2, SID_FXSLOT_REVERB_LIGHT) => [
            Decay,
            Law(|a| (-(1.0 - (0.9 - 0.6 * a)).ln()).ln()),
            Law(value),
        ][f],
        (2, SID_FXSLOT_REVERB_LUSH) => {
            [Decay, Law(|a| (-(0.05 + 0.65 * a).ln()).ln()), Law(value)][f]
        }
        _ => Law(value),
    }
}

/// Controls whose law no shape of the three makes even: the drive laws of OVERDRIVE (Plaits',
/// made for a linear knob: most of its rise in the top half) and FUZZ (1 + 99 a^2, whose clip
/// starts abruptly at a SID voice's level). They stay linear, as before; the ear check
/// (docs/hardware-test.md) decides.
fn exception(slot: u32, alg: u32, field: u32) -> bool {
    slot == 0 && (alg == SID_DIST_OVERDRIVE || alg == SID_DIST_FUZZ) && field == LIVE_SLOT_AMOUNT
}

/// The slots, warmed up with an algorithm running (its delay line filled, faded in), to fork
/// from: each value is measured on a copy, from the same state.
#[derive(Clone)]
struct Slots {
    s: Box<sid_slots>,
    pool: Box<sid_slot_pool>,
}

fn params(p: [u8; 5]) -> sid_slot_params {
    let f = |v: u8| f32::from(v) / 127.0;
    sid_slot_params {
        algorithm: u32::from(p[0]),
        amount: f(p[1]),
        a: f(p[2]),
        b: f(p[3]),
        mix: f(p[4]),
    }
}

impl Slots {
    fn warm(slot: u32, p: [u8; 5], input: &[i16]) -> Self {
        let mut me = Slots {
            s: Box::<sid_slots>::default(),
            pool: Box::<sid_slot_pool>::default(),
        };
        unsafe { sid_slots_init(&mut *me.s, FS, 1, &mut *me.pool) };
        me.run(slot, p, input);
        me
    }

    /// A copy whose slots use its own pool.
    fn fork(&self) -> Self {
        let mut f = self.clone();
        f.s.pool = &mut *f.pool;
        f
    }

    /// Both sides summed (what both ears get), after setting p.
    fn run(&mut self, slot: u32, p: [u8; 5], input: &[i16]) -> Vec<f64> {
        unsafe { sid_slots_set(&mut *self.s, slot, &params(p)) };
        let mut out = vec![0i16; 2 * input.len()];
        for (i, c) in input.chunks(128).enumerate() {
            unsafe {
                sid_slots_process(
                    &mut *self.s,
                    c.as_ptr(),
                    out[2 * 128 * i..].as_mut_ptr(),
                    c.len() as u32,
                )
            };
        }
        out.chunks(2)
            .map(|lr| f64::from(lr[0]) + f64::from(lr[1]))
            .collect()
    }
}

/// A SID voice's level (0.25 full scale): a 441 Hz sine for the drives, a 220 Hz saw of 20
/// harmonics for the rest.
fn voice(slot: u32, n: usize) -> Vec<i16> {
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(FS);
            let x = if slot == 0 {
                (2.0 * PI * 441.0 * t).sin()
            } else {
                (1..=20)
                    .map(|k| (2.0 * PI * 220.0 * f64::from(k) * t).sin() / f64::from(k))
                    .sum::<f64>()
                    / 1.7
            };
            (x * 0.25 * 32767.0).round() as i16
        })
        .collect()
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

/// The slot's controls with field set to v: the algorithm's defaults, the wet signal alone (Mix
/// 127) unless Mix is the field.
fn controls(slot: u32, alg: u32, field: u32, v: u8) -> [u8; 5] {
    let d = unsafe { live_fx_defaults[slot as usize][alg as usize] };
    let mut p = [alg as u8, d[0], d[1], d[2], 127];
    p[field as usize] = v;
    p
}

/// One control's renders: the slots warmed up at the algorithm's defaults (Mix 127), fed the
/// voice (Adds) or silence (an echo, a decay); then each value from a copy of that state.
struct Renders {
    slot: u32,
    alg: u32,
    field: u32,
    warm: Option<Slots>,
    at_zero: Option<Vec<f64>>,
}

/// Samples to warm up: the longest delay line a slot fills before it fades in (the FX pool's
/// 1.49 s), and the fade.
const WARM: usize = 70_000;
/// Samples after a change before measuring: the controls' smoothing has settled.
const SETTLE: usize = 4410;

impl Renders {
    fn new(slot: u32, alg: u32, field: u32) -> Self {
        Renders {
            slot,
            alg,
            field,
            warm: None,
            at_zero: None,
        }
    }

    fn render(&mut self, v: u8, input: &[i16]) -> Vec<f64> {
        let (slot, alg, field) = (self.slot, self.alg, self.field);
        let warm = self.warm.get_or_insert_with(|| {
            let feed = match ear(slot, alg, field) {
                Ear::Adds => voice(slot, WARM),
                _ => vec![0; WARM],
            };
            Slots::warm(slot, controls(slot, alg, LIVE_SLOT_MIX, 127), &feed)
        });
        let mut f = warm.fork();
        f.run(slot, controls(slot, alg, field, v), input)
    }

    /// The ear's measure of the control at value v (None where it is not defined: 0, an echo too
    /// faint).
    fn measure(&mut self, v: u8) -> Option<f64> {
        if v == 0 {
            return None;
        }
        let (slot, alg, field) = (self.slot, self.alg, self.field);
        let a = f64::from(v) / 127.0;
        match ear(slot, alg, field) {
            Ear::Law(f) => {
                let m = f(a);
                m.is_finite().then_some(m)
            }
            Ear::Adds => {
                let n = SETTLE + 8820;
                let input: Vec<i16> = voice(slot, WARM + n)[WARM..].to_vec();
                if self.at_zero.is_none() {
                    self.at_zero = Some(self.render(0, &input));
                }
                let y = self.render(v, &input);
                let zero = self.at_zero.as_ref().unwrap();
                let diff: Vec<f64> = y[SETTLE..]
                    .iter()
                    .zip(&zero[SETTLE..])
                    .map(|(a, b)| a - b)
                    .collect();
                let reference: Vec<f64> = input[SETTLE..]
                    .iter()
                    .map(|&x| 2.0 * f64::from(x))
                    .collect();
                Some(20.0 * (rms(&diff) / rms(&reference)).log10())
            }
            Ear::Echo => {
                // a 5 ms burst after the change has settled; T where the first echo peaks, and the
                // echo the feedback made of it: at 2T (DELAY), or at 3T (PING-PONG, whose second
                // echo, at 2T, is the right side's tap of the same pass)
                let n = 3 * FS as usize + SETTLE;
                let mut input = vec![0i16; n];
                for (i, x) in input[SETTLE..].iter_mut().enumerate().take(220) {
                    *x = (8000.0 * (2.0 * PI * 1000.0 * i as f64 / f64::from(FS)).sin()) as i16;
                }
                let y = self.render(v, &input);
                let y = &y[SETTLE..];
                let from = 441;
                let (t, p1) =
                    y[from..y.len() / 4]
                        .iter()
                        .enumerate()
                        .fold((0, 0.0f64), |b, (i, x)| {
                            if x.abs() > b.1 {
                                (i + from, x.abs())
                            } else {
                                b
                            }
                        });
                let k = if alg == SID_FXSLOT_PINGPONG { 3 } else { 2 };
                let w = 300;
                let p2 = y[k * t - w..(k * t + w).min(y.len())]
                    .iter()
                    .fold(0.0f64, |m, x| m.max(x.abs()));
                (p1 > 0.0 && p2 > 4.0).then(|| 20.0 * (p2 / p1).log10())
            }
            Ear::Decay => {
                // an impulse; RT60 from the Schroeder integral's fall from -5 to -20 dB (x 4)
                let n = 4 * FS as usize + SETTLE;
                let mut input = vec![0i16; n];
                input[SETTLE] = 16000;
                input[SETTLE + 1] = -16000;
                let y = self.render(v, &input);
                let y = &y[SETTLE..];
                let mut tail: Vec<f64> = y.iter().map(|x| x * x).collect();
                for i in (0..tail.len() - 1).rev() {
                    tail[i] += tail[i + 1];
                }
                let total = tail[0];
                let at = |db: f64| {
                    tail.iter()
                        .position(|&e| 10.0 * (e / total).log10() < db)
                        .map(|i| i as f64 / f64::from(FS))
                };
                match (at(-5.0), at(-20.0)) {
                    (Some(a), Some(b)) if b > a => Some((4.0 * (b - a)).ln()),
                    _ => None,
                }
            }
        }
    }
}

fn algorithms(slot: u32) -> std::ops::Range<u32> {
    1..unsafe { live_slot_algorithms(slot) }
}

/// Every slot control: its shape's steps even to the ear (0.5x..2x of the median), the most even
/// of the three shapes, each measured by the algorithm's law or rendered through the slots.
#[test]
fn each_slot_control_takes_the_most_even_shape_for_its_ear() {
    let shapes = [LIVE_WALK_FX_LIN, LIVE_WALK_FX_LOG, LIVE_WALK_FX_EQP];
    let names = ["LIN", "LOG", "EQP"];
    let mut report = Vec::new();
    let mut uneven = Vec::new();
    for slot in 0..LIVE_SLOTS {
        for alg in algorithms(slot) {
            for field in LIVE_SLOT_AMOUNT..=LIVE_SLOT_MIX {
                let chosen = unsafe { live_walk_slot(slot, alg, field) };
                let mut cache = std::collections::BTreeMap::new();
                let mut renders = Renders::new(slot, alg, field);
                let mut spreads = Vec::new();
                for &w in &shapes {
                    let m: Vec<f64> = walk(w)
                        .iter()
                        .filter_map(|&v| {
                            *cache.entry(v).or_insert_with(|| renders.measure(v as u8))
                        })
                        .collect();
                    spreads.push(spread(&m));
                }
                let i = shapes.iter().position(|&w| w == chosen).unwrap();
                let best = (0..3)
                    .min_by(|&a, &b| unevenness(spreads[a]).total_cmp(&unevenness(spreads[b])))
                    .unwrap();
                let line = format!(
                    "slot {slot} alg {alg} field {field} ({:?}): {} {:.2}..{:.2} (LIN {:.2}..{:.2}, LOG {:.2}..{:.2}, EQP {:.2}..{:.2})",
                    ear(slot, alg, field),
                    names[i],
                    spreads[i].0,
                    spreads[i].1,
                    spreads[0].0,
                    spreads[0].1,
                    spreads[1].0,
                    spreads[1].1,
                    spreads[2].0,
                    spreads[2].1
                );
                report.push(line.clone());
                if exception(slot, alg, field) {
                    assert_eq!(
                        chosen, LIVE_WALK_FX_LIN,
                        "{line}: an exception stays linear"
                    );
                    continue;
                }
                if !even(spreads[i])
                    || (best != i && unevenness(spreads[best]) < unevenness(spreads[i]) - 0.05)
                {
                    uneven.push(line);
                }
            }
        }
    }
    println!("{}", report.join("\n"));
    assert!(uneven.is_empty(), "{uneven:#?}");
}

/// A slot control's CC lands on its shape's positions (by the slot's algorithm now) and reaches
/// every one; NONE's controls are linear.
#[test]
fn slot_ccs_reach_every_position_of_their_shape() {
    let mut rig = Rig::new(SID_MODEL_6581);
    for slot in 0..LIVE_SLOTS {
        for alg in 0..unsafe { live_slot_algorithms(slot) } {
            unsafe { live_slot_set(&mut *rig.live, slot, LIVE_SLOT_ALGORITHM, alg as i32) };
            for field in LIVE_SLOT_AMOUNT..=LIVE_SLOT_MIX {
                let w = unsafe { live_walk_slot(slot, alg, field) };
                let cc = LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + field;
                let reached: BTreeSet<u32> = (0..128u8)
                    .map(|v| {
                        rig.cc(cc, v);
                        u32::from(rig.live.slot[slot as usize][field as usize])
                    })
                    .collect();
                assert_eq!(
                    reached,
                    walk(w).into_iter().collect(),
                    "slot {slot} alg {alg} field {field}"
                );
            }
        }
    }
}

/// LAB's cards turn along the walks: OSC's PWM and PWM HZ, MOD's VIB HZ, DEPTH and DELAY, FX's
/// four by the slot's algorithm; and LIVE's EDIT layer the same slot walks.
#[test]
fn lab_cards_and_the_edit_layer_turn_along_the_walks() {
    let mut ui = lab_ui();
    let k1 = PANEL_ENC_K1;
    let get = |ui: &crate::views::Ui, p: u32| {
        if p == LIVE_PARAM_PWM_RATE {
            u32::from(ui.rig.live.pwm_rate)
        } else {
            unsafe { live_param_get(&*ui.rig.live, p) }
        }
    };
    for (page, k, p, w) in [
        (LAB_PAGE_OSC, 1, LIVE_PARAM_PWM_DEPTH, LIVE_WALK_PWM_DEPTH),
        (LAB_PAGE_OSC, 2, LIVE_PARAM_PWM_RATE, LIVE_WALK_PWM_RATE),
        (LAB_PAGE_MOD, 0, LIVE_PARAM_VIBRATO_RATE, LIVE_WALK_VIB_RATE),
        (
            LAB_PAGE_MOD,
            1,
            LIVE_PARAM_VIBRATO_DEPTH,
            LIVE_WALK_VIB_DEPTH,
        ),
        (
            LAB_PAGE_MOD,
            2,
            LIVE_PARAM_VIBRATO_DELAY,
            LIVE_WALK_VIB_DELAY,
        ),
    ] {
        to_page(&mut ui, page);
        let t = walk(w);
        let start = get(&ui, p);
        ui.turn(k1 + k, 1);
        assert_eq!(get(&ui, p), step(w, start, 1), "page {page} K{}", k + 1);
        ui.turn(k1 + k, 2);
        assert_eq!(get(&ui, p), step(w, start, 3), "page {page} K{}: on", k + 1);
        ui.turn(k1 + k, -64);
        assert_eq!(get(&ui, p), t[0], "page {page} K{}: the bottom", k + 1);
    }
    // FX: the selected slot's controls by its algorithm's shapes (DELAY: feedback and Mix LOG).
    to_page(&mut ui, LAB_PAGE_FX);
    let slot = u32::from(ui.rig.live.fx_select) % LIVE_SLOTS;
    unsafe { live_slot_pick(&mut *ui.rig.live, slot, 1) };
    for k in 0..4u32 {
        let field = LIVE_SLOT_AMOUNT + k;
        let w = unsafe { live_walk_slot(slot, 1, field) };
        let before = u32::from(ui.rig.live.slot[slot as usize][field as usize]);
        ui.turn(k1 + k, 2);
        assert_eq!(
            u32::from(ui.rig.live.slot[slot as usize][field as usize]),
            step(w, before, 2),
            "FX K{}",
            k + 1
        );
    }
    // LIVE's EDIT layer: the same walk through panel_encoder.
    let mut p = Box::<panel>::default();
    unsafe { panel_init(&mut *p) };
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe {
        live_fx_select(&mut *rig.live, 2);
        live_slot_pick(&mut *rig.live, 2, SID_FXSLOT_DELAY);
        panel_buttons(&mut *p, &mut *rig.live, 0, 1 << PANEL_BTN_EDIT);
    }
    let f = unsafe { live_fx_defaults[2][SID_FXSLOT_DELAY as usize] };
    for k in 0..4u32 {
        unsafe { panel_encoder(&mut *p, &mut *rig.live, k1 + k, 1) };
        let w = unsafe { live_walk_slot(2, SID_FXSLOT_DELAY, LIVE_SLOT_AMOUNT + k) };
        assert_eq!(
            u32::from(rig.live.slot[2][(LIVE_SLOT_AMOUNT + k) as usize]),
            step(w, u32::from(f[k as usize]), 1),
            "EDIT + K{}",
            k + 1
        );
    }
}

/// Stored values keep their meaning: a sound with values off every walk saved and loaded is the
/// same sound, and only a turn moves a value onto a walk.
#[test]
fn values_off_the_walk_stay_until_turned() {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe {
        live_param_set(&mut *rig.live, LIVE_PARAM_VIBRATO_DEPTH, 3);
        live_param_set(&mut *rig.live, LIVE_PARAM_VIBRATO_RATE, 60);
        live_param_set(&mut *rig.live, LIVE_PARAM_PWM_DEPTH, 1000);
        live_slot_set(&mut *rig.live, 2, LIVE_SLOT_MIX, 40);
    }
    rig.live.pwm_rate = 1234;
    let mut a = [0u8; LAB_SOUND_BYTES as usize];
    let mut b = [0u8; LAB_SOUND_BYTES as usize];
    unsafe { live_sound_get(&*rig.live, a.as_mut_ptr()) };
    let mut other = Rig::new(SID_MODEL_6581);
    unsafe {
        live_sound_apply(&mut *other.live, a.as_ptr());
        live_sound_get(&*other.live, b.as_mut_ptr());
    }
    assert_eq!(a, b);
    assert_eq!(other.live.pwm_rate, 1234);
    assert_eq!(other.live.vibrato_depth, 3);
}
