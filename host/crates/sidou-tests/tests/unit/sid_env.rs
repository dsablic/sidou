// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

fn reset() -> sid_env {
    let mut e = sid_env::default();
    unsafe { sid_env_reset(&mut e) };
    e
}

fn gated(ad: u8, sr: u8) -> sid_env {
    let mut e = reset();
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

#[test]
fn starts_silent_and_frozen() {
    let mut e = reset();
    advance(&mut e, 100000);
    assert_eq!(e.counter, 0);
}

#[test]
fn fastest_attack_steps_every_nine_cycles() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 8);
    assert_eq!(e.counter, 0);
    advance(&mut e, 1);
    assert_eq!(e.counter, 1);
}

#[test]
fn attack_reaches_full_scale_then_decays() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 9 * 255);
    assert_eq!(e.counter, 0xff);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
}

#[test]
fn decay_stops_at_sustain_level() {
    let mut e = gated(0x00, 0x80);
    advance(&mut e, 9 * 255 + 9 * (0xff - 0x88));
    assert_eq!(e.counter, 0x88);
    advance(&mut e, 100000);
    assert_eq!(e.counter, 0x88);
}

#[test]
fn exponential_divider_doubles_step_time_below_0x5d() {
    let mut e = gated(0x00, 0x00);
    advance(&mut e, 9 * 255); // finish the attack, which also passes 0x5d
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    while e.counter != 0x5d {
        advance(&mut e, 1);
    }
    let mut cycles = 0u32;
    while e.counter == 0x5d {
        advance(&mut e, 1);
        cycles += 1;
    }
    assert_eq!(cycles, 18);
}

#[test]
fn release_freezes_at_zero() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 9 * 255);
    unsafe { sid_env_set_gate(&mut e, 0) };
    advance(&mut e, 2000000);
    assert_eq!(e.counter, 0);
}

#[test]
fn lowering_the_period_below_the_counter_wraps_through_0x7fff() {
    let mut e = gated(0xf0, 0xf0); // attack 15: period 31251
    advance(&mut e, 1000);
    assert_eq!(e.counter, 0);
    unsafe { sid_env_write_ad(&mut e, 0x00) }; // period 9, but the counter is already at 1000
    advance(&mut e, 0x7fff - 1000 + 9 - 1);
    assert_eq!(e.counter, 0);
    advance(&mut e, 1);
    assert_eq!(e.counter, 1);
}

#[test]
fn raising_sustain_above_counter_during_decay_falls_to_zero_and_freezes() {
    let mut e = gated(0x00, 0x00);
    advance(&mut e, 9 * 255);
    assert_eq!(e.state, SID_ENV_DECAY_SUSTAIN);
    while e.counter > 0x80 {
        advance(&mut e, 1);
    }
    unsafe { sid_env_write_sr(&mut e, 0xf0) }; // sustain level 0xff, above the counter
    advance(&mut e, 1000000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.hold_zero, 1);
    advance(&mut e, 1000000);
    assert_eq!(e.counter, 0);
}

#[test]
fn gate_off_mid_attack_keeps_attack_exponential_period() {
    let mut e = gated(0x00, 0xf0);
    // Gate off before attack reaches 0x06, the first threshold that changes the period.
    advance(&mut e, 9 * 5);
    assert_eq!(e.counter, 5);
    assert_eq!(e.exp_period, 1);
    unsafe { sid_env_set_gate(&mut e, 0) };
    // A release from sustain would step every 30 rate periods below 0x06; this one keeps 1.
    advance(&mut e, 9);
    assert_eq!(e.counter, 4);
    advance(&mut e, 9);
    assert_eq!(e.counter, 3);
    assert_eq!(e.exp_period, 1);
}

#[test]
fn gate_on_after_freeze_clears_hold_zero_and_restarts_attack() {
    let mut e = gated(0x00, 0xf0);
    advance(&mut e, 9 * 255);
    unsafe { sid_env_set_gate(&mut e, 0) };
    advance(&mut e, 2000000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.hold_zero, 1);
    assert!(e.rate_counter < 9);
    unsafe { sid_env_set_gate(&mut e, 1) };
    assert_eq!(e.hold_zero, 0);
    assert_eq!(e.state, SID_ENV_ATTACK);
    advance(&mut e, 9);
    assert_eq!(e.counter, 1);
}

#[test]
fn ad_write_equal_to_rate_counter_waits_full_wrap() {
    let mut e = gated(0x10, 0xf0); // attack 1: period 32
    advance(&mut e, 9);
    assert_eq!(e.rate_counter, 9);
    unsafe { sid_env_write_ad(&mut e, 0x00) }; // period 9 == rate_counter: the match was already missed
    advance(&mut e, 0x7fff - 1);
    assert_eq!(e.counter, 0);
    advance(&mut e, 1);
    assert_eq!(e.counter, 1);
}

