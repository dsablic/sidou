// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

use crate::{Lcg, ATTACK_0_FROM_RELOAD, RELEASE_6_PERIOD, RELEASE_FROM_RELOAD};

const SUSTAIN_0XFF: u8 = 0xf0;

fn powered_on() -> sid_env {
    let mut e = sid_env {
        counter: 0xaa,
        ..Default::default()
    };
    unsafe { sid_env_reset(&mut e) };
    e
}

fn released_to_zero_and_frozen() -> sid_env {
    let mut e = powered_on();
    advance(&mut e, 100_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
    e
}

fn gated(ad: u8, sr: u8) -> sid_env {
    let mut e = released_to_zero_and_frozen();
    unsafe {
        sid_env_write_ad(&mut e, ad);
        sid_env_write_sr(&mut e, sr);
        sid_env_set_gate(&mut e, 1);
    }
    e
}

fn advance(e: &mut sid_env, cycles: u32) {
    unsafe { sid_env_advance(e, cycles) }
    assert_resting_invariants(e);
}

fn assert_resting_invariants(e: &sid_env) {
    let failed = unsafe { sid_env_check(e) };
    assert_eq!(failed, 0, "sid_env_check: invariant {failed} fails: {e:?}");
}

fn cycles_to_next_step(e: &mut sid_env, limit: u32) -> Option<u32> {
    let start = e.counter;
    (1..=limit).find(|_| {
        advance(e, 1);
        e.counter != start
    })
}

#[test]
fn reset_keeps_the_counter_and_releases_from_it_three_cycles_in() {
    let mut e = powered_on();
    assert_eq!(e.counter, 0xaa);
    assert_eq!(e.counter_enabled, 1);
    assert_eq!(e.state, SID_ENV_RELEASE);
    advance(&mut e, 2);
    assert_eq!(e.counter, 0xaa);
    advance(&mut e, 1);
    assert_eq!(e.counter, 0xa9);
    advance(&mut e, 8);
    assert_eq!(e.counter, 0xa9);
    advance(&mut e, 1);
    assert_eq!(e.counter, 0xa8);
    advance(&mut e, 100_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
}

#[test]
fn reset_with_the_counter_at_zero_wraps_it_to_0xff() {
    let mut e = sid_env::default();
    unsafe { sid_env_reset(&mut e) };
    advance(&mut e, 3);
    assert_eq!(e.counter, 0xff);
}

#[test]
fn env3_shows_the_counter_one_cycle_late() {
    let mut e = powered_on();
    let mut previous = e.counter;
    for cycle in 0..200 {
        advance(&mut e, 1);
        assert_eq!(e.env3, previous, "cycle {cycle}");
        previous = e.counter;
    }
}

#[test]
fn attack_steps_once_per_rate_period() {
    for (rate, period) in [(0u8, 9u32), (1, 32), (8, 392)] {
        let mut e = gated(rate << 4, 0xf0);
        cycles_to_next_step(&mut e, 40_000).expect("first attack step");
        for step in 0..10 {
            assert_eq!(
                cycles_to_next_step(&mut e, 40_000),
                Some(period),
                "rate {rate}, step {step}"
            );
        }
    }
}

#[test]
fn attack_reaches_full_scale_then_decays_to_sustain_and_holds() {
    let mut e = gated(0x00, 0x80);
    advance(&mut e, 9 * 256 + 20);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    advance(&mut e, 9 * (0xff - 0x88) + 20);
    assert_eq!(e.counter, 0x88);
    advance(&mut e, 100_000);
    assert_eq!(e.counter, 0x88);
}

#[test]
fn exponential_divider_doubles_step_time_below_0x5d() {
    let mut e = gated(0x00, 0x00);
    advance(&mut e, 9 * 256 + 20);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    while e.counter != 0x5d {
        advance(&mut e, 1);
    }
    assert_eq!(cycles_to_next_step(&mut e, 100), Some(19));
    assert_eq!(cycles_to_next_step(&mut e, 100), Some(18));
    assert_eq!(cycles_to_next_step(&mut e, 100), Some(18));
}

#[test]
fn release_freezes_at_zero() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 9 * 256 + 20);
    unsafe { sid_env_set_gate(&mut e, 0) };
    advance(&mut e, 2_000_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
}

#[test]
fn lowering_the_rate_below_the_lfsr_position_waits_for_the_lfsr_to_wrap() {
    let mut e = gated(0xf0, 0xf0);
    advance(&mut e, 1000);
    let counter = e.counter;
    unsafe { sid_env_write_ad(&mut e, 0x00) };
    advance(&mut e, 30_000);
    assert_eq!(e.counter, counter);
    let wait = cycles_to_next_step(&mut e, 5_000).expect("a step after the wrap");
    assert!(wait < 3_000, "{wait}");
}

fn saw_lead_retrigger_waits_per_release_phase(gate_reset: u8) -> Vec<u32> {
    let mut e = powered_on();
    e.gate_reset = gate_reset;
    unsafe {
        sid_env_write_ad(&mut e, 0x09);
        sid_env_write_sr(&mut e, 0xa6);
        sid_env_set_gate(&mut e, 1);
    }
    advance(&mut e, 100_000);
    unsafe { sid_env_set_gate(&mut e, 0) };
    advance(&mut e, 4_000_000);
    assert_eq!(e.counter, 0, "released to 0");
    (0..RELEASE_6_PERIOD)
        .map(|phase| {
            let mut x = e;
            advance(&mut x, phase);
            unsafe { sid_env_set_gate(&mut x, 1) };
            cycles_to_next_step(&mut x, 40_000).expect("the attack never started")
        })
        .collect()
}

#[test]
fn a_retrigger_after_a_slow_release_waits_for_the_lfsr_unless_the_gate_resets_it() {
    let authentic = saw_lead_retrigger_waits_per_release_phase(0);
    let at_once = authentic
        .iter()
        .filter(|&&w| w <= ATTACK_0_FROM_RELOAD)
        .count();
    assert_eq!(at_once, 8, "{authentic:?}");
    assert!(
        authentic
            .iter()
            .all(|&w| w <= ATTACK_0_FROM_RELOAD || w > 32_000),
        "{authentic:?}"
    );
    let instant = saw_lead_retrigger_waits_per_release_phase(1);
    assert!(
        instant.iter().all(|&w| w <= ATTACK_0_FROM_RELOAD),
        "{instant:?}"
    );
}

const RELEASE_FASTER_THAN_DECAY: [(&str, u8, u8); 3] = [
    ("SAW LEAD", 0x09, 0xa6),
    ("SYNC LEAD", 0x0a, 0xb8),
    ("PWM BASS", 0x05, 0x84),
];

fn rate_period(rate: u8) -> u32 {
    unsafe { sid_env_rate_pos(rate) + 1 }
}

fn release_waits_per_decay_phase(
    ad: u8,
    sr: u8,
    gate_reset: u8,
    limit: u32,
) -> Vec<(u32, u32, u32)> {
    let mut e = powered_on();
    e.gate_reset = gate_reset;
    unsafe {
        sid_env_write_ad(&mut e, ad);
        sid_env_write_sr(&mut e, sr);
        sid_env_set_gate(&mut e, 1);
    }
    advance(&mut e, 1_000_000);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    assert_eq!(e.counter, (sr >> 4) * 0x11, "holding sustain");
    (0..rate_period(ad & 0x0f))
        .map(|phase| {
            let mut x = e;
            advance(&mut x, phase);
            unsafe { sid_env_settle(&mut x) };
            let divider = (x.exp_period, x.exp_counter);
            unsafe { sid_env_set_gate(&mut x, 0) };
            let wait = cycles_to_next_step(&mut x, limit).expect("the release never started");
            (wait, divider.0, divider.1)
        })
        .collect()
}

#[test]
fn with_the_gate_reset_a_release_starts_within_a_release_period_at_every_phase() {
    for (name, ad, sr) in RELEASE_FASTER_THAN_DECAY {
        let bound = rate_period(sr & 0x0f) + RELEASE_FROM_RELOAD;
        let waits = release_waits_per_decay_phase(ad, sr, 1, 40_000);
        assert!(
            waits.iter().all(|&(_, p, _)| p == 1),
            "{name}: divider period 1"
        );
        let worst = waits.iter().map(|w| w.0).max().unwrap();
        assert!(worst <= bound, "{name}: {worst} cycles, over {bound}");
    }
}

#[test]
fn without_the_gate_reset_a_release_faster_than_the_decay_waits_for_the_lfsr() {
    for (name, ad, sr) in RELEASE_FASTER_THAN_DECAY {
        let (decay, release) = (rate_period(ad & 0x0f), rate_period(sr & 0x0f));
        let bound = release + RELEASE_FROM_RELOAD;
        let waits: Vec<u32> = release_waits_per_decay_phase(ad, sr, 0, 40_000)
            .iter()
            .map(|w| w.0)
            .collect();
        let n = waits.iter().filter(|&&w| w <= bound).count();
        assert_eq!(
            n, release as usize,
            "{name}: {n} of {decay} phases start at once"
        );
        let late = SID_ENV_LFSR_PERIOD - decay;
        assert!(
            waits.iter().all(|&w| w <= bound || w > late),
            "{name}: {waits:?}"
        );
    }
}

#[test]
fn with_the_gate_reset_a_low_sustain_releases_after_the_divider_as_the_chip_does() {
    for s in 1..=5u8 {
        let sr = (s << 4) | 0x06;
        let release = rate_period(6);
        for (phase, &(wait, p, x)) in release_waits_per_decay_phase(0x09, sr, 1, 100_000)
            .iter()
            .enumerate()
        {
            assert!(p > 1, "sustain {s}: divider period {p}");
            let bound = (p - x) * release + 4;
            assert!(
                wait <= bound,
                "sustain {s}, phase {phase}: {wait} cycles, over {bound} (divider {x}/{p})"
            );
        }
    }
}

#[test]
fn with_the_gate_reset_a_counter_frozen_at_zero_stays_frozen_on_release() {
    for phase in [0u32, 1, 2, 100, 975, 976] {
        let mut ends = Vec::new();
        for gate_reset in [0u8, 1] {
            let mut e = powered_on();
            e.gate_reset = gate_reset;
            unsafe {
                sid_env_write_ad(&mut e, 0x09);
                sid_env_write_sr(&mut e, 0x06);
                sid_env_set_gate(&mut e, 1);
            }
            advance(&mut e, 2_000_000);
            assert_eq!((e.counter, e.counter_enabled), (0, 0), "frozen");
            advance(&mut e, phase);
            unsafe { sid_env_set_gate(&mut e, 0) };
            for _ in 0..1000 {
                advance(&mut e, 37);
                assert_eq!((e.counter, e.env3), (0, 0), "phase {phase}");
            }
            unsafe { sid_env_settle(&mut e) };
            assert_eq!(e.counter_enabled, 0);
            assert_eq!(e.state, SID_ENV_RELEASE);
            ends.push((e.exp_counter, e.exp_period, e.rate_pos));
        }
        assert_eq!(ends[0], ends[1], "phase {phase}");
    }
}

#[test]
fn raising_sustain_above_counter_during_decay_falls_to_zero_and_freezes() {
    let mut e = gated(0x00, 0x00);
    advance(&mut e, 9 * 256 + 20);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    while e.counter > 0x80 {
        advance(&mut e, 1);
    }
    unsafe { sid_env_write_sr(&mut e, SUSTAIN_0XFF) };
    advance(&mut e, 1_000_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
    advance(&mut e, 1_000_000);
    assert_eq!(e.counter, 0);
}

#[test]
fn gate_on_after_freeze_restarts_the_attack() {
    let mut e = released_to_zero_and_frozen();
    unsafe { sid_env_set_gate(&mut e, 1) };
    advance(&mut e, 2);
    assert_eq!(e.state, SID_ENV_ATTACK);
    assert_eq!(e.counter_enabled, 1);
    advance(&mut e, 40);
    assert!(e.counter > 0);
}

#[test]
fn a_frozen_envelope_rests_until_the_next_write() {
    let mut e = released_to_zero_and_frozen();
    advance(&mut e, 23);
    assert_eq!(e.dormant, 1);
    let before = e.idle;
    advance(&mut e, 23);
    assert_eq!(e.idle, before - 23);
    unsafe { sid_env_write_sr(&mut e, 0x00) };
    assert_eq!(e.dormant, 0);
    assert_eq!(e.idle, 0);
}

#[test]
fn a_fast_attack_rests_in_linear_runs_between_thresholds() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 23);
    assert_eq!(e.linear, 1);
    let mut r = e;
    unsafe { sid_env_settle(&mut r) };
    assert_eq!(r.linear, 0);
    assert_eq!(r.counter, e.counter);
}

