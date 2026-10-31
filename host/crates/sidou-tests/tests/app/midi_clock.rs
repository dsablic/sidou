// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

const RATE: f64 = 44118.0;
const MS: f64 = RATE / 1000.0;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Debug, Clone, Copy)]
enum Port {
    Trs,
    Usb,
}

struct Wire {
    port: Port,
    poll_phase: f64,
    frame_phase: f64,
    rng: std::cell::RefCell<Rng>,
}

fn ceil_to(t: f64, step: f64, phase: f64) -> f64 {
    ((t - phase) / step).ceil() * step + phase
}

impl Wire {
    fn new(port: Port, rng: &mut Rng) -> Self {
        Wire {
            port,
            poll_phase: rng.unit() * 0.5 * MS,
            frame_phase: rng.unit() * MS,
            rng: Rng(rng.next() | 1).into(),
        }
    }

    fn stamp(&self, t: f64) -> f64 {
        let r = self.rng.borrow_mut().unit();
        let arrived = match self.port {
            Port::Trs => t + r * 0.5 * MS + 10.0 / 31250.0 * RATE,
            Port::Usb => ceil_to(t + r * MS, MS, self.frame_phase),
        };
        ceil_to(arrived, 0.5 * MS, self.poll_phase)
    }
}

fn rt(status: u8) -> midi_msg {
    midi_msg {
        kind: MIDI_REALTIME,
        data1: status,
        ..Default::default()
    }
}

fn spp(value: u16) -> midi_msg {
    midi_msg {
        kind: MIDI_SONG_POSITION,
        bend: value,
        ..Default::default()
    }
}

struct Follower(Box<midi_clock>);

impl Follower {
    fn new() -> Self {
        let mut c = Box::<midi_clock>::default();
        unsafe { midi_clock_init(&mut *c, RATE as u32) };
        Follower(c)
    }

    fn msg(&mut self, m: midi_msg, stamp: u32) -> midi_clock_event {
        unsafe { midi_clock_msg(&mut *self.0, &m, stamp) }
    }

    fn clock(&mut self, stamp: u32) -> midi_clock_event {
        self.msg(rt(0xf8), stamp)
    }

    fn poll(&mut self, now: u32) -> midi_clock_event {
        unsafe { midi_clock_poll(&mut *self.0, now) }
    }

    fn hold(&mut self, start: u32, end: u32) {
        unsafe { midi_clock_hold(&mut *self.0, start, end) }
    }

    fn period(&self) -> Option<f64> {
        let q = unsafe { midi_clock_period_q16(&*self.0) };
        (q != 0).then(|| f64::from(q) / 65536.0)
    }

    fn time_of(&self, position: u32) -> Option<u32> {
        let mut stamp = 0u32;
        (unsafe { midi_clock_time_of(&*self.0, position, &mut stamp) } != 0).then_some(stamp)
    }

    fn frame(&self, t0: u8, t1: u8, rows: u8, late: i32) -> f64 {
        f64::from(unsafe { midi_clock_frame_q16(&*self.0, t0, t1, rows, late) }) / 65536.0
    }
}

fn period_of(bpm: f64) -> f64 {
    RATE * 60.0 / bpm / 24.0
}

#[derive(Debug, Default)]
struct Worst {
    tempo: f64,
    phase: f64,
}

fn wrapped_stamp(x: f64) -> u32 {
    x as u64 as u32
}

fn phase_ms(got: u32, want: f64) -> f64 {
    f64::from(got.wrapping_sub(wrapped_stamp(want)) as i32) / MS
}

fn clocks(
    wire: &Wire,
    t0: f64,
    n: usize,
    period: &dyn Fn(usize) -> f64,
    skip: &dyn Fn(usize) -> bool,
) -> Vec<(f64, f64, u32)> {
    let mut t = t0;
    let mut out = Vec::new();
    for k in 0..n {
        if !skip(k) {
            out.push((t, wire.stamp(t), k as u32));
        }
        t += period(k);
    }
    out
}