/// Independent per-cycle model of reSID 0.16 EnvelopeGenerator::clock().
struct RefEnv {
    rate_counter: u32,
    rate_period: u32,
    exp_counter: u32,
    exp_period: u32,
    state: sid_env_state,
    counter: u32,
    attack: u32,
    decay: u32,
    sustain: u32,
    release: u32,
    gate: bool,
    hold_zero: bool,
}

impl RefEnv {
    const PERIOD_TABLE: [u32; 16] = [
        9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
    ];

    fn new() -> Self {
        Self {
            rate_counter: 0,
            rate_period: Self::PERIOD_TABLE[0],
            exp_counter: 0,
            exp_period: 1,
            state: SID_ENV_RELEASE,
            counter: 0,
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
            gate: false,
            hold_zero: true,
        }
    }

    fn write_ad(&mut self, v: u8) {
        self.attack = u32::from(v >> 4);
        self.decay = u32::from(v & 0x0f);
        if self.state == SID_ENV_ATTACK {
            self.rate_period = Self::PERIOD_TABLE[self.attack as usize];
        } else if self.state == SID_ENV_DECAY_SUSTAIN {
            self.rate_period = Self::PERIOD_TABLE[self.decay as usize];
        }
    }

    fn write_sr(&mut self, v: u8) {
        self.sustain = u32::from(v >> 4);
        self.release = u32::from(v & 0x0f);
        if self.state == SID_ENV_RELEASE {
            self.rate_period = Self::PERIOD_TABLE[self.release as usize];
        }
    }

    fn set_gate(&mut self, next: bool) {
        if !self.gate && next {
            self.state = SID_ENV_ATTACK;
            self.rate_period = Self::PERIOD_TABLE[self.attack as usize];
            self.hold_zero = false;
        } else if self.gate && !next {
            self.state = SID_ENV_RELEASE;
            self.rate_period = Self::PERIOD_TABLE[self.release as usize];
        }
        self.gate = next;
    }

    fn clock(&mut self) {
        self.rate_counter += 1;
        if self.rate_counter & 0x8000 != 0 {
            self.rate_counter = (self.rate_counter + 1) & 0x7fff;
        }
        if self.rate_counter != self.rate_period {
            return;
        }
        self.rate_counter = 0;
        if self.state != SID_ENV_ATTACK {
            self.exp_counter += 1;
            if self.exp_counter != self.exp_period {
                return;
            }
        }
        self.exp_counter = 0;
        if self.hold_zero {
            return;
        }
        match self.state {
            SID_ENV_ATTACK => {
                self.counter = (self.counter + 1) & 0xff;
                if self.counter == 0xff {
                    self.state = SID_ENV_DECAY_SUSTAIN;
                    self.rate_period = Self::PERIOD_TABLE[self.decay as usize];
                }
            }
            SID_ENV_DECAY_SUSTAIN => {
                if self.counter != self.sustain * 0x11 {
                    self.counter = self.counter.wrapping_sub(1);
                }
            }
            SID_ENV_RELEASE => {
                self.counter = self.counter.wrapping_sub(1) & 0xff;
            }
            _ => unreachable!(),
        }
        match self.counter {
            0xff => self.exp_period = 1,
            0x5d => self.exp_period = 2,
            0x36 => self.exp_period = 4,
            0x1a => self.exp_period = 8,
            0x0e => self.exp_period = 16,
            0x06 => self.exp_period = 30,
            0x00 => {
                self.exp_period = 1;
                self.hold_zero = true;
            }
            _ => {}
        }
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

#[test]
fn jump_ahead_matches_per_cycle_reference() {
    let mut e = reset();
    let mut r = RefEnv::new();
    let mut rng = Lcg { x: 0x51d0 };
    for step in 0..4000 {
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
            r.set_gate(g);
        } else {
            let cycles = if op <= 5 {
                rng.next() % 70001
            } else {
                22 + rng.next() % 2
            };
            advance(&mut e, cycles);
            for _ in 0..cycles {
                r.clock();
            }
        }
        assert_eq!(u32::from(e.counter), r.counter, "step {step}");
        assert_eq!(e.rate_counter, r.rate_counter, "step {step}");
        assert_eq!(e.exp_counter, r.exp_counter, "step {step}");
        assert_eq!(e.exp_period, r.exp_period, "step {step}");
        assert_eq!(e.rate_period, r.rate_period, "step {step}");
        assert_eq!(e.state, r.state, "step {step}");
        assert_eq!(e.hold_zero != 0, r.hold_zero, "step {step}");
    }
}

#[test]
fn reset_keeps_the_counter_and_releases_from_it() {
    // reSIDfp: "counter is not changed on reset".
    let mut e = sid_env::default();
    e.counter = 0xaa;
    unsafe { sid_env_reset(&mut e) };
    assert_eq!(e.counter, 0xaa);
    assert_eq!(e.hold_zero, 0);
    assert_eq!(e.state, SID_ENV_RELEASE);
    advance(&mut e, 9);
    assert_eq!(e.counter, 0xa9);
    advance(&mut e, 100_000);
    assert_eq!(e.counter, 0);
    assert_eq!(e.hold_zero, 1);
}