#[test]
fn an_envelope_holding_sustain_rests_until_the_next_write() {
    for sr in [0xf0u8, 0xa0, 0x50, 0x10] {
        let mut e = gated(0x00, sr);
        advance(&mut e, 40_000);
        assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
        assert_eq!(e.counter, (sr >> 4) * 0x11);
        advance(&mut e, 23);
        assert_eq!(e.dormant, 1, "sustain {sr:#04x}");
    }
}

const ADSRTABLE: [u16; 16] = [
    0x007f, 0x3000, 0x1e00, 0x0660, 0x0182, 0x5573, 0x000e, 0x3805, 0x2424, 0x2220, 0x090c, 0x0ecd,
    0x010e, 0x23f7, 0x5237, 0x64a8,
];

fn lfsr_shift(lfsr: u16) -> u16 {
    let feedback = ((lfsr << 14) ^ (lfsr << 13)) & 0x4000;
    (lfsr >> 1) | feedback
}

fn lfsr_positions() -> Vec<u32> {
    let mut pos = vec![u32::MAX; 0x8000];
    let mut lfsr = 0x7fffu16;
    for k in 0..0x7fff {
        pos[lfsr as usize] = k;
        lfsr = lfsr_shift(lfsr);
    }
    assert_eq!(lfsr, 0x7fff, "the LFSR has period 32767");
    pos
}

