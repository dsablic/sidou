// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;
use std::f64::consts::PI;

use sidou_sys::*;

use crate::lab_view::{lab_ui, to_page};
use crate::{rms, Rig};

const FS: u32 = 44100;
const WARM_UP_SAMPLES: usize = 70_000;
const SETTLE_SAMPLES: usize = 4410;
const MIX_WET_ONLY: u8 = 127;

fn walk(w: u32) -> Vec<u32> {
    let n = unsafe { live_walk_count(w) };
    (0..n).map(|i| unsafe { live_walk_at(w, i) }).collect()
}

fn step(w: u32, v: u32, d: i32) -> u32 {
    unsafe { live_walk_step(w, v, d) }
}

fn walked_value(l: &live, p: u32) -> u32 {
    if p == LIVE_PARAM_PWM_RATE {
        u32::from(l.pwm_rate)
    } else {
        unsafe { live_param_get(l, p) }
    }
}

fn steps_over_median(measure: &[f64]) -> (f64, f64) {
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

fn unevenness((lo, hi): (f64, f64)) -> f64 {
    hi.max(1.0 / lo)
}

type OwnEar = fn(u32) -> Option<f64>;

fn log_of(v: f64) -> Option<f64> {
    (v > 0.0).then(|| v.ln())
}

fn own_walks() -> Vec<(&'static str, u32, u32, usize, OwnEar)> {
    vec![
        ("PWM HZ", LIVE_PARAM_PWM_RATE, LIVE_WALK_PWM_RATE, 32, |v| {
            log_of(f64::from(v) * 100.0 / 65536.0)
        }),
        (
            "PWM depth",
            LIVE_PARAM_PWM_DEPTH,
            LIVE_WALK_PWM_DEPTH,
            32,
            |v| Some(f64::from(v).sqrt()),
        ),
        (
            "VIB HZ",
            LIVE_PARAM_VIBRATO_RATE,
            LIVE_WALK_VIB_RATE,
            24,
            |v| log_of(f64::from(v) / 10.0),
        ),
        (
            "VIB DEPTH",
            LIVE_PARAM_VIBRATO_DEPTH,
            LIVE_WALK_VIB_DEPTH,
            32,
            |v| log_of(f64::from(v) * 99.0 / 127.0),
        ),
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
        let s = steps_over_median(&m);
        report.push(format!("{name}: {:.2}..{:.2}", s.0, s.1));
        assert!(
            even(s),
            "{name}: steps {:.2}..{:.2} of the median: {t:?}",
            s.0,
            s.1
        );
    }
    println!("{}", report.join("\n"));
    let pr = walk(LIVE_WALK_PWM_RATE);
    assert_eq!(
        unsafe { live_pwm_rate(pr[0]) },
        1,
        "PWM HZ starts at 0.05 Hz"
    );
    assert_eq!(pr[31], LIVE_PWM_STEP_MAX, "PWM HZ ends at 12.75 Hz");
    assert_eq!(walk(LIVE_WALK_VIB_RATE)[0], 5, "VIB HZ starts at 0.5 Hz");
    assert_eq!(
        *walk(LIVE_WALK_VIB_RATE).last().unwrap(),
        127,
        "VIB HZ ends at 12.7 Hz"
    );
    assert_eq!(
        *walk(LIVE_WALK_PWM_DEPTH).last().unwrap(),
        LIVE_PWM_DEPTH_MAX
    );
}

#[test]
fn every_detent_moves_to_the_next_position() {
    for (name, p, w, _, _) in own_walks() {
        let t = walk(w);
        let mut rig = Rig::new(SID_MODEL_6581);
        let stored = |rig: &Rig| walked_value(&rig.live, p);
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
            let v = walked_value(&rig.live, p);
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
                assert_eq!(
                    back, param,
                    "{name}: {v} -> CC {cc} (0.05 Hz cannot tell the slowest positions apart)"
                );
            } else {
                assert_eq!(back, v, "{name}: {v} -> CC {cc}");
            }
        }
    }
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

#[derive(Clone, Copy, Debug)]
enum Ear {
    Law(fn(f64) -> f64),
    AddedLevelDb,
    SecondEchoOverFirstDb,
    LogRt60,
}

fn ear(slot: u32, alg: u32, field: u32) -> Ear {
    use Ear::*;
    let value: fn(f64) -> f64 = |a| a;
    let log_mapped_by_algorithm: fn(f64) -> f64 = |a| a;
    if field == LIVE_SLOT_MIX {
        return AddedLevelDb;
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
            Law(log_mapped_by_algorithm),
            AddedLevelDb,
        ][f],
        (0, SID_DIST_FUZZ) => [
            Law(|a| 20.0 * (1.0 + 99.0 * a * a).log10()),
            AddedLevelDb,
            AddedLevelDb,
        ][f],
        (0, SID_DIST_CRUSH) => [
            Law(value),
            Law(|a| (1.0 - (1.0 - 4410.0 / 44100.0) * a).ln()),
            Law(value),
        ][f],
        (0, SID_DIST_LOFI) => [Law(log_mapped_by_algorithm), AddedLevelDb, AddedLevelDb][f],
        (1, SID_MOD_CHORUS) => [Law(|a| a.ln()), Law(log_mapped_by_algorithm), AddedLevelDb][f],
        (1, SID_MOD_FLANGER) => [
            Law(|a| (1.0 + 99.0 * a).ln()),
            Law(log_mapped_by_algorithm),
            Law(value),
        ][f],
        (1, SID_MOD_PHASER) => [
            Law(|a| a.ln()),
            Law(log_mapped_by_algorithm),
            Law(|a| -20.0 * (1.0 - 0.7 * a).log10()),
        ][f],
        (1, SID_MOD_TREMOLO) | (1, SID_MOD_AUTOPAN) => {
            [AddedLevelDb, Law(log_mapped_by_algorithm), Law(value)][f]
        }
        (1, SID_MOD_WOW) => [Law(|a| a.ln()), Law(|a| a.ln()), Law(value)][f],
        (2, SID_FXSLOT_DELAY) | (2, SID_FXSLOT_PINGPONG) => [
            SecondEchoOverFirstDb,
            Law(log_mapped_by_algorithm),
            Law(log_mapped_by_algorithm),
        ][f],
        (2, SID_FXSLOT_REVERB_LIGHT) => [
            LogRt60,
            Law(|a| (-(1.0 - (0.9 - 0.6 * a)).ln()).ln()),
            Law(value),
        ][f],
        (2, SID_FXSLOT_REVERB_LUSH) => {
            [LogRt60, Law(|a| (-(0.05 + 0.65 * a).ln()).ln()), Law(value)][f]
        }
        _ => Law(value),
    }
}

