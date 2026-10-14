// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU against reSIDfp, register by register: ENV3 (`$1C`) and OSC3 (`$1B`) must read
//! back exactly the same values at the same chip cycles, for both chip models.
//!
//! Each case runs a script of register writes on both engines and reads once per PAL
//! frame, or once every 100 cycles where the timing inside a frame matters. A read at
//! the start of frame N comes after frame N's writes; messages give its cycle and N. Cases that
//! fail for a known reason a later M1b task fixes are `#[ignore]`d with that reason;
//! `cargo test -- --ignored` shows where they stand.

use sidou_sys::{sid_model, SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::register_probe::{
    first_mismatch, probe, Residfp, Sidou, TimedWrite, CYCLES_PER_FRAME,
};

const ENV3: u8 = 0x1c;
const OSC3: u8 = 0x1b;

/* Voice 3's registers. */
const FREQ_LO: u8 = 0x0e;
const FREQ_HI: u8 = 0x0f;
const PW_LO: u8 = 0x10;
const PW_HI: u8 = 0x11;
const CONTROL: u8 = 0x12;
const AD: u8 = 0x13;
const SR: u8 = 0x14;

const GATE: u8 = 0x01;

/* reSID's rate periods, only to size the scripts: long enough for the slowest phase. */
const RATE_PERIOD: [u64; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
];
/* Rate periods from 0xff down to 0 through the exponential divider:
 * 162 * 1 + 39 * 2 + 28 * 4 + 12 * 8 + 8 * 16 + 6 * 30. */
const FALL_PERIODS: u64 = 756;

struct Script {
    writes: Vec<TimedWrite>,
    frames: u64,
    interval: u64,
}

fn at(frame: u64, reg: u8, value: u8) -> TimedWrite {
    TimedWrite {
        cycle: frame * CYCLES_PER_FRAME,
        reg,
        value,
    }
}

fn frames_for(cycles: u64) -> u64 {
    cycles.div_ceil(CYCLES_PER_FRAME)
}

fn model_name(model: sid_model) -> &'static str {
    if model == SID_MODEL_6581 {
        "MOS6581"
    } else {
        "CSG8580"
    }
}

/// None if every read matches, else where they first differ.
fn reads_mismatch(model: sid_model, reg: u8, script: &Script) -> Option<String> {
    let total = script.frames * CYCLES_PER_FRAME;
    let ours = probe(
        &mut Sidou::new(model),
        &script.writes,
        reg,
        script.interval,
        total,
    );
    let reference = probe(
        &mut Residfp::new(model),
        &script.writes,
        reg,
        script.interval,
        total,
    );
    first_mismatch(&ours, &reference).map(|m| {
        let at_cycle = |i: usize| (i as u64 + 1) * script.interval;
        let first_diffs: Vec<String> = (m.index..ours.len())
            .filter(|&i| ours[i] != reference[i])
            .take(8)
            .map(|i| format!("{}: {:#04x}/{:#04x}", at_cycle(i), ours[i], reference[i]))
            .collect();
        format!(
            "{}: ${reg:02X} first differs at cycle {} (frame {}): SIDOU {:#04x}, reSIDfp \
             {:#04x}; {} of {} reads differ; first (cycle: SIDOU/reSIDfp) {}",
            model_name(model),
            at_cycle(m.index),
            at_cycle(m.index) / CYCLES_PER_FRAME,
            m.ours,
            m.reference,
            m.count,
            ours.len(),
            first_diffs.join(", ")
        )
    })
}

/* ENV3 scripts. Voice 3's control register gets only the gate bit: ENV3 does not
 * depend on the waveform. Frame 0 runs the power-on release from 0xaa to 0. */

fn attack(rate: u8) -> Script {
    Script {
        writes: vec![at(0, AD, rate << 4), at(0, SR, 0xf0), at(1, CONTROL, GATE)],
        frames: 1 + frames_for(RATE_PERIOD[rate as usize] * 255) + 10,
        interval: CYCLES_PER_FRAME,
    }
}

fn decay(rate: u8) -> Script {
    Script {
        writes: vec![at(0, AD, rate), at(0, SR, 0x00), at(1, CONTROL, GATE)],
        frames: 2 + frames_for(RATE_PERIOD[rate as usize] * FALL_PERIODS) + 10,
        interval: CYCLES_PER_FRAME,
    }
}

fn release(rate: u8) -> Script {
    Script {
        writes: vec![
            at(0, AD, 0x00),
            at(0, SR, 0xf0 | rate),
            at(1, CONTROL, GATE),
            at(3, CONTROL, 0x00),
        ],
        frames: 3 + frames_for(RATE_PERIOD[rate as usize] * FALL_PERIODS) + 10,
        interval: CYCLES_PER_FRAME,
    }
}