#[test]
fn rate_positions_are_the_lfsr_table_values() {
    let pos = lfsr_positions();
    for (i, &v) in ADSRTABLE.iter().enumerate() {
        assert_eq!(
            unsafe { sid_env_rate_pos(i as u8) },
            pos[v as usize],
            "rate {i}"
        );
    }
}

#[derive(Clone)]
struct RefEnv {
    lfsr: u16,
    rate: u16,
    exponential_counter: u32,
    exponential_counter_period: u32,
    new_exponential_counter_period: u32,
    state_pipeline: u32,
    envelope_pipeline: u32,
    exponential_pipeline: u32,
    state: sid_env_state,
    next_state: sid_env_state,
    counter_enabled: bool,
    gate: bool,
    reset_lfsr: bool,
    envelope_counter: u8,
    attack: u8,
    decay: u8,
    sustain: u8,
    release: u8,
    env3: u8,
    gate_reset: bool,
}

impl RefEnv {
    fn new(counter: u8) -> Self {
        let mut e = Self {
            lfsr: 0x7fff,
            rate: 0,
            exponential_counter: 0,
            exponential_counter_period: 1,
            new_exponential_counter_period: 0,
            state_pipeline: 0,
            envelope_pipeline: 0,
            exponential_pipeline: 0,
            state: SID_ENV_RELEASE,
            next_state: SID_ENV_RELEASE,
            counter_enabled: true,
            gate: false,
            reset_lfsr: false,
            envelope_counter: counter,
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
            env3: 0,
            gate_reset: false,
        };
        e.reset();
        e
    }