fn mean_delay(sent: &[(f64, f64, u32)]) -> f64 {
    sent.iter().map(|&(t, s, _)| s - t).sum::<f64>() / sent.len() as f64
}

fn started_at(t0: f64) -> Follower {
    let mut f = Follower::new();
    assert_eq!(f.msg(rt(0xfa), wrapped_stamp(t0)), MIDI_CLOCK_EV_START);
    f
}

fn worst_after_lock(
    wire: &Wire,
    t0: f64,
    n: usize,
    period: &dyn Fn(usize) -> f64,
    from: usize,
) -> Worst {
    let sent = clocks(wire, t0, n, period, &|_| false);
    let delay = mean_delay(&sent);
    let mut f = started_at(t0 - 10.0);
    let mut w = Worst::default();
    for (k, &(t, stamp, number)) in sent.iter().enumerate() {
        assert_eq!(
            f.clock(wrapped_stamp(stamp)),
            MIDI_CLOCK_EV_NONE,
            "clock {k}"
        );
        if k + 1 < MIDI_CLOCK_LOCK as usize {
            assert_eq!(f.period(), None, "a tempo after {} clocks", k + 1);
        }
        if k < from {
            continue;
        }
        let want = period(k.saturating_sub(1));
        let got = f.period().expect("locked");
        w.tempo = w.tempo.max((got / want - 1.0).abs());
        let at = f.time_of(number).expect("a time");
        w.phase = w.phase.max(phase_ms(at, t + delay).abs());
    }
    w
}

const TEMPOS: [f64; 4] = [60.0, 120.0, 125.3, 200.0];

#[test]
fn steady_tempos_lock_within_two_beats_and_hold() {
    let mut rng = Rng(0x5eed_c10c);
    for port in [Port::Trs, Port::Usb] {
        for bpm in TEMPOS {
            for trial in 0..8 {
                let wire = Wire::new(port, &mut rng);
                let t0 = 1000.0 + rng.unit() * 4e9;
                let p = period_of(bpm);
                let w = worst_after_lock(&wire, t0, 24 * 40, &|_| p, 2 * 24);
                println!("{port:?} {bpm} BPM trial {trial}: {w:?}");
                assert!(
                    w.tempo < 0.001 && w.phase < 1.0,
                    "{port:?} {bpm} BPM: {w:?}"
                );
            }
        }
    }
}

#[test]
fn tempo_and_phase_hold_across_the_stamps_wrap() {
    let mut rng = Rng(7);
    let wire = Wire::new(Port::Trs, &mut rng);
    let p = period_of(120.0);
    let t0 = f64::from(u32::MAX) - 30.0 * p;
    let w = worst_after_lock(&wire, t0, 24 * 8, &|_| p, 2 * 24);
    assert!(w.tempo < 0.001 && w.phase < 1.0, "{w:?}");
}