/* Attack 8 and decay 8 (392 cycles per step) take about 5 frames from 0 to 0xff, and
 * about 4 more down to sustain 0x44; release 8 takes about 15 frames to 0. */
fn gate_toggles(interval: u64) -> Script {
    Script {
        writes: vec![
            at(0, AD, 0x88),
            at(0, SR, 0x48),
            at(1, CONTROL, GATE),
            at(3, CONTROL, 0x00),  // mid-attack
            at(5, CONTROL, GATE),  // mid-release
            at(12, CONTROL, 0x00), // mid-decay
            at(14, CONTROL, GATE), // mid-release
            at(30, CONTROL, 0x00), // from sustain
        ],
        frames: 60,
        interval,
    }
}

/* The ADSR delay bug: attack 15's period is 31251. One frame into it the rate counter
 * is at about 19705; lowering the period to 9 makes it count on to 0x7fff and wrap
 * before the first step. */
fn adsr_delay_bug(interval: u64) -> Script {
    Script {
        writes: vec![
            at(0, AD, 0xf0),
            at(0, SR, 0xf0),
            at(1, CONTROL, GATE),
            at(2, AD, 0x00),
        ],
        frames: 8,
        interval,
    }
}

/* Decay 9 (977 cycles per step) from 0xff: sustain moves down mid-decay, then up past
 * the counter, which then falls through to 0 and freezes there. */
fn sustain_changes() -> Script {
    Script {
        writes: vec![
            at(0, AD, 0x09),
            at(0, SR, 0x80),
            at(1, CONTROL, GATE),
            at(4, SR, 0x40),
            at(9, SR, 0xc0),
        ],
        frames: 150,
        interval: CYCLES_PER_FRAME,
    }
}

/* No writes at all: the power-on release from 0xaa, every 100 cycles. */
fn power_on() -> Script {
    Script {
        writes: Vec::new(),
        frames: 1,
        interval: 100,
    }
}