    fn reset(&mut self) {
        self.envelope_pipeline = 0;
        self.state_pipeline = 0;
        self.attack = 0;
        self.decay = 0;
        self.sustain = 0;
        self.release = 0;
        self.gate = false;
        self.reset_lfsr = true;
        self.exponential_counter = 0;
        self.exponential_counter_period = 1;
        self.new_exponential_counter_period = 0;
        self.state = SID_ENV_RELEASE;
        self.counter_enabled = true;
        self.rate = ADSRTABLE[self.release as usize];
    }

    fn write_control(&mut self, control: u8) {
        let gate_next = control & 0x01 != 0;
        if gate_next == self.gate {
            return;
        }
        self.gate = gate_next;
        if self.gate_reset {
            self.lfsr = 0x7fff;
        }
        if gate_next {
            self.next_state = SID_ENV_ATTACK;
            self.state_pipeline = 2;
            if self.reset_lfsr || self.exponential_pipeline == 2 {
                self.envelope_pipeline =
                    if self.exponential_counter_period == 1 || self.exponential_pipeline == 2 {
                        2
                    } else {
                        4
                    };
            } else if self.exponential_pipeline == 1 {
                self.state_pipeline = 3;
            }
        } else {
            self.next_state = SID_ENV_RELEASE;
            self.state_pipeline = if self.envelope_pipeline > 0 { 3 } else { 2 };
        }
    }