fn stays_linear_by_exception(slot: u32, alg: u32, field: u32) -> bool {
    slot == 0 && (alg == SID_DIST_OVERDRIVE || alg == SID_DIST_FUZZ) && field == LIVE_SLOT_AMOUNT
}

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
        me.render_left_plus_right(slot, p, input);
        me
    }

    fn fork(&self) -> Self {
        let mut f = self.clone();
        f.s.pool = &mut *f.pool;
        f
    }

    fn render_left_plus_right(&mut self, slot: u32, p: [u8; 5], input: &[i16]) -> Vec<f64> {
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

fn voice(slot: u32, n: usize) -> Vec<i16> {
    let level = if slot == 0 {
        0.25 / SID_SLOT_DIST_GAIN
    } else {
        0.25
    };
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
            (x * level * 32767.0).round() as i16
        })
        .collect()
}

fn controls(slot: u32, alg: u32, field: u32, v: u8) -> [u8; 5] {
    let d = unsafe { live_fx_defaults[slot as usize][alg as usize] };
    let mut p = [alg as u8, d[0], d[1], d[2], MIX_WET_ONLY];
    p[field as usize] = v;
    p
}

struct Renders {
    slot: u32,
    alg: u32,
    field: u32,
    warm: Option<Slots>,
    at_zero: Option<Vec<f64>>,
}

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
                Ear::AddedLevelDb => voice(slot, WARM_UP_SAMPLES),
                _ => vec![0; WARM_UP_SAMPLES],
            };
            Slots::warm(
                slot,
                controls(slot, alg, LIVE_SLOT_MIX, MIX_WET_ONLY),
                &feed,
            )
        });
        let mut f = warm.fork();
        f.render_left_plus_right(slot, controls(slot, alg, field, v), input)
    }

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
            Ear::AddedLevelDb => {
                let n = SETTLE_SAMPLES + 8820;
                let input: Vec<i16> = voice(slot, WARM_UP_SAMPLES + n)[WARM_UP_SAMPLES..].to_vec();
                if self.at_zero.is_none() {
                    self.at_zero = Some(self.render(0, &input));
                }
                let y = self.render(v, &input);
                let zero = self.at_zero.as_ref().unwrap();
                let diff: Vec<f64> = y[SETTLE_SAMPLES..]
                    .iter()
                    .zip(&zero[SETTLE_SAMPLES..])
                    .map(|(a, b)| a - b)
                    .collect();
                let reference: Vec<f64> = input[SETTLE_SAMPLES..]
                    .iter()
                    .map(|&x| 2.0 * f64::from(x))
                    .collect();
                Some(20.0 * (rms(&diff) / rms(&reference)).log10())
            }
            Ear::SecondEchoOverFirstDb => {
                let n = 3 * FS as usize + SETTLE_SAMPLES;
                let mut input = vec![0i16; n];
                for (i, x) in input[SETTLE_SAMPLES..].iter_mut().enumerate().take(220) {
                    *x = (8000.0 * (2.0 * PI * 1000.0 * i as f64 / f64::from(FS)).sin()) as i16;
                }
                let y = self.render(v, &input);
                let y = &y[SETTLE_SAMPLES..];
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
            Ear::LogRt60 => {
                let n = 4 * FS as usize + SETTLE_SAMPLES;
                let mut input = vec![0i16; n];
                input[SETTLE_SAMPLES] = 16000;
                input[SETTLE_SAMPLES + 1] = -16000;
                let y = self.render(v, &input);
                let y = &y[SETTLE_SAMPLES..];
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
                    spreads.push(steps_over_median(&m));
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
                if stays_linear_by_exception(slot, alg, field) {
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

#[test]
fn lab_cards_and_the_edit_layer_turn_along_the_walks() {
    let mut ui = lab_ui();
    let k1 = PANEL_ENC_K1;
    let get = |ui: &crate::views::Ui, p: u32| walked_value(&ui.rig.live, p);
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
    let a = rig.sound();
    let mut other = Rig::new(SID_MODEL_6581);
    unsafe { live_sound_apply(&mut *other.live, a.as_ptr()) };
    assert_eq!(a, other.sound());
    assert_eq!(other.live.pwm_rate, 1234);
    assert_eq!(other.live.vibrato_depth, 3);
}