macro_rules! env3_cases {
    ($($(#[$attr:meta])* $name:ident: $model:expr, $script:expr;)*) => {$(
        #[test]
        $(#[$attr])*
        fn $name() {
            if let Some(m) = reads_mismatch($model, ENV3, &$script) {
                panic!("{m}");
            }
        }
    )*};
}

/* OSC3: voice 3 alone, one waveform, at a few frequencies over 100 frames. */

const FREQS: [u16; 4] = [0x0123, 0x1d45, 0x7531, 0xfedc];

fn single_waveform(control: u8, pw: u16, freq: u16) -> Script {
    Script {
        writes: vec![
            at(0, FREQ_LO, freq as u8),
            at(0, FREQ_HI, (freq >> 8) as u8),
            at(0, PW_LO, pw as u8),
            at(0, PW_HI, (pw >> 8) as u8),
            at(0, CONTROL, control),
        ],
        frames: 100,
        interval: CYCLES_PER_FRAME,
    }
}

fn assert_osc3_matches(model: sid_model, control: u8, pw: u16) {
    let failures: Vec<String> = FREQS
        .iter()
        .filter_map(|&freq| {
            reads_mismatch(model, OSC3, &single_waveform(control, pw, freq))
                .map(|m| format!("control {control:#04x}, pw {pw:#05x}, freq {freq:#06x}: {m}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

macro_rules! osc3_cases {
    ($($(#[$attr:meta])* $name:ident: $model:expr, $control:expr, $pw:expr;)*) => {$(
        #[test]
        $(#[$attr])*
        fn $name() {
            assert_osc3_matches($model, $control, $pw);
        }
    )*};
}

env3_cases! {
    env3_attack_0_mos6581: SID_MODEL_6581, attack(0);
    env3_attack_1_mos6581: SID_MODEL_6581, attack(1);
    env3_attack_2_mos6581: SID_MODEL_6581, attack(2);
    env3_attack_3_mos6581: SID_MODEL_6581, attack(3);
    env3_attack_4_mos6581: SID_MODEL_6581, attack(4);
    env3_attack_5_mos6581: SID_MODEL_6581, attack(5);
    env3_attack_6_mos6581: SID_MODEL_6581, attack(6);
    env3_attack_7_mos6581: SID_MODEL_6581, attack(7);
    env3_attack_8_mos6581: SID_MODEL_6581, attack(8);
    env3_attack_9_mos6581: SID_MODEL_6581, attack(9);
    env3_attack_10_mos6581: SID_MODEL_6581, attack(10);
    env3_attack_11_mos6581: SID_MODEL_6581, attack(11);
    env3_attack_12_mos6581: SID_MODEL_6581, attack(12);
    env3_attack_13_mos6581: SID_MODEL_6581, attack(13);
    env3_attack_14_mos6581: SID_MODEL_6581, attack(14);
    env3_attack_15_mos6581: SID_MODEL_6581, attack(15);
    env3_decay_0_mos6581: SID_MODEL_6581, decay(0);
    env3_decay_1_mos6581: SID_MODEL_6581, decay(1);
    env3_decay_2_mos6581: SID_MODEL_6581, decay(2);
    env3_decay_3_mos6581: SID_MODEL_6581, decay(3);
    env3_decay_4_mos6581: SID_MODEL_6581, decay(4);
    env3_decay_5_mos6581: SID_MODEL_6581, decay(5);
    env3_decay_6_mos6581: SID_MODEL_6581, decay(6);
    env3_decay_7_mos6581: SID_MODEL_6581, decay(7);
    env3_decay_8_mos6581: SID_MODEL_6581, decay(8);
    env3_decay_9_mos6581: SID_MODEL_6581, decay(9);
    env3_decay_10_mos6581: SID_MODEL_6581, decay(10);
    env3_decay_11_mos6581: SID_MODEL_6581, decay(11);
    env3_decay_12_mos6581: SID_MODEL_6581, decay(12);
    env3_decay_13_mos6581: SID_MODEL_6581, decay(13);
    env3_decay_14_mos6581: SID_MODEL_6581, decay(14);
    env3_decay_15_mos6581: SID_MODEL_6581, decay(15);
    env3_release_0_mos6581: SID_MODEL_6581, release(0);
    env3_release_1_mos6581: SID_MODEL_6581, release(1);
    env3_release_2_mos6581: SID_MODEL_6581, release(2);
    env3_release_3_mos6581: SID_MODEL_6581, release(3);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_4_mos6581: SID_MODEL_6581, release(4);
    #[ignore = "M1b Task 2: one step apart mid-release (first at frame 4: 0xa6 vs 0xa5)"]
    env3_release_5_mos6581: SID_MODEL_6581, release(5);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_6_mos6581: SID_MODEL_6581, release(6);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_7_mos6581: SID_MODEL_6581, release(7);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_8_mos6581: SID_MODEL_6581, release(8);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_9_mos6581: SID_MODEL_6581, release(9);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_10_mos6581: SID_MODEL_6581, release(10);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_11_mos6581: SID_MODEL_6581, release(11);
    #[ignore = "M1b Task 2: power-on release one step ahead, then one step apart through the release"]
    env3_release_12_mos6581: SID_MODEL_6581, release(12);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_13_mos6581: SID_MODEL_6581, release(13);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_14_mos6581: SID_MODEL_6581, release(14);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_15_mos6581: SID_MODEL_6581, release(15);
    #[ignore = "M1b Task 2: one step apart from the power-on release on and around gate changes"]
    env3_gate_toggles_per_frame_mos6581: SID_MODEL_6581, gate_toggles(CYCLES_PER_FRAME);
    #[ignore = "M1b Task 2: one step apart from the power-on release on and around gate changes"]
    env3_gate_toggles_per_100_cycles_mos6581: SID_MODEL_6581, gate_toggles(100);
    env3_adsr_delay_bug_per_frame_mos6581: SID_MODEL_6581, adsr_delay_bug(CYCLES_PER_FRAME);
    #[ignore = "M1b Task 2: power-on release one step ahead; one step apart after the delay-bug wrap"]
    env3_adsr_delay_bug_per_100_cycles_mos6581: SID_MODEL_6581, adsr_delay_bug(100);
    env3_sustain_changes_mos6581: SID_MODEL_6581, sustain_changes();
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_power_on_release_mos6581: SID_MODEL_6581, power_on();
    env3_attack_0_csg8580: SID_MODEL_8580, attack(0);
    env3_attack_1_csg8580: SID_MODEL_8580, attack(1);
    env3_attack_2_csg8580: SID_MODEL_8580, attack(2);
    env3_attack_3_csg8580: SID_MODEL_8580, attack(3);
    env3_attack_4_csg8580: SID_MODEL_8580, attack(4);
    env3_attack_5_csg8580: SID_MODEL_8580, attack(5);
    env3_attack_6_csg8580: SID_MODEL_8580, attack(6);
    env3_attack_7_csg8580: SID_MODEL_8580, attack(7);
    env3_attack_8_csg8580: SID_MODEL_8580, attack(8);
    env3_attack_9_csg8580: SID_MODEL_8580, attack(9);
    env3_attack_10_csg8580: SID_MODEL_8580, attack(10);
    env3_attack_11_csg8580: SID_MODEL_8580, attack(11);
    env3_attack_12_csg8580: SID_MODEL_8580, attack(12);
    env3_attack_13_csg8580: SID_MODEL_8580, attack(13);
    env3_attack_14_csg8580: SID_MODEL_8580, attack(14);
    env3_attack_15_csg8580: SID_MODEL_8580, attack(15);
    env3_decay_0_csg8580: SID_MODEL_8580, decay(0);
    env3_decay_1_csg8580: SID_MODEL_8580, decay(1);
    env3_decay_2_csg8580: SID_MODEL_8580, decay(2);
    env3_decay_3_csg8580: SID_MODEL_8580, decay(3);
    env3_decay_4_csg8580: SID_MODEL_8580, decay(4);
    env3_decay_5_csg8580: SID_MODEL_8580, decay(5);
    env3_decay_6_csg8580: SID_MODEL_8580, decay(6);
    env3_decay_7_csg8580: SID_MODEL_8580, decay(7);
    env3_decay_8_csg8580: SID_MODEL_8580, decay(8);
    env3_decay_9_csg8580: SID_MODEL_8580, decay(9);
    env3_decay_10_csg8580: SID_MODEL_8580, decay(10);
    env3_decay_11_csg8580: SID_MODEL_8580, decay(11);
    env3_decay_12_csg8580: SID_MODEL_8580, decay(12);
    env3_decay_13_csg8580: SID_MODEL_8580, decay(13);
    env3_decay_14_csg8580: SID_MODEL_8580, decay(14);
    env3_decay_15_csg8580: SID_MODEL_8580, decay(15);
    env3_release_0_csg8580: SID_MODEL_8580, release(0);
    env3_release_1_csg8580: SID_MODEL_8580, release(1);
    env3_release_2_csg8580: SID_MODEL_8580, release(2);
    env3_release_3_csg8580: SID_MODEL_8580, release(3);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_4_csg8580: SID_MODEL_8580, release(4);
    #[ignore = "M1b Task 2: one step apart mid-release (first at frame 4: 0xa6 vs 0xa5)"]
    env3_release_5_csg8580: SID_MODEL_8580, release(5);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_6_csg8580: SID_MODEL_8580, release(6);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_7_csg8580: SID_MODEL_8580, release(7);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_8_csg8580: SID_MODEL_8580, release(8);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_9_csg8580: SID_MODEL_8580, release(9);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_10_csg8580: SID_MODEL_8580, release(10);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_11_csg8580: SID_MODEL_8580, release(11);
    #[ignore = "M1b Task 2: power-on release one step ahead, then one step apart through the release"]
    env3_release_12_csg8580: SID_MODEL_8580, release(12);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_13_csg8580: SID_MODEL_8580, release(13);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_14_csg8580: SID_MODEL_8580, release(14);
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_release_15_csg8580: SID_MODEL_8580, release(15);
    #[ignore = "M1b Task 2: one step apart from the power-on release on and around gate changes"]
    env3_gate_toggles_per_frame_csg8580: SID_MODEL_8580, gate_toggles(CYCLES_PER_FRAME);
    #[ignore = "M1b Task 2: one step apart from the power-on release on and around gate changes"]
    env3_gate_toggles_per_100_cycles_csg8580: SID_MODEL_8580, gate_toggles(100);
    env3_adsr_delay_bug_per_frame_csg8580: SID_MODEL_8580, adsr_delay_bug(CYCLES_PER_FRAME);
    #[ignore = "M1b Task 2: power-on release one step ahead; one step apart after the delay-bug wrap"]
    env3_adsr_delay_bug_per_100_cycles_csg8580: SID_MODEL_8580, adsr_delay_bug(100);
    env3_sustain_changes_csg8580: SID_MODEL_8580, sustain_changes();
    #[ignore = "M1b Task 2: reSIDfp's power-on release runs one step ahead (rate-counter phase after reset)"]
    env3_power_on_release_csg8580: SID_MODEL_8580, power_on();
}

osc3_cases! {
    osc3_triangle_mos6581: SID_MODEL_6581, 0x10, 0x000;
    osc3_sawtooth_mos6581: SID_MODEL_6581, 0x20, 0x000;
    osc3_pulse_narrow_mos6581: SID_MODEL_6581, 0x40, 0x080;
    osc3_pulse_square_mos6581: SID_MODEL_6581, 0x40, 0x800;
    osc3_pulse_wide_mos6581: SID_MODEL_6581, 0x40, 0xf00;
    osc3_noise_mos6581: SID_MODEL_6581, 0x80, 0x000;
    osc3_triangle_csg8580: SID_MODEL_8580, 0x10, 0x000;
    osc3_sawtooth_csg8580: SID_MODEL_8580, 0x20, 0x000;
    osc3_pulse_narrow_csg8580: SID_MODEL_8580, 0x40, 0x080;
    osc3_pulse_square_csg8580: SID_MODEL_8580, 0x40, 0x800;
    osc3_pulse_wide_csg8580: SID_MODEL_8580, 0x40, 0xf00;
    osc3_noise_csg8580: SID_MODEL_8580, 0x80, 0x000;
}