    fn write_ad(&mut self, v: u8) {
        self.attack = (v >> 4) & 0x0f;
        self.decay = v & 0x0f;
        if self.state == SID_ENV_ATTACK {
            self.rate = ADSRTABLE[self.attack as usize];
        } else if self.state == SID_ENV_DECAY_SUSTAIN {
            self.rate = ADSRTABLE[self.decay as usize];
        }
    }

    fn write_sr(&mut self, v: u8) {
        self.sustain = (v & 0xf0) | ((v >> 4) & 0x0f);
        self.release = v & 0x0f;
        if self.state == SID_ENV_RELEASE {
            self.rate = ADSRTABLE[self.release as usize];
        }
    }

    fn clock(&mut self) {
        self.env3 = self.envelope_counter;
        if self.new_exponential_counter_period > 0 {
            self.exponential_counter_period = self.new_exponential_counter_period;
            self.new_exponential_counter_period = 0;
        }
        if self.state_pipeline != 0 {
            self.state_change();
        }
        if self.envelope_pipeline != 0 && {
            self.envelope_pipeline -= 1;
            self.envelope_pipeline == 0
        } {
            if self.counter_enabled {
                if self.state == SID_ENV_ATTACK {
                    self.envelope_counter = self.envelope_counter.wrapping_add(1);
                    if self.envelope_counter == 0xff {
                        self.next_state = SID_ENV_DECAY_SUSTAIN;
                        self.state_pipeline = 3;
                    }
                } else {
                    self.envelope_counter = self.envelope_counter.wrapping_sub(1);
                    if self.envelope_counter == 0 {
                        self.counter_enabled = false;
                    }
                }
                self.set_exponential_counter();
            }
        } else if self.exponential_pipeline != 0 && {
            self.exponential_pipeline -= 1;
            self.exponential_pipeline == 0
        } {
            self.exponential_counter = 0;
            if (self.state == SID_ENV_DECAY_SUSTAIN && self.envelope_counter != self.sustain)
                || self.state == SID_ENV_RELEASE
            {
                self.envelope_pipeline = 1;
            }
        } else if self.reset_lfsr {
            self.lfsr = 0x7fff;
            self.reset_lfsr = false;
            if self.state == SID_ENV_ATTACK {
                self.exponential_counter = 0;
                self.envelope_pipeline = 2;
            } else if self.counter_enabled && {
                self.exponential_counter += 1;
                self.exponential_counter == self.exponential_counter_period
            } {
                self.exponential_pipeline = if self.exponential_counter_period != 1 {
                    2
                } else {
                    1
                };
            }
        }
        if self.lfsr != self.rate {
            self.lfsr = lfsr_shift(self.lfsr);
        } else {
            self.reset_lfsr = true;
        }
    }

    fn state_change(&mut self) {
        self.state_pipeline -= 1;
        match self.next_state {
            SID_ENV_ATTACK => {
                if self.state_pipeline == 1 {
                    self.rate = ADSRTABLE[self.decay as usize];
                } else if self.state_pipeline == 0 {
                    self.state = SID_ENV_ATTACK;
                    self.rate = ADSRTABLE[self.attack as usize];
                    self.counter_enabled = true;
                }
            }
            SID_ENV_DECAY_SUSTAIN => {
                if self.state_pipeline == 0 {
                    self.state = SID_ENV_DECAY_SUSTAIN;
                    self.rate = ADSRTABLE[self.decay as usize];
                }
            }
            SID_ENV_RELEASE => {
                if (self.state == SID_ENV_ATTACK && self.state_pipeline == 0)
                    || (self.state == SID_ENV_DECAY_SUSTAIN && self.state_pipeline == 1)
                {
                    self.state = SID_ENV_RELEASE;
                    self.rate = ADSRTABLE[self.release as usize];
                }
            }
            _ => unreachable!(),
        }
    }

