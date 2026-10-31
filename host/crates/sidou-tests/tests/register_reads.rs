// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::{sid_model, SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::register_probe::{
    first_mismatch, probe, Residfp, Sidou, TimedWrite, CYCLES_PER_FRAME,
};

const ENV3: u8 = 0x1c;
const OSC3: u8 = 0x1b;

const VOICE2_FREQ_LO: u8 = 0x07;
const VOICE2_FREQ_HI: u8 = 0x08;
const VOICE2_PW_LO: u8 = 0x09;
const VOICE2_PW_HI: u8 = 0x0a;
const VOICE2_CONTROL: u8 = 0x0b;

const VOICE3_FREQ_LO: u8 = 0x0e;
const VOICE3_FREQ_HI: u8 = 0x0f;
const VOICE3_PW_LO: u8 = 0x10;
const VOICE3_PW_HI: u8 = 0x11;
const VOICE3_CONTROL: u8 = 0x12;
const VOICE3_AD: u8 = 0x13;
const VOICE3_SR: u8 = 0x14;

const GATE: u8 = 0x01;
const TEST: u8 = 0x08;

const RESID_RATE_PERIODS: [u64; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
];
const RATE_PERIODS_FROM_FF_DOWN_TO_0: u64 = 756;

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

fn attack(rate: u8) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, rate << 4),
            at(0, VOICE3_SR, 0xf0),
            at(1, VOICE3_CONTROL, GATE),
        ],
        frames: 1 + frames_for(RESID_RATE_PERIODS[rate as usize] * 255) + 10,
        interval: CYCLES_PER_FRAME,
    }
}

fn decay(rate: u8) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, rate),
            at(0, VOICE3_SR, 0x00),
            at(1, VOICE3_CONTROL, GATE),
        ],
        frames: 2
            + frames_for(RESID_RATE_PERIODS[rate as usize] * RATE_PERIODS_FROM_FF_DOWN_TO_0)
            + 10,
        interval: CYCLES_PER_FRAME,
    }
}

fn release(rate: u8) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, 0x00),
            at(0, VOICE3_SR, 0xf0 | rate),
            at(1, VOICE3_CONTROL, GATE),
            at(3, VOICE3_CONTROL, 0x00),
        ],
        frames: 3
            + frames_for(RESID_RATE_PERIODS[rate as usize] * RATE_PERIODS_FROM_FF_DOWN_TO_0)
            + 10,
        interval: CYCLES_PER_FRAME,
    }
}

fn gate_toggles(interval: u64) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, 0x88),
            at(0, VOICE3_SR, 0x48),
            at(1, VOICE3_CONTROL, GATE),
            at(3, VOICE3_CONTROL, 0x00),
            at(5, VOICE3_CONTROL, GATE),
            at(12, VOICE3_CONTROL, 0x00),
            at(14, VOICE3_CONTROL, GATE),
            at(30, VOICE3_CONTROL, 0x00),
        ],
        frames: 60,
        interval,
    }
}

fn adsr_delay_bug(interval: u64) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, 0xf0),
            at(0, VOICE3_SR, 0xf0),
            at(1, VOICE3_CONTROL, GATE),
            at(2, VOICE3_AD, 0x00),
        ],
        frames: 8,
        interval,
    }
}

fn sustain_lowered_then_raised_past_the_counter() -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_AD, 0x09),
            at(0, VOICE3_SR, 0x80),
            at(1, VOICE3_CONTROL, GATE),
            at(4, VOICE3_SR, 0x40),
            at(9, VOICE3_SR, 0xc0),
        ],
        frames: 150,
        interval: CYCLES_PER_FRAME,
    }
}

fn power_on_release() -> Script {
    Script {
        writes: Vec::new(),
        frames: 1,
        interval: 100,
    }
}

fn assert_reads_match(model: sid_model, reg: u8, script: &Script) {
    if let Some(m) = reads_mismatch(model, reg, script) {
        panic!("{m}");
    }
}