#[test]
fn a_tempo_step_settles_within_the_window_without_overshoot() {
    let mut rng = Rng(0x57e9);
    for (from_bpm, to_bpm) in [(120.0, 150.0), (150.0, 100.0), (60.0, 200.0), (200.0, 90.0)] {
        for port in [Port::Trs, Port::Usb] {
            let wire = Wire::new(port, &mut rng);
            let step = 24 * 8;
            let period = |k: usize| period_of(if k < step { from_bpm } else { to_bpm });
            let (lo, hi) = (period(0).min(period(step)), period(0).max(period(step)));
            let sent = clocks(
                &wire,
                5000.0,
                step + MIDI_CLOCK_WINDOW as usize + 24 * 4,
                &period,
                &|_| false,
            );
            let mut f = started_at(4990.0);
            for (k, &(_, stamp, _)) in sent.iter().enumerate() {
                f.clock(wrapped_stamp(stamp));
                if k < 2 * 24 {
                    continue;
                }
                let got = f.period().unwrap();
                assert!(
                    got > lo * 0.99 && got < hi * 1.01,
                    "{port:?} {from_bpm}->{to_bpm}: clock {k} {got} outside {lo}..{hi}"
                );
                if k >= step + MIDI_CLOCK_WINDOW as usize {
                    let err = (got / period(k - 1) - 1.0).abs();
                    assert!(
                        err < 0.001,
                        "{port:?} {from_bpm}->{to_bpm}: clock {k} {err}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_tempo_ramp_is_followed_without_overshooting() {
    let mut rng = Rng(0x7a39);
    for (from_bpm, to_bpm) in [(100.0, 160.0), (180.0, 70.0)] {
        for port in [Port::Trs, Port::Usb] {
            let wire = Wire::new(port, &mut rng);
            let (start, len) = (24 * 4, 24 * 8);
            let bpm = |k: usize| {
                let x = (k.saturating_sub(start) as f64 / len as f64).min(1.0);
                from_bpm + (to_bpm - from_bpm) * x
            };
            let period = |k: usize| period_of(bpm(k));
            let n = start + len + MIDI_CLOCK_WINDOW as usize + 24 * 2;
            let sent = clocks(&wire, 100.0, n, &period, &|_| false);
            let mut f = started_at(0.0);
            let mut worst: f64 = 0.0;
            for (k, &(_, stamp, _)) in sent.iter().enumerate() {
                f.clock(wrapped_stamp(stamp));
                if k < 2 * 24 {
                    continue;
                }
                let got = 60.0 * RATE / 24.0 / f.period().unwrap();
                let seen = (k.saturating_sub(MIDI_CLOCK_WINDOW as usize)..k).map(bpm);
                let (lo, hi) = seen.fold((f64::MAX, 0.0f64), |(l, h), b| (l.min(b), h.max(b)));
                let over = (got / hi - 1.0).max(1.0 - got / lo).max(0.0);
                worst = worst.max(over);
                if k >= start + len + MIDI_CLOCK_WINDOW as usize {
                    assert!(
                        (got / to_bpm - 1.0).abs() < 0.001,
                        "{port:?} clock {k} {got}"
                    );
                }
            }
            println!("{port:?} {from_bpm}->{to_bpm}: worst overshoot {worst:.5}");
            assert!(
                worst < 0.01,
                "{port:?} {from_bpm}->{to_bpm}: overshoot {worst}"
            );
        }
    }
}

#[test]
fn dropped_clocks_disturb_the_tempo_for_a_window_at_most() {
    let mut rng = Rng(0xd209);
    let wire = Wire::new(Port::Trs, &mut rng);
    let p = period_of(120.0);
    let drops = [24 * 4 + 5, 24 * 12 + 17, 24 * 20];
    let sent = clocks(&wire, 100.0, 24 * 26, &|_| p, &|k| drops.contains(&k));
    let mut f = started_at(0.0);
    for (i, &(_, stamp, k)) in sent.iter().enumerate() {
        f.clock(wrapped_stamp(stamp));
        if i < 2 * 24 {
            continue;
        }
        let err = (f.period().expect("stays locked") / p - 1.0).abs();
        assert!(err < 0.04, "clock {k}: {err}");
        let since = drops.iter().filter(|&&d| d <= k as usize).max();
        if since.is_none_or(|&d| k as usize >= d + MIDI_CLOCK_WINDOW as usize) {
            assert!(err < 0.001, "clock {k}: {err}");
        }
    }
}

#[test]
fn a_clock_that_stops_is_lost_after_300_ms() {
    let p = period_of(120.0);
    let mut f = Follower::new();
    assert_eq!(f.poll(0), MIDI_CLOCK_EV_NONE, "no clock yet");
    let mut t = 100.0;
    send_clocks(&mut f, &mut t, p, 24 * 4);
    let last = wrapped_stamp(t - p);
    assert!(f.period().is_some());
    let lost = MIDI_CLOCK_LOST_MS * RATE as u32 / 1000;
    assert_eq!(f.poll(last + lost), MIDI_CLOCK_EV_NONE);
    assert!(f.period().is_some());
    assert_eq!(f.poll(last + lost + 1), MIDI_CLOCK_EV_LOST);
    assert_eq!(f.period(), None);
    assert_eq!(f.poll(last + lost + 2), MIDI_CLOCK_EV_NONE, "reported once");
    assert_eq!(f.poll(last + 10 * lost), MIDI_CLOCK_EV_NONE);

    let p2 = period_of(90.0);
    let mut t = f64::from(last) + 2.0 * f64::from(lost);
    for k in 0..48 {
        assert_eq!(f.clock(t as u32), MIDI_CLOCK_EV_NONE);
        t += p2;
        if k + 1 >= MIDI_CLOCK_LOCK as usize {
            assert!((f.period().unwrap() / p2 - 1.0).abs() < 1e-3);
        }
    }

    let t = t + 2.0 * f64::from(lost);
    assert_eq!(f.clock(t as u32), MIDI_CLOCK_EV_LOST);
    assert_eq!(f.period(), None);
    assert_eq!(f.poll(t as u32 + 1), MIDI_CLOCK_EV_NONE);
}

fn worst_from_a_35_ms_hold(port: Port, bpm: f64, told: bool, seed: u64) -> Worst {
    let mut rng = Rng(seed);
    let wire = Wire::new(port, &mut rng);
    let p = period_of(bpm);
    let t0 = 2000.0;
    let sent = clocks(&wire, t0, 24 * 12, &|_| p, &|_| false);
    let delay = mean_delay(&sent);
    let hold_start = t0 + 24.0 * 6.0 * p + rng.unit() * p;
    let hold_end = hold_start + 35.0 * MS;
    let after = ceil_to(hold_end, 0.5 * MS, wire.poll_phase);
    let mut f = started_at(t0 - 10.0);
    let mut reported = false;
    let mut late = 0;
    let mut w = Worst::default();
    for &(t, stamp, number) in &sent {
        let stamp = if stamp >= hold_start && stamp < after {
            late += 1;
            after
        } else {
            stamp
        };
        if told && !reported && stamp >= hold_start {
            f.hold(wrapped_stamp(hold_start), wrapped_stamp(hold_end));
            reported = true;
        }
        f.clock(wrapped_stamp(stamp));
        if t < hold_start {
            continue;
        }
        let got = f.period().unwrap();
        w.tempo = w.tempo.max((got / p - 1.0).abs());
        let at = f.time_of(number).unwrap();
        w.phase = w.phase.max(phase_ms(at, t + delay).abs());
    }
    assert!(late >= 1, "the hold caught {late} clocks");
    w
}

#[test]
fn a_35_ms_hold_keeps_the_tempo_and_phase() {
    for port in [Port::Trs, Port::Usb] {
        for seed in 1..=16 {
            let w = worst_from_a_35_ms_hold(port, 120.0, true, seed);
            assert!(
                w.tempo < 0.001 && w.phase < 1.0,
                "{port:?} seed {seed}: {w:?}"
            );
        }
    }
    let bad = (1..=16)
        .map(|seed| worst_from_a_35_ms_hold(Port::Trs, 120.0, false, seed))
        .filter(|w| w.tempo > 0.001 || w.phase > 1.0)
        .count();
    assert!(bad >= 4, "untold, {bad} of 16 holds broke a bound");
}

fn send_clocks(f: &mut Follower, t: &mut f64, p: f64, n: usize) {
    for _ in 0..n {
        assert_eq!(f.clock(wrapped_stamp(*t)), MIDI_CLOCK_EV_NONE);
        *t += p;
    }
}

#[test]
fn transport_events_and_the_song_position() {
    let p = period_of(120.0);
    let mut f = Follower::new();
    let mut t = 0.0f64;
    send_clocks(&mut f, &mut t, p, 30);
    assert!(f.period().is_some());
    assert_eq!(f.time_of(0), None, "stopped");
    assert_eq!(f.0.position, 0);

    assert_eq!(f.msg(rt(0xfa), wrapped_stamp(t) - 100), MIDI_CLOCK_EV_START);
    assert_eq!(f.time_of(0), None, "no clock since the start");
    let first = t;
    send_clocks(&mut f, &mut t, p, 1);
    assert_eq!(f.0.position, 1);
    assert!(phase_ms(f.time_of(0).unwrap(), first).abs() < 0.05);
    send_clocks(&mut f, &mut t, p, 47);
    assert_eq!(f.0.position, 48);
    assert!(
        phase_ms(f.time_of(48).unwrap(), t).abs() < 0.05,
        "the next clock"
    );
    assert!(phase_ms(f.time_of(24).unwrap(), first + 24.0 * p).abs() < 0.05);

    assert_eq!(f.msg(rt(0xfc), wrapped_stamp(t)), MIDI_CLOCK_EV_STOP);
    send_clocks(&mut f, &mut t, p, 10);
    assert_eq!(f.0.position, 48);
    assert_eq!(f.time_of(48), None);

    assert_eq!(f.msg(spp(33), wrapped_stamp(t)), MIDI_CLOCK_EV_POSITION);
    assert_eq!(f.0.position, 33 * MIDI_CLOCK_PER_16TH);
    assert_eq!(f.msg(rt(0xfb), wrapped_stamp(t)), MIDI_CLOCK_EV_CONTINUE);
    let next = t;
    send_clocks(&mut f, &mut t, p, 1);
    assert_eq!(f.0.position, 33 * 6 + 1);
    assert!(phase_ms(f.time_of(33 * 6).unwrap(), next).abs() < 0.05);
    assert!(phase_ms(f.time_of(33 * 6 + 24).unwrap(), next + 24.0 * p).abs() < 0.05);

    assert_eq!(f.msg(rt(0xfa), wrapped_stamp(t)), MIDI_CLOCK_EV_START);
    assert_eq!(f.0.position, 0);
    for other in [0xf9u8, 0xfd, 0xfe, 0xff] {
        assert_eq!(f.msg(rt(other), wrapped_stamp(t)), MIDI_CLOCK_EV_NONE);
    }
    let note = midi_msg {
        kind: MIDI_NOTE_ON,
        data1: 60,
        data2: 100,
        ..Default::default()
    };
    assert_eq!(f.msg(note, wrapped_stamp(t)), MIDI_CLOCK_EV_NONE);
    send_clocks(&mut f, &mut t, p, 3);
    assert_eq!(f.0.position, 3);
}

const PAL_FRAME: f64 = RATE * 19656.0 / 985248.0;

#[test]
fn the_frame_period_lands_rows_on_the_clock() {
    let p = period_of(125.3);
    let mut f = Follower::new();
    assert_eq!(f.frame(6, 6, 4, 0), 0.0, "no tempo yet");
    let mut t = 0.0;
    send_clocks(&mut f, &mut t, p, 48);
    let frame = f.frame(6, 6, 4, 0);
    assert!(
        (frame / PAL_FRAME - 1.0).abs() < 0.001,
        "{frame} {PAL_FRAME}"
    );
    assert!(
        (frame - p * 24.0 / 24.0).abs() < 0.01,
        "a beat is 6 x 4 frames: {frame}"
    );

    assert!((f.frame(5, 7, 4, 0) - frame).abs() < 0.01);
    assert!((f.frame(3, 3, 8, 0) - frame).abs() < 0.01);
    assert!((f.frame(6, 6, 2, 0) - 2.0 * frame).abs() < 0.02);

    let beat = 24.0 * p;
    let late = (beat * 0.005) as i32;
    let want = (beat - f64::from(late)) / 24.0;
    assert!((f.frame(6, 6, 4, late) - want).abs() < 0.01);
    let want = (beat + f64::from(late)) / 24.0;
    assert!((f.frame(6, 6, 4, -late) - want).abs() < 0.01);
    let most = beat / f64::from(MIDI_CLOCK_CORRECTION) / 24.0;
    assert!((f.frame(6, 6, 4, i32::MAX) - (frame - most)).abs() < 0.01);
    assert!((f.frame(6, 6, 4, i32::MIN) - (frame + most)).abs() < 0.01);

    assert_eq!(f.frame(0, 0, 4, 0), 0.0);
    assert_eq!(f.frame(6, 6, 0, 0), 0.0);
}