    fn set_exponential_counter(&mut self) {
        match self.envelope_counter {
            0xff | 0x00 => self.new_exponential_counter_period = 1,
            0x5d => self.new_exponential_counter_period = 2,
            0x36 => self.new_exponential_counter_period = 4,
            0x1a => self.new_exponential_counter_period = 8,
            0x0e => self.new_exponential_counter_period = 16,
            0x06 => self.new_exponential_counter_period = 30,
            _ => {}
        }
    }
}

fn assert_settled_copy_matches_reference(e: &sid_env, r: &RefEnv, pos: &[u32], step: usize) {
    assert_resting_invariants(e);
    assert_eq!(
        e.counter, r.envelope_counter,
        "counter at rest, step {step}"
    );
    assert_eq!(e.env3, r.env3, "env3 at rest, step {step}");
    let mut e = *e;
    unsafe { sid_env_settle(&mut e) };
    let e = &e;
    let at = format!("step {step}");
    assert_eq!(e.counter, r.envelope_counter, "counter, {at}");
    assert_eq!(e.env3, r.env3, "env3, {at}");
    assert_eq!(e.state, r.state, "state, {at}");
    assert_eq!(e.lfsr_pos, pos[r.lfsr as usize], "LFSR, {at}");
    assert_eq!(e.rate_pos, pos[r.rate as usize], "rate, {at}");
    assert_eq!(e.reset_lfsr != 0, r.reset_lfsr, "LFSR reset, {at}");
    assert_eq!(e.exp_counter, r.exponential_counter, "exp counter, {at}");
    assert_eq!(
        e.exp_period, r.exponential_counter_period,
        "exp period, {at}"
    );
    assert_eq!(
        e.new_exp_period, r.new_exponential_counter_period,
        "new exp period, {at}"
    );
    assert_eq!(
        u32::from(e.state_pipeline),
        r.state_pipeline,
        "state pipeline, {at}"
    );
    assert_eq!(
        u32::from(e.envelope_pipeline),
        r.envelope_pipeline,
        "envelope pipeline, {at}"
    );
    assert_eq!(
        u32::from(e.exponential_pipeline),
        r.exponential_pipeline,
        "exponential pipeline, {at}"
    );
    assert_eq!(e.counter_enabled != 0, r.counter_enabled, "enabled, {at}");
    assert_eq!(e.gate != 0, r.gate, "gate, {at}");
    if r.state_pipeline != 0 {
        assert_eq!(e.next_state, r.next_state, "next state, {at}");
    }
}

#[test]
fn jump_ahead_matches_per_cycle_reference() {
    let pos = lfsr_positions();
    let mut falls = FallingEdges::default();
    for (seed, gate_reset) in [
        (0x51d0u32, false),
        (0xbeef, false),
        (0x1234_5678, false),
        (0x51d0, true),
        (0xbeef, true),
        (0x1234_5678, true),
        (0x0bad_cafe, true),
    ] {
        let mut e = powered_on();
        e.gate_reset = u8::from(gate_reset);
        let mut r = RefEnv::new(0xaa);
        r.gate_reset = gate_reset;
        let mut rng = Lcg(seed);
        for step in 0..20_000 {
            let op = rng.next_u32() % 16;
            if op == 0 {
                let v = rng.next_u32() as u8;
                unsafe { sid_env_write_ad(&mut e, v) };
                r.write_ad(v);
            } else if op == 1 {
                let v = rng.next_u32() as u8;
                unsafe { sid_env_write_sr(&mut e, v) };
                r.write_sr(v);
            } else if op <= 3 {
                toggle_gate_counting_falls_with_the_reset(&mut e, &mut r, &mut falls);
            } else if op == 4 {
                if rng.next_u32().is_multiple_of(2) {
                    toggle_gate_counting_falls_with_the_reset(&mut e, &mut r, &mut falls);
                    let cycles = rng.next_u32() % 3;
                    advance(&mut e, cycles);
                    for _ in 0..cycles {
                        r.clock();
                    }
                }
                unsafe { sid_env_reset(&mut e) };
                r.reset();
            } else {
                let cycles = match op {
                    5 => rng.next_u32() % 70_001,
                    6..=8 => rng.next_u32() % 2_000,
                    9..=12 => rng.next_u32() % 6,
                    _ => 20 + rng.next_u32() % 4,
                };
                advance(&mut e, cycles);
                for _ in 0..cycles {
                    r.clock();
                }
            }
            assert_settled_copy_matches_reference(&e, &r, &pos, step);
        }
    }
    assert!(falls.all > 4_000, "{falls:?}");
    assert!(falls.in_pipeline > 1_000, "{falls:?}");
    assert!(falls.divided > 1_500, "{falls:?}");
    assert!(falls.frozen > 500, "{falls:?}");
}