macro_rules! env3_cases {
    ($($(#[$attr:meta])* $name:ident: $model:expr, $script:expr;)*) => {$(
        #[test]
        $(#[$attr])*
        fn $name() {
            assert_reads_match($model, ENV3, &$script);
        }
    )*};
}

const FREQS: [u16; 4] = [0x0123, 0x1d45, 0x7531, 0xfedc];

fn single_waveform(control: u8, pw: u16, freq: u16, interval: u64) -> Script {
    Script {
        writes: vec![
            at(0, VOICE3_FREQ_LO, freq as u8),
            at(0, VOICE3_FREQ_HI, (freq >> 8) as u8),
            at(0, VOICE3_PW_LO, pw as u8),
            at(0, VOICE3_PW_HI, (pw >> 8) as u8),
            at(0, VOICE3_CONTROL, control),
        ],
        frames: if interval == CYCLES_PER_FRAME { 100 } else { 1 },
        interval,
    }
}

fn assert_osc3_matches(model: sid_model, control: u8, pw: u16, interval: u64) {
    let failures: Vec<String> = FREQS
        .iter()
        .filter_map(|&freq| {
            reads_mismatch(model, OSC3, &single_waveform(control, pw, freq, interval))
                .map(|m| format!("control {control:#04x}, pw {pw:#05x}, freq {freq:#06x}: {m}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn osc3_with_no_waveform_reads_0_where_residfp_holds_the_last_value() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let writes = [
            at(0, VOICE3_FREQ_LO, 0x45),
            at(0, VOICE3_FREQ_HI, 0x1d),
            at(0, VOICE3_CONTROL, 0x20),
            at(1, VOICE3_CONTROL, 0x00),
        ];
        let total = 2 * CYCLES_PER_FRAME;
        let ours = probe(&mut Sidou::new(model), &writes, OSC3, 100, total);
        let reference = probe(&mut Residfp::new(model), &writes, OSC3, 100, total);
        let first_read_after_the_write = (CYCLES_PER_FRAME / 100) as usize;
        assert!(
            ours[first_read_after_the_write..].iter().all(|&v| v == 0),
            "{}",
            model_name(model)
        );
        let held = reference[first_read_after_the_write];
        assert_ne!(held, 0, "{}", model_name(model));
        assert!(
            reference[first_read_after_the_write..]
                .iter()
                .all(|&v| v == held),
            "{}",
            model_name(model)
        );
    }
}

macro_rules! osc3_cases {
    ($($(#[$attr:meta])* $name:ident: $model:expr, $control:expr, $pw:expr, $interval:expr;)*) => {$(
        #[test]
        $(#[$attr])*
        fn $name() {
            assert_osc3_matches($model, $control, $pw, $interval);
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
    env3_release_4_mos6581: SID_MODEL_6581, release(4);
    env3_release_5_mos6581: SID_MODEL_6581, release(5);
    env3_release_6_mos6581: SID_MODEL_6581, release(6);
    env3_release_7_mos6581: SID_MODEL_6581, release(7);
    env3_release_8_mos6581: SID_MODEL_6581, release(8);
    env3_release_9_mos6581: SID_MODEL_6581, release(9);
    env3_release_10_mos6581: SID_MODEL_6581, release(10);
    env3_release_11_mos6581: SID_MODEL_6581, release(11);
    env3_release_12_mos6581: SID_MODEL_6581, release(12);
    env3_release_13_mos6581: SID_MODEL_6581, release(13);
    env3_release_14_mos6581: SID_MODEL_6581, release(14);
    env3_release_15_mos6581: SID_MODEL_6581, release(15);
    env3_gate_toggles_per_frame_mos6581: SID_MODEL_6581, gate_toggles(CYCLES_PER_FRAME);
    env3_gate_toggles_per_100_cycles_mos6581: SID_MODEL_6581, gate_toggles(100);
    env3_adsr_delay_bug_per_frame_mos6581: SID_MODEL_6581, adsr_delay_bug(CYCLES_PER_FRAME);
    env3_adsr_delay_bug_per_100_cycles_mos6581: SID_MODEL_6581, adsr_delay_bug(100);
    env3_sustain_changes_mos6581: SID_MODEL_6581, sustain_lowered_then_raised_past_the_counter();
    env3_power_on_release_mos6581: SID_MODEL_6581, power_on_release();
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
    env3_release_4_csg8580: SID_MODEL_8580, release(4);
    env3_release_5_csg8580: SID_MODEL_8580, release(5);
    env3_release_6_csg8580: SID_MODEL_8580, release(6);
    env3_release_7_csg8580: SID_MODEL_8580, release(7);
    env3_release_8_csg8580: SID_MODEL_8580, release(8);
    env3_release_9_csg8580: SID_MODEL_8580, release(9);
    env3_release_10_csg8580: SID_MODEL_8580, release(10);
    env3_release_11_csg8580: SID_MODEL_8580, release(11);
    env3_release_12_csg8580: SID_MODEL_8580, release(12);
    env3_release_13_csg8580: SID_MODEL_8580, release(13);
    env3_release_14_csg8580: SID_MODEL_8580, release(14);
    env3_release_15_csg8580: SID_MODEL_8580, release(15);
    env3_gate_toggles_per_frame_csg8580: SID_MODEL_8580, gate_toggles(CYCLES_PER_FRAME);
    env3_gate_toggles_per_100_cycles_csg8580: SID_MODEL_8580, gate_toggles(100);
    env3_adsr_delay_bug_per_frame_csg8580: SID_MODEL_8580, adsr_delay_bug(CYCLES_PER_FRAME);
    env3_adsr_delay_bug_per_100_cycles_csg8580: SID_MODEL_8580, adsr_delay_bug(100);
    env3_sustain_changes_csg8580: SID_MODEL_8580, sustain_lowered_then_raised_past_the_counter();
    env3_power_on_release_csg8580: SID_MODEL_8580, power_on_release();
}

osc3_cases! {
    osc3_triangle_mos6581: SID_MODEL_6581, 0x10, 0x000, CYCLES_PER_FRAME;
    osc3_sawtooth_mos6581: SID_MODEL_6581, 0x20, 0x000, CYCLES_PER_FRAME;
    osc3_pulse_narrow_mos6581: SID_MODEL_6581, 0x40, 0x080, CYCLES_PER_FRAME;
    osc3_pulse_square_mos6581: SID_MODEL_6581, 0x40, 0x800, CYCLES_PER_FRAME;
    osc3_pulse_wide_mos6581: SID_MODEL_6581, 0x40, 0xf00, CYCLES_PER_FRAME;
    osc3_noise_mos6581: SID_MODEL_6581, 0x80, 0x000, CYCLES_PER_FRAME;
    osc3_triangle_csg8580: SID_MODEL_8580, 0x10, 0x000, CYCLES_PER_FRAME;
    osc3_sawtooth_csg8580: SID_MODEL_8580, 0x20, 0x000, CYCLES_PER_FRAME;
    osc3_pulse_narrow_csg8580: SID_MODEL_8580, 0x40, 0x080, CYCLES_PER_FRAME;
    osc3_pulse_square_csg8580: SID_MODEL_8580, 0x40, 0x800, CYCLES_PER_FRAME;
    osc3_pulse_wide_csg8580: SID_MODEL_8580, 0x40, 0xf00, CYCLES_PER_FRAME;
    osc3_noise_csg8580: SID_MODEL_8580, 0x80, 0x000, CYCLES_PER_FRAME;
    osc3_sawtooth_every_7_cycles_mos6581: SID_MODEL_6581, 0x20, 0x000, 7;
    osc3_pulse_narrow_every_7_cycles_mos6581: SID_MODEL_6581, 0x40, 0x080, 7;
    osc3_pulse_square_every_7_cycles_mos6581: SID_MODEL_6581, 0x40, 0x800, 7;
    osc3_noise_every_7_cycles_mos6581: SID_MODEL_6581, 0x80, 0x000, 7;
    osc3_sawtooth_every_7_cycles_csg8580: SID_MODEL_8580, 0x20, 0x000, 7;
    osc3_pulse_narrow_every_7_cycles_csg8580: SID_MODEL_8580, 0x40, 0x080, 7;
    osc3_pulse_square_every_7_cycles_csg8580: SID_MODEL_8580, 0x40, 0x800, 7;
    osc3_noise_every_7_cycles_csg8580: SID_MODEL_8580, 0x80, 0x000, 7;
    osc3_combined_st_mos6581: SID_MODEL_6581, 0x30, 0x000, CYCLES_PER_FRAME;
    osc3_combined_st_every_7_cycles_mos6581: SID_MODEL_6581, 0x30, 0x000, 7;
    osc3_combined_pt_mos6581: SID_MODEL_6581, 0x50, 0x800, CYCLES_PER_FRAME;
    osc3_combined_pt_every_7_cycles_mos6581: SID_MODEL_6581, 0x50, 0x800, 7;
    osc3_combined_pt_narrow_mos6581: SID_MODEL_6581, 0x50, 0x100, CYCLES_PER_FRAME;
    osc3_combined_pt_narrow_every_7_cycles_mos6581: SID_MODEL_6581, 0x50, 0x100, 7;
    osc3_combined_ps_mos6581: SID_MODEL_6581, 0x60, 0x800, CYCLES_PER_FRAME;
    osc3_combined_ps_every_7_cycles_mos6581: SID_MODEL_6581, 0x60, 0x800, 7;
    osc3_combined_ps_narrow_mos6581: SID_MODEL_6581, 0x60, 0x100, CYCLES_PER_FRAME;
    osc3_combined_ps_narrow_every_7_cycles_mos6581: SID_MODEL_6581, 0x60, 0x100, 7;
    osc3_combined_pst_mos6581: SID_MODEL_6581, 0x70, 0x800, CYCLES_PER_FRAME;
    osc3_combined_pst_every_7_cycles_mos6581: SID_MODEL_6581, 0x70, 0x800, 7;
    osc3_combined_pst_narrow_mos6581: SID_MODEL_6581, 0x70, 0x100, CYCLES_PER_FRAME;
    osc3_combined_pst_narrow_every_7_cycles_mos6581: SID_MODEL_6581, 0x70, 0x100, 7;
    osc3_combined_nt_mos6581: SID_MODEL_6581, 0x90, 0x000, CYCLES_PER_FRAME;
    osc3_combined_nt_every_7_cycles_mos6581: SID_MODEL_6581, 0x90, 0x000, 7;
    osc3_combined_ns_mos6581: SID_MODEL_6581, 0xa0, 0x000, CYCLES_PER_FRAME;
    osc3_combined_ns_every_7_cycles_mos6581: SID_MODEL_6581, 0xa0, 0x000, 7;
    osc3_combined_nst_mos6581: SID_MODEL_6581, 0xb0, 0x000, CYCLES_PER_FRAME;
    osc3_combined_nst_every_7_cycles_mos6581: SID_MODEL_6581, 0xb0, 0x000, 7;
    osc3_combined_np_mos6581: SID_MODEL_6581, 0xc0, 0x800, CYCLES_PER_FRAME;
    osc3_combined_np_every_7_cycles_mos6581: SID_MODEL_6581, 0xc0, 0x800, 7;
    osc3_combined_np_narrow_mos6581: SID_MODEL_6581, 0xc0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_np_narrow_every_7_cycles_mos6581: SID_MODEL_6581, 0xc0, 0x100, 7;
    osc3_combined_npt_mos6581: SID_MODEL_6581, 0xd0, 0x800, CYCLES_PER_FRAME;
    osc3_combined_npt_every_7_cycles_mos6581: SID_MODEL_6581, 0xd0, 0x800, 7;
    osc3_combined_nps_mos6581: SID_MODEL_6581, 0xe0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_nps_every_7_cycles_mos6581: SID_MODEL_6581, 0xe0, 0x100, 7;
    osc3_combined_npst_mos6581: SID_MODEL_6581, 0xf0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_npst_every_7_cycles_mos6581: SID_MODEL_6581, 0xf0, 0x100, 7;
    osc3_combined_st_csg8580: SID_MODEL_8580, 0x30, 0x000, CYCLES_PER_FRAME;
    osc3_combined_st_every_7_cycles_csg8580: SID_MODEL_8580, 0x30, 0x000, 7;
    osc3_combined_pt_csg8580: SID_MODEL_8580, 0x50, 0x800, CYCLES_PER_FRAME;
    osc3_combined_pt_every_7_cycles_csg8580: SID_MODEL_8580, 0x50, 0x800, 7;
    osc3_combined_pt_narrow_csg8580: SID_MODEL_8580, 0x50, 0x100, CYCLES_PER_FRAME;
    osc3_combined_pt_narrow_every_7_cycles_csg8580: SID_MODEL_8580, 0x50, 0x100, 7;
    osc3_combined_ps_csg8580: SID_MODEL_8580, 0x60, 0x800, CYCLES_PER_FRAME;
    osc3_combined_ps_every_7_cycles_csg8580: SID_MODEL_8580, 0x60, 0x800, 7;
    osc3_combined_ps_narrow_csg8580: SID_MODEL_8580, 0x60, 0x100, CYCLES_PER_FRAME;
    osc3_combined_ps_narrow_every_7_cycles_csg8580: SID_MODEL_8580, 0x60, 0x100, 7;
    osc3_combined_pst_csg8580: SID_MODEL_8580, 0x70, 0x800, CYCLES_PER_FRAME;
    osc3_combined_pst_every_7_cycles_csg8580: SID_MODEL_8580, 0x70, 0x800, 7;
    osc3_combined_pst_narrow_csg8580: SID_MODEL_8580, 0x70, 0x100, CYCLES_PER_FRAME;
    osc3_combined_pst_narrow_every_7_cycles_csg8580: SID_MODEL_8580, 0x70, 0x100, 7;
    osc3_combined_nt_csg8580: SID_MODEL_8580, 0x90, 0x000, CYCLES_PER_FRAME;
    osc3_combined_nt_every_7_cycles_csg8580: SID_MODEL_8580, 0x90, 0x000, 7;
    osc3_combined_ns_csg8580: SID_MODEL_8580, 0xa0, 0x000, CYCLES_PER_FRAME;
    osc3_combined_ns_every_7_cycles_csg8580: SID_MODEL_8580, 0xa0, 0x000, 7;
    osc3_combined_nst_csg8580: SID_MODEL_8580, 0xb0, 0x000, CYCLES_PER_FRAME;
    osc3_combined_nst_every_7_cycles_csg8580: SID_MODEL_8580, 0xb0, 0x000, 7;
    osc3_combined_np_csg8580: SID_MODEL_8580, 0xc0, 0x800, CYCLES_PER_FRAME;
    osc3_combined_np_every_7_cycles_csg8580: SID_MODEL_8580, 0xc0, 0x800, 7;
    osc3_combined_np_narrow_csg8580: SID_MODEL_8580, 0xc0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_np_narrow_every_7_cycles_csg8580: SID_MODEL_8580, 0xc0, 0x100, 7;
    osc3_combined_npt_csg8580: SID_MODEL_8580, 0xd0, 0x800, CYCLES_PER_FRAME;
    osc3_combined_npt_every_7_cycles_csg8580: SID_MODEL_8580, 0xd0, 0x800, 7;
    osc3_combined_nps_csg8580: SID_MODEL_8580, 0xe0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_nps_every_7_cycles_csg8580: SID_MODEL_8580, 0xe0, 0x100, 7;
    osc3_combined_npst_csg8580: SID_MODEL_8580, 0xf0, 0x100, CYCLES_PER_FRAME;
    osc3_combined_npst_every_7_cycles_csg8580: SID_MODEL_8580, 0xf0, 0x100, 7;
}

const NOISE_SCRIPT_FREQ: u16 = 0x7531;

fn noise_script(control_writes: &[(u64, u8)], frames: u64, interval: u64) -> Script {
    let mut writes = vec![
        at(0, VOICE3_FREQ_LO, NOISE_SCRIPT_FREQ as u8),
        at(0, VOICE3_FREQ_HI, (NOISE_SCRIPT_FREQ >> 8) as u8),
        at(0, VOICE3_PW_LO, 0x00),
        at(0, VOICE3_PW_HI, 0x01),
    ];
    for &(cycle, control) in control_writes {
        writes.push(TimedWrite {
            cycle,
            reg: VOICE3_CONTROL,
            value: control,
        });
    }
    writes.sort_by_key(|w| w.cycle);
    Script {
        writes,
        frames,
        interval,
    }
}

const F: u64 = CYCLES_PER_FRAME;

macro_rules! osc3_script_cases {
    ($($name:ident: $writes:expr, $frames:expr, $interval:expr;)*) => {$(
        #[test]
        fn $name() {
            for model in [SID_MODEL_6581, SID_MODEL_8580] {
                assert_reads_match(model, OSC3, &noise_script(&$writes, $frames, $interval));
            }
        }
    )*};
}

osc3_script_cases! {
    osc3_noise_from_power_on_every_7_cycles: [(0, 0x80)], 1, 7;
    osc3_noise_through_short_test: [(0, 0x80), (2 * F, 0x88), (3 * F, 0x80)], 6, 7;
    osc3_noise_through_long_test: [(0, 0x80), (F, 0x88), (40 * F, 0x80)], 45, 101;
    osc3_noise_test_right_after_a_clock_flushes_the_pending_shift: [(0, 0x80), (42, 0x88), (60, 0x80), (78, 0x88), (100, 0x80)], 1, 1;
    osc3_noise_locks_up_combined_with_pulse: [(0, 0x80), (F, 0xc0), (5 * F, 0x80), (6 * F, 0x88), (7 * F, 0x80)], 9, 7;
    osc3_noise_locks_up_combined_with_saw: [(0, 0x80), (F, 0xa0), (5 * F, 0x80), (6 * F, 0x88), (7 * F, 0x80)], 9, 7;
    osc3_noise_locks_up_combined_with_triangle: [(0, 0x80), (F, 0x90), (3 * F, 0xf0), (5 * F, 0x80), (6 * F, 0x88), (7 * F, 0x80)], 9, 7;
    osc3_latch_from_power_on: [(100, 0x10)], 1, 1;
    osc3_latch_from_other_waveforms: [(0, 0x20), (3000, 0x40), (5000, 0x10), (9000, 0x80), (12000, 0x20), (15000, 0xc0), (17000, 0x30)], 1, 1;
    osc3_noise_after_long_saw_and_pulse: [(0, 0x80), (F, 0x20), (30 * F, 0x80), (31 * F, 0x40), (60 * F, 0x80), (61 * F, 0x10), (61 * F + 333, 0x80)], 62, 13;
    osc3_noise_back_from_triangle_every_cycle: [(0, 0x80), (100, 0x10), (5000, 0x80), (5017, 0x20), (9001, 0x80)], 1, 1;
    osc3_test_release_into_combined_waveforms: [(0, 0x80), (F, 0xb8), (2 * F, 0xa0), (3 * F, 0xd8), (4 * F, 0xe0), (5 * F, 0x98), (6 * F, 0x80), (7 * F, 0xf8), (8 * F, 0xc0)], 10, 7;
}

fn saw_switches() -> Vec<(u64, u8)> {
    let controls = [0x20u8, 0x60, 0x30, 0x20, 0x70, 0x60, 0x40, 0x30];
    (0..20u64)
        .map(|i| (i * 997, controls[i as usize % controls.len()]))
        .collect()
}

fn ring_script(control: u8) -> Script {
    let mut script = noise_script(&[(0, control)], 1, 7);
    script.writes.push(at(0, VOICE2_FREQ_LO, 0x89));
    script.writes.push(at(0, VOICE2_FREQ_HI, 0x2b));
    script.writes.sort_by_key(|w| w.cycle);
    script
}

#[test]
fn osc3_switching_saw_combinations_every_7_cycles() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        assert_reads_match(model, OSC3, &noise_script(&saw_switches(), 1, 7));
    }
}

#[test]
fn osc3_ring_modulation_every_7_cycles() {
    for control in [0x14u8, 0x54, 0x94, 0xd4] {
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            let m = reads_mismatch(model, OSC3, &ring_script(control));
            assert!(m.is_none(), "control {control:#04x}: {}", m.unwrap());
        }
    }
}

#[test]
fn osc3_relock_after_a_nonzero_output_writes_back_the_old_waveforms_output() {
    let f = CYCLES_PER_FRAME;
    let mut scripts = Vec::new();
    for (into, back) in [
        (0x20u8, 0xa0u8),
        (0x20, 0xc0),
        (0x10, 0x90),
        (0x40, 0xc0),
        (0x20, 0xe0),
    ] {
        for off in 0..40u64 {
            let w = vec![(0, 0x80), (f, 0xc0), (3 * f, into), (4 * f + off, back)];
            scripts.push((
                format!("{into:#04x}->{back:#04x} off {off}"),
                noise_script(&w, 6, 1),
            ));
        }
    }
    assert_osc3_matches_on_both_models(&scripts, 200);
}

#[test]
fn osc3_ring_from_a_6581_source_with_its_msb_pulled_sees_the_pulled_accumulator() {
    let mut bad = Vec::new();
    for c2 in [0x61u8, 0x31, 0x71, 0x21] {
        for c3 in [0x94u8, 0xd4, 0x14] {
            for interval in [1u64, 7, 22] {
                let mut w = vec![
                    at(0, VOICE2_FREQ_LO, 0x00),
                    at(0, VOICE2_FREQ_HI, 0x90),
                    at(0, VOICE2_PW_LO, 0x00),
                    at(0, VOICE2_PW_HI, 0x08),
                    at(0, VOICE2_CONTROL, c2),
                    at(0, VOICE3_CONTROL, c3),
                ];
                w.extend([
                    at(0, VOICE3_FREQ_LO, 0x31),
                    at(0, VOICE3_FREQ_HI, 0x07),
                    at(0, VOICE3_PW_LO, 0x00),
                    at(0, VOICE3_PW_HI, 0x08),
                ]);
                for f in 1..3u64 {
                    w.push(at(f, VOICE3_CONTROL, c3 | TEST));
                    w.push(TimedWrite {
                        cycle: f * CYCLES_PER_FRAME + 50,
                        reg: VOICE3_CONTROL,
                        value: c3,
                    });
                }
                w.sort_by_key(|x| x.cycle);
                let s = Script {
                    writes: w,
                    frames: 3,
                    interval,
                };
                if let Some(m) = reads_mismatch(SID_MODEL_6581, OSC3, &s) {
                    bad.push(format!(
                        "c2 {c2:#04x} c3 {c3:#04x} iv {interval}: {}",
                        &m[..m.len().min(220)]
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

fn timed_script(writes: Vec<(u64, u8, u8)>, frames: u64, interval: u64) -> Script {
    let mut w: Vec<TimedWrite> = writes
        .into_iter()
        .map(|(cycle, reg, value)| TimedWrite { cycle, reg, value })
        .collect();
    w.sort_by_key(|x| x.cycle);
    Script {
        writes: w,
        frames,
        interval,
    }
}

type LabelledScripts = Vec<(String, Script)>;

fn long_high_pitch_saws_then_noise_at_every_phase() -> LabelledScripts {
    let f = CYCLES_PER_FRAME;
    let mut scripts = Vec::new();
    for (fhi, flo) in [(0xffu8, 0xffu8), (0x75, 0x31), (0x01, 0x23)] {
        for target in [0x80u8, 0xa0, 0x88, 0xc0] {
            for off in 0..40u64 {
                let at0 = 25 * f + off * 3;
                let w = vec![
                    (0, VOICE3_FREQ_LO, flo),
                    (0, VOICE3_FREQ_HI, fhi),
                    (0, VOICE3_PW_LO, 0),
                    (0, VOICE3_PW_HI, 8),
                    (0, VOICE3_CONTROL, 0x20),
                    (at0, VOICE3_CONTROL, target),
                    (at0 + 400, VOICE3_CONTROL, 0x80),
                ];
                scripts.push((
                    format!("long saw {fhi:02x}{flo:02x} -> {target:#04x} +{off}"),
                    timed_script(w, 26, 1),
                ));
            }
        }
    }
    scripts
}

fn frequency_writes_mid_deferral_then_noise() -> LabelledScripts {
    let f = CYCLES_PER_FRAME;
    (0..30u64)
        .map(|off| {
            let w = vec![
                (0, VOICE3_FREQ_LO, 0xff),
                (0, VOICE3_FREQ_HI, 0xff),
                (0, VOICE3_CONTROL, 0x10),
                (3 * f + off, VOICE3_FREQ_HI, 0x13),
                (3 * f + off + 5, VOICE3_FREQ_LO, 0x77),
                (5 * f + off, VOICE3_FREQ_HI, 0xfe),
                (6 * f + off, VOICE3_CONTROL, 0x80),
            ];
            (format!("freq writes +{off}"), timed_script(w, 7, 1))
        })
        .collect()
}

fn long_lockups_then_test_release_or_switch_at_every_phase() -> LabelledScripts {
    let f = CYCLES_PER_FRAME;
    let mut scripts = Vec::new();
    for off in 0..40u64 {
        for (a, b) in [(0x20u8, 0xa0u8), (0x88, 0x80), (0x80, 0x80)] {
            let w = vec![
                (0, VOICE3_FREQ_LO, 0xff),
                (0, VOICE3_FREQ_HI, 0xff),
                (0, VOICE3_PW_LO, 0),
                (0, VOICE3_PW_HI, 1),
                (0, VOICE3_CONTROL, 0xc0),
                (20 * f + off, VOICE3_CONTROL, a),
                (20 * f + off + 3, VOICE3_CONTROL, b),
            ];
            scripts.push((
                format!("locked {a:#04x}->{b:#04x} +{off}"),
                timed_script(w, 21, 1),
            ));
        }
    }
    scripts
}

fn hard_syncs_while_deferring_then_noise() -> LabelledScripts {
    let f = CYCLES_PER_FRAME;
    (0..30u64)
        .map(|off| {
            let w = vec![
                (0, VOICE2_FREQ_LO, 0x00),
                (0, VOICE2_FREQ_HI, 0xf0),
                (0, VOICE2_CONTROL, 0x20),
                (0, VOICE3_FREQ_LO, 0x31),
                (0, VOICE3_FREQ_HI, 0x45),
                (0, VOICE3_CONTROL, 0x22),
                (4 * f + off * 7, VOICE3_CONTROL, 0x82),
            ];
            (format!("sync +{off}"), timed_script(w, 5, 1))
        })
        .collect()
}

fn assert_osc3_matches_on_both_models(scripts: &[(String, Script)], message_len: usize) {
    let mut bad = Vec::new();
    for (label, script) in scripts {
        for model in [SID_MODEL_6581, SID_MODEL_8580] {
            if let Some(m) = reads_mismatch(model, OSC3, script) {
                bad.push(format!("{label}: {}", &m[..m.len().min(message_len)]));
            }
        }
    }
    let n = 2 * scripts.len();
    eprintln!("{n} scripts");
    assert!(
        bad.is_empty(),
        "{} of {n} mismatch:\n{}",
        bad.len(),
        bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn deferred_noise_shifts_survive_adversarial_switches() {
    let scripts: LabelledScripts = [
        long_high_pitch_saws_then_noise_at_every_phase(),
        frequency_writes_mid_deferral_then_noise(),
        long_lockups_then_test_release_or_switch_at_every_phase(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_osc3_matches_on_both_models(&scripts, 160);
}

#[test]
#[ignore = "OSC3 on the cycles right after a hard sync: read_osc3 rebuilds the previous cycle from \
            the restarted accumulator (one read per sync on the 6581, two on the 8580)"]
fn hard_sync_while_deferring_reads_osc3_exactly() {
    assert_osc3_matches_on_both_models(&hard_syncs_while_deferring_then_noise(), 220);
}

#[test]
#[ignore = "OSC3 a cycle after a pulse width write: read_osc3 ignores pulse_held"]
fn pulse_width_write_reads_osc3_exactly() {
    let w = vec![
        (0, VOICE3_FREQ_LO, 0x00),
        (0, VOICE3_FREQ_HI, 0x10),
        (0, VOICE3_PW_LO, 0x00),
        (0, VOICE3_PW_HI, 0x00),
        (0, VOICE3_CONTROL, 0x40),
        (100, VOICE3_PW_LO, 0x00),
        (100, VOICE3_PW_HI, 0x08),
    ];
    assert_osc3_matches_on_both_models(
        &[("pw write at cycle 100".to_string(), timed_script(w, 1, 1))],
        usize::MAX,
    );
}
