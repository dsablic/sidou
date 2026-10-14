// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

/// An envelope as after power-on and reset: counter 0xaa, releasing at rate 0.
fn powered_on() -> sid_env {
    let mut e = sid_env {
        counter: 0xaa,
        ..Default::default()
    };
    unsafe { sid_env_reset(&mut e) };
    e
}

/// Released to 0 and frozen there, as long after power-on.
fn silent() -> sid_env {
    let mut e = powered_on();
    advance(&mut e, 100_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
    e
}

fn gated(ad: u8, sr: u8) -> sid_env {
    let mut e = silent();
    unsafe {
        sid_env_write_ad(&mut e, ad);
        sid_env_write_sr(&mut e, sr);
        sid_env_set_gate(&mut e, 1);
    }
    e
}

fn advance(e: &mut sid_env, cycles: u32) {
    unsafe { sid_env_advance(e, cycles) }
}

/// Cycles until the counter next changes, at most `limit`.
fn cycles_to_next_step(e: &mut sid_env, limit: u32) -> Option<u32> {
    let start = e.counter;
    (1..=limit).find(|_| {
        advance(e, 1);
        e.counter != start
    })
}

#[test]
fn reset_keeps_the_counter_and_releases_from_it_three_cycles_in() {
    // reSIDfp: "counter is not changed on reset". Reset sets the rate LFSR to reload on
    // the next cycle, so the first release step comes 3 cycles in (LFSR reload,
    // exponential pipeline, envelope pipeline), then one every 9 cycles.
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
    // reSIDfp's reset enables the counter whatever its value, so a counter at 0 steps
    // down to 0xff and releases from there.
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
    // The first step at period 2 comes a cycle later still: from period 2 on, the
    // exponential pipeline takes two cycles instead of one.
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
    // The ADSR delay bug: attack 15 has a period of 31251. 1000 cycles in, the period
    // drops to 9, but the LFSR is already past that position and has to run through
    // all 32767 states before it matches again.
    let mut e = gated(0xf0, 0xf0);
    advance(&mut e, 1000);
    let counter = e.counter;
    unsafe { sid_env_write_ad(&mut e, 0x00) };
    advance(&mut e, 30_000);
    assert_eq!(e.counter, counter);
    let wait = cycles_to_next_step(&mut e, 5_000).expect("a step after the wrap");
    assert!(wait < 3_000, "{wait}");
}

#[test]
fn raising_sustain_above_counter_during_decay_falls_to_zero_and_freezes() {
    let mut e = gated(0x00, 0x00);
    advance(&mut e, 9 * 256 + 20);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    while e.counter > 0x80 {
        advance(&mut e, 1);
    }
    unsafe { sid_env_write_sr(&mut e, 0xf0) }; // sustain level 0xff, above the counter
    advance(&mut e, 1_000_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.counter_enabled, 0);
    advance(&mut e, 1_000_000);
    assert_eq!(e.counter, 0);
}

#[test]
fn gate_on_after_freeze_restarts_the_attack() {
    let mut e = silent();
    unsafe { sid_env_set_gate(&mut e, 1) };
    advance(&mut e, 2);
    assert_eq!(e.state, SID_ENV_ATTACK);
    assert_eq!(e.counter_enabled, 1);
    advance(&mut e, 40);
    assert!(e.counter > 0);
}

#[test]
fn a_frozen_envelope_rests_until_the_next_write() {
    let mut e = silent();
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

/// reSIDfp's rate table: the LFSR value that ends each rate's period.
const ADSRTABLE: [u16; 16] = [
    0x007f, 0x3000, 0x1e00, 0x0660, 0x0182, 0x5573, 0x000e, 0x3805, 0x2424, 0x2220, 0x090c, 0x0ecd,
    0x010e, 0x23f7, 0x5237, 0x64a8,
];

fn lfsr_shift(lfsr: u16) -> u16 {
    let feedback = ((lfsr << 14) ^ (lfsr << 13)) & 0x4000;
    (lfsr >> 1) | feedback
}

/// Shifts from 0x7fff to each LFSR value: the positions `sid_env` counts in.
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

/// Independent per-cycle port of reSIDfp's EnvelopeGenerator (libresidfp at c1791383),
/// with the real 15-bit rate LFSR.
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
}

impl RefEnv {
    /// The class's initial values, with `counter` in place of 0xaa, then reset().
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

/// Compares a settled copy, so `e` itself may stay at rest across steps. The counter
/// and ENV3 must be current even at rest: the audio path and $1C read them directly.
fn assert_same(e: &sid_env, r: &RefEnv, pos: &[u32], step: usize) {
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

struct Lcg {
    x: u32,
}

impl Lcg {
    fn next(&mut self) -> u32 {
        self.x = self.x.wrapping_mul(1664525).wrapping_add(1013904223);
        self.x >> 8
    }
}

/// sid_env_advance jumps over quiet stretches; reSIDfp clocks every cycle. Random
/// writes at random times, including within a few cycles of each other so they land in
/// the state-change and envelope pipelines, must leave both in the same state.
#[test]
fn jump_ahead_matches_per_cycle_reference() {
    let pos = lfsr_positions();
    for seed in [0x51d0u32, 0xbeef, 0x1234_5678] {
        let mut e = powered_on();
        let mut r = RefEnv::new(0xaa);
        let mut rng = Lcg { x: seed };
        for step in 0..20_000 {
            let op = rng.next() % 16;
            if op == 0 {
                let v = rng.next() as u8;
                unsafe { sid_env_write_ad(&mut e, v) };
                r.write_ad(v);
            } else if op == 1 {
                let v = rng.next() as u8;
                unsafe { sid_env_write_sr(&mut e, v) };
                r.write_sr(v);
            } else if op <= 3 {
                let g = !r.gate;
                unsafe { sid_env_set_gate(&mut e, u8::from(g)) };
                r.write_control(u8::from(g));
            } else if op == 4 {
                // A reset, half the time right after a gate change, with its state and
                // envelope pipelines still pending.
                if rng.next().is_multiple_of(2) {
                    let g = !r.gate;
                    unsafe { sid_env_set_gate(&mut e, u8::from(g)) };
                    r.write_control(u8::from(g));
                    let cycles = rng.next() % 3;
                    advance(&mut e, cycles);
                    for _ in 0..cycles {
                        r.clock();
                    }
                }
                unsafe { sid_env_reset(&mut e) };
                r.reset();
            } else {
                let cycles = match op {
                    5 => rng.next() % 70_001,
                    6..=8 => rng.next() % 2_000,
                    9..=12 => rng.next() % 6,
                    _ => 20 + rng.next() % 4,
                };
                advance(&mut e, cycles);
                for _ in 0..cycles {
                    r.clock();
                }
            }
            assert_same(&e, &r, &pos, step);
        }
    }
}

/// Steady states rest across calls; waking them (here: settling a copy after every
/// call, and a write now and then) must land where reSIDfp's per-cycle clock does,
/// at every divider period a held sustain level can have (1, 2, 4, 8) and when frozen.
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
            let mut rng = Lcg {
                x: u32::from(sr) << 8 | u32::from(decay),
            };
            for step in 0..3_000 {
                let cycles = match rng.next() % 8 {
                    0 => rng.next() % 50_000,
                    1 => {
                        unsafe { sid_env_write_sr(&mut e, sr) };
                        r.write_sr(sr);
                        0
                    }
                    _ => 1 + rng.next() % 40,
                };
                advance(&mut e, cycles);
                for _ in 0..cycles {
                    r.clock();
                }
                assert_same(&e, &r, &pos, step);
            }
            assert_eq!(e.counter, (sr >> 4) * 0x11, "sr {sr:#04x}, decay {decay}");
        }
    }
}