#[derive(Debug, Default)]
struct FallingEdges {
    all: u32,
    in_pipeline: u32,
    divided: u32,
    frozen: u32,
}

fn toggle_gate_counting_falls_with_the_reset(
    e: &mut sid_env,
    r: &mut RefEnv,
    falls: &mut FallingEdges,
) {
    let g = !r.gate;
    if !g && r.gate_reset {
        falls.all += 1;
        falls.in_pipeline += u32::from(
            r.state_pipeline + r.envelope_pipeline + r.exponential_pipeline != 0 || r.reset_lfsr,
        );
        falls.divided += u32::from(r.exponential_counter_period > 1);
        falls.frozen += u32::from(!r.counter_enabled);
    }
    unsafe { sid_env_set_gate(e, u8::from(g)) };
    r.write_control(u8::from(g));
}

#[test]
fn resting_steady_states_match_per_cycle_reference() {
    let pos = lfsr_positions();
    for sr in [0xf0u8, 0x50, 0x30, 0x10, 0x00] {
        for decay in [0u8, 3, 9] {
            let mut e = powered_on();
            let mut r = RefEnv::new(0xaa);
            unsafe {
                sid_env_write_ad(&mut e, decay);
                sid_env_write_sr(&mut e, sr);
                sid_env_set_gate(&mut e, 1);
            }
            r.write_ad(decay);
            r.write_sr(sr);
            r.write_control(1);
            let mut rng = Lcg(u32::from(sr) << 8 | u32::from(decay));
            for step in 0..3_000 {
                let cycles = match rng.next_u32() % 8 {
                    0 => rng.next_u32() % 50_000,
                    1 => {
                        unsafe { sid_env_write_sr(&mut e, sr) };
                        r.write_sr(sr);
                        0
                    }
                    _ => 1 + rng.next_u32() % 40,
                };
                advance(&mut e, cycles);
                for _ in 0..cycles {
                    r.clock();
                }
                assert_settled_copy_matches_reference(&e, &r, &pos, step);
            }
            assert_eq!(e.counter, (sr >> 4) * 0x11, "sr {sr:#04x}, decay {decay}");
        }
    }
}

#[test]
fn rests_longer_than_2_to_the_32_cycles() {
    for (sr, divider) in [(0x00u8, 1u64), (0xf0, 1), (0x50, 2), (0x30, 4), (0x10, 8)] {
        for decay in [0u8, 9] {
            let mut start = gated(decay, sr);
            advance(&mut start, 2_000_000);
            assert_eq!(start.counter, (sr >> 4) * 0x11, "sr {sr:#04x}");
            let pattern = u64::from(unsafe { sid_env_rate_pos(decay) } + 1) * divider;
            for total in [(1u64 << 32) + 12_345, (3u64 << 32) + 7, 5u64 << 31] {
                let mut long = start;
                let mut left = total;
                while left > 0 {
                    let step = left.min(1 << 30) as u32;
                    advance(&mut long, step);
                    left -= u64::from(step);
                }
                let mut short = start;
                advance(&mut short, (total % pattern) as u32);
                unsafe {
                    sid_env_settle(&mut long);
                    sid_env_settle(&mut short);
                }
                let at = format!("sr {sr:#04x}, decay {decay}, {total} cycles");
                assert_eq!(long.counter, short.counter, "{at}");
                assert_eq!(long.env3, short.env3, "{at}");
                assert_eq!(long.lfsr_pos, short.lfsr_pos, "{at}");
                assert_eq!(long.exp_counter, short.exp_counter, "{at}");
                assert_eq!(long.reset_lfsr, short.reset_lfsr, "{at}");
                assert_eq!(long.envelope_pipeline, short.envelope_pipeline, "{at}");
                assert_eq!(
                    long.exponential_pipeline, short.exponential_pipeline,
                    "{at}"
                );
            }
        }
    }
}
