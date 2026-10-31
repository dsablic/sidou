// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

use crate::{chip, peak, ATTACK_0_FROM_RELOAD, FS, RELEASE_6_PERIOD, RELEASE_FROM_RELOAD};

const GATE: u8 = SID_CTRL_GATE as u8;
const RING: u8 = SID_CTRL_RING as u8;
const TRIANGLE: u8 = SID_CTRL_TRI as u8;
const SAW: u8 = SID_CTRL_SAW as u8;
const PULSE: u8 = SID_CTRL_PULSE as u8;
const NOISE: u8 = SID_CTRL_NOISE as u8;
const NO_WAVEFORM: u8 = 0;
const A4_AT_PAL: u16 = 0x1d45;
const FC_HI_800_HZ: u8 = 0x10;
const VOICE_1_INTO_FILTER: u8 = 0x01;
const LOW_PASS_AT_VOLUME_15: u8 = 0x1f;
const VOICE_3_FREQ: u32 = 0x1000;
const DECAY_9_PERIOD: u32 = 977;
const ENV3_ONE_CYCLE_LATE: u32 = 1;
const RISE_MAX: u32 = ATTACK_0_FROM_RELOAD + ENV3_ONE_CYCLE_LATE;
const FALL_MAX: u32 = RELEASE_6_PERIOD + RELEASE_FROM_RELOAD + ENV3_ONE_CYCLE_LATE;

fn write(s: &mut sid, reg: u8, value: u8) {
    unsafe { sid_write(s, reg, value) }
}

fn render(s: &mut sid, n: u32) -> Vec<i16> {
    let mut out = vec![0i16; n as usize];
    unsafe { sid_render(s, out.as_mut_ptr(), n) };
    out
}

fn saw_a4(s: &mut sid) {
    let [lo, hi] = A4_AT_PAL.to_le_bytes();
    write(s, 0x00, lo);
    write(s, 0x01, hi);
    write(s, 0x05, 0x00);
    write(s, 0x06, 0xf0);
    write(s, 0x18, 0x0f);
    write(s, 0x04, 0x21);
}

#[test]
fn an_8580_is_silent_after_init() {
    let mut s = chip(SID_MODEL_8580);
    assert_eq!(peak(&render(&mut s, 4410)), 0);
}

#[test]
fn gated_saw_produces_unclipped_sound() {
    let mut s = chip(SID_MODEL_8580);
    saw_a4(&mut s);
    let out = render(&mut s, 4410);
    assert!(peak(&out) > 1000);
    assert!(peak(&out) < 32767);
}

#[test]
fn volume_write_on_a_6581_clicks_then_settles() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, 0x18, 0x0f);
    let click = render(&mut s, 10);
    assert!(peak(&click) > 1000);
    render(&mut s, 44100);
    assert!(peak(&render(&mut s, 100)) < 50);
}

#[test]
fn voice_3_off_makes_a_playing_unfiltered_voice_3_sound_like_a_silent_one() {
    let setup = |play: bool, mode_vol: u8| {
        let mut s = chip(SID_MODEL_8580);
        write(&mut s, 0x0e, 0x45);
        write(&mut s, 0x0f, 0x1d);
        write(&mut s, 0x14, 0xf0);
        write(&mut s, 0x18, mode_vol);
        if play {
            write(&mut s, 0x12, 0x21);
        }
        render(&mut s, 4410)
    };
    assert_eq!(setup(true, 0x8f), setup(false, 0x8f));
    assert_ne!(setup(true, 0x0f), setup(false, 0x0f), "voice 3 on is heard");
}

#[test]
fn low_pass_filter_removes_top_end() {
    let mut a = chip(SID_MODEL_8580);
    let mut b = chip(SID_MODEL_8580);
    saw_a4(&mut a);
    saw_a4(&mut b);
    write(&mut b, 0x16, FC_HI_800_HZ);
    write(&mut b, 0x17, VOICE_1_INTO_FILTER);
    write(&mut b, 0x18, LOW_PASS_AT_VOLUME_15);
    let dry = render(&mut a, 4410);
    let wet = render(&mut b, 4410);
    let hf_energy = |v: &[i16]| -> f64 {
        v.windows(2)
            .map(|p| {
                let d = f64::from(p[1]) - f64::from(p[0]);
                d * d
            })
            .sum()
    };
    assert!(hf_energy(&wet) < hf_energy(&dry) * 0.1);
}

fn assert_dc_blocker_never_subnormal(s: &mut sid, seconds: u32) {
    let mut block = vec![0i16; 4410];
    for i in 0..seconds * 10 {
        unsafe { sid_render(s, block.as_mut_ptr(), block.len() as u32) };
        assert!(!s.dc_x1.is_subnormal(), "block {i}");
        assert!(!s.dc_y1.is_subnormal(), "block {i}");
    }
}

#[test]
fn dc_blocker_on_a_6581_volume_step_never_goes_subnormal() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, 0x18, 0x0f);
    assert_dc_blocker_never_subnormal(&mut s, 20);
}

#[test]
fn dc_blocker_after_an_8580_note_never_goes_subnormal() {
    let mut s = chip(SID_MODEL_8580);
    saw_a4(&mut s);
    render(&mut s, 4410);
    write(&mut s, 0x04, 0x20);
    assert_dc_blocker_never_subnormal(&mut s, 20);
}

#[test]
fn voice_3_off_keeps_a_filtered_voice_3() {
    let mut s = chip(SID_MODEL_8580);
    write(&mut s, 0x0e, 0x45);
    write(&mut s, 0x0f, 0x1d);
    write(&mut s, 0x14, 0xf0);
    write(&mut s, 0x16, 0xff);
    write(&mut s, 0x17, 0x04);
    write(&mut s, 0x18, 0x9f);
    write(&mut s, 0x12, 0x21);
    assert!(peak(&render(&mut s, 4410)) > 1000);
}

#[test]
fn cutoff_low_bits_come_from_register_15() {
    let mut a = chip(SID_MODEL_8580);
    let mut b = chip(SID_MODEL_8580);
    for s in [&mut a, &mut b] {
        saw_a4(s);
        write(s, 0x16, 0x02);
        write(s, 0x17, 0x01);
        write(s, 0x18, 0x1f);
    }
    write(&mut b, 0x15, 0x07);
    assert_eq!(a.fc, 0x010);
    assert_eq!(b.fc, 0x017);
    assert_ne!(render(&mut a, 4410), render(&mut b, 4410));
}

fn read(s: &mut sid, reg: u8) -> u8 {
    unsafe { sid_read(s, reg) }
}

fn write_voice_3_control_unlatched(s: &mut sid, value: u8) {
    write(s, 0x12, value);
    s.vf.osc3_held = 0;
}

fn clock(s: &mut sid, cycles: u32) {
    unsafe { sid_clock(s, cycles) }
}

#[test]
fn envelopes_and_accumulators_power_up_like_the_chip() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let s = chip(model);
        for v in 0..3 {
            assert_eq!(s.env[v].counter, 0xaa, "voice {v}");
            assert_eq!(s.env[v].counter_enabled, 1, "voice {v}");
            assert_eq!(s.env[v].state, SID_ENV_RELEASE, "voice {v}");
            assert_eq!(s.osc[v].acc, 0x555555, "voice {v}");
        }
    }
}

#[test]
fn power_on_envelopes_release_to_zero_and_freeze() {
    let mut s = chip(SID_MODEL_8580);
    clock(&mut s, 10_000);
    for v in 0..3 {
        assert_eq!(s.env[v].counter, 0, "voice {v}");
        assert_eq!(s.env[v].counter_enabled, 0, "voice {v}");
    }
}

#[test]
fn env3_reads_the_voice_3_envelope_counter_one_cycle_late() {
    let mut s = chip(SID_MODEL_8580);
    assert_eq!(read(&mut s, 0x1c), 0x00);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    clock(&mut s, 2);
    assert_eq!(
        s.env[2].counter, 0xa9,
        "the first power-on release step lands in the third cycle"
    );
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xa9);
}

#[test]
fn osc3_reads_the_top_8_bits_of_the_voice_3_waveform() {
    let mut s = chip(SID_MODEL_6581);
    write_voice_3_control_unlatched(&mut s, SAW);
    s.osc[2].acc = 0xabcdef;
    assert_eq!(read(&mut s, 0x1b), 0xab);
    write_voice_3_control_unlatched(&mut s, TRIANGLE);
    s.osc[2].acc = 0x400000;
    assert_eq!(read(&mut s, 0x1b), 0x80);
    s.osc[2].acc = 0xc00000;
    assert_eq!(read(&mut s, 0x1b), 0x7f);
    write_voice_3_control_unlatched(&mut s, NO_WAVEFORM);
    assert_eq!(read(&mut s, 0x1b), 0x00);
}

#[test]
fn osc3_follows_voice_3_ring_modulation_by_voice_2() {
    let mut s = chip(SID_MODEL_6581);
    write_voice_3_control_unlatched(&mut s, TRIANGLE | RING);
    s.osc[2].acc = 0x400000;
    s.osc[1].acc = 0x800000;
    assert_eq!(read(&mut s, 0x1b), 0x80);
    s.osc[1].acc = 0x000000;
    assert_eq!(
        read(&mut s, 0x1b),
        0x7f,
        "voice 2's MSB low inverts voice 3's"
    );
}

#[test]
fn osc3_reads_the_value_latched_before_a_write_until_the_next_cycle() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, 0x12, SAW);
    clock(&mut s, 1);
    s.osc[2].acc = 0xabc000;
    clock(&mut s, 1);
    let saw = read(&mut s, 0x1b);
    write(&mut s, 0x12, PULSE);
    assert_eq!(read(&mut s, 0x1b), saw);
    write(&mut s, 0x0e, 0x12);
    assert_eq!(
        read(&mut s, 0x1b),
        saw,
        "a further write keeps the first latch"
    );
    clock(&mut s, 1);
    assert_ne!(read(&mut s, 0x1b), saw);
}

#[test]
fn paddles_read_0xff() {
    let mut s = chip(SID_MODEL_8580);
    assert_eq!(read(&mut s, 0x19), 0xff);
    assert_eq!(read(&mut s, 0x1a), 0xff);
}

#[test]
fn write_only_registers_read_the_last_bus_value() {
    let mut s = chip(SID_MODEL_8580);
    assert_eq!(read(&mut s, 0x00), 0x00);
    write(&mut s, 0x05, 0x3c);
    assert_eq!(read(&mut s, 0x00), 0x3c);
    assert_eq!(read(&mut s, 0x18), 0x3c);
    write(&mut s, 0x18, 0x0f);
    assert_eq!(read(&mut s, 0x05), 0x0f);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    assert_eq!(read(&mut s, 0x1d), 0xaa);
    assert_eq!(read(&mut s, 0x19), 0xff);
    assert_eq!(read(&mut s, 0x1f), 0xff);
}

#[test]
fn register_numbers_wrap_at_0x20() {
    let mut s = chip(SID_MODEL_8580);
    s.env[2].env3 = 0x42;
    assert_eq!(read(&mut s, 0x3c), 0x42);
    assert_eq!(read(&mut s, 0xf9), 0xff);
}

#[test]
fn clock_advances_an_exact_number_of_cycles() {
    let mut s = chip(SID_MODEL_8580);
    write(&mut s, 0x0e, 0x00);
    write(&mut s, 0x0f, (VOICE_3_FREQ >> 8) as u8);
    clock(&mut s, 1);
    assert_eq!(s.osc[2].acc, 0x555555 + VOICE_3_FREQ);
    clock(&mut s, 99_999);
    assert_eq!(s.osc[2].acc, (0x555555 + 100_000 * VOICE_3_FREQ) & 0xffffff);
    assert_eq!(s.osc[0].acc, 0x555555);
}

#[test]
fn clock_leaves_the_chip_where_render_would() {
    let setup = |s: &mut sid| {
        write(s, 0x0e, 0x45);
        write(s, 0x0f, 0x1d);
        write(s, 0x07, 0x77);
        write(s, 0x08, 0x2a);
        write(s, 0x13, 0x26);
        write(s, 0x14, 0x84);
        write(s, 0x12, NOISE | GATE);
        write(s, 0x0b, 0x41);
    };
    let mut rendered = chip(SID_MODEL_6581);
    let mut clocked = chip(SID_MODEL_6581);
    setup(&mut rendered);
    setup(&mut clocked);
    const SAMPLES: u32 = 4410;
    render(&mut rendered, SAMPLES);
    let cycles = (u64::from(SAMPLES) * u64::from(rendered.cycles_per_sample_q16)) >> 16;
    clock(&mut clocked, u32::try_from(cycles).unwrap());
    for v in 0..3 {
        assert_eq!(clocked.osc[v].acc, rendered.osc[v].acc, "voice {v}");
        let (mut co, mut ro) = (clocked.osc[v], rendered.osc[v]);
        unsafe {
            sid_noise_settle(&mut co);
            sid_noise_settle(&mut ro);
        }
        assert_eq!(co.sr, ro.sr, "voice {v}");
        assert_eq!(clocked.env[v].counter, rendered.env[v].counter, "voice {v}");
        let (mut a, mut b) = (clocked.env[v], rendered.env[v]);
        unsafe {
            sid_env_settle(&mut a);
            sid_env_settle(&mut b);
        }
        assert_eq!(a.lfsr_pos, b.lfsr_pos, "voice {v}");
        assert_eq!(a.exp_counter, b.exp_counter, "voice {v}");
    }
    assert_eq!(read(&mut clocked, 0x1b), read(&mut rendered, 0x1b));
    assert_eq!(read(&mut clocked, 0x1c), read(&mut rendered, 0x1c));
}

#[test]
fn combined_profile_selects_tables_and_falls_back_to_average() {
    let mut s = chip(SID_MODEL_6581);
    let average = s.osc[0].pulldown;
    for (profile, differs) in [
        (SID_PROFILE_WEAK as i32, true),
        (SID_PROFILE_STRONG as i32, true),
    ] {
        unsafe { sid_set_combined_profile(&mut *s, profile) };
        assert_eq!(s.osc[0].pulldown != average, differs, "profile {profile}");
        assert_eq!(s.osc[2].pulldown, s.osc[0].pulldown);
    }
    for profile in [SID_PROFILE_AVERAGE as i32, -1, 3, i32::MAX] {
        unsafe { sid_set_combined_profile(&mut *s, profile) };
        assert_eq!(s.osc[0].pulldown, average, "profile {profile}");
    }
}

#[test]
fn osc3_on_the_8580_uses_the_latched_triangle_saw_for_one_cycle() {
    let mut s = chip(SID_MODEL_8580);
    clock(&mut s, 100);
    write(&mut s, 0x12, 0x10);
    clock(&mut s, 1);
    assert_eq!(read(&mut s, 0x1b), 0x55);
    write(&mut s, 0x0e, 0x00);
    write(&mut s, 0x0f, 0x10);
    write(&mut s, 0x12, 0x20);
    clock(&mut s, 1000);
    let saw_ramp = (s.osc[2].acc >> 12) & 0xfff;
    write(&mut s, 0x12, 0x40);
    clock(&mut s, 500);
    write(&mut s, 0x12, 0x10);
    clock(&mut s, 1);
    assert_eq!(u32::from(read(&mut s, 0x1b)), saw_ramp >> 4);
}

#[test]
fn render_folds_deferred_shift_counts_by_the_noise_period() {
    let mut s = chip(SID_MODEL_8580);
    write(&mut s, 0x00, 0xff);
    write(&mut s, 0x01, 0xff);
    write(&mut s, 0x04, SAW | GATE);
    render(&mut s, 64);
    assert_eq!(s.osc[0].defer, 1);
    s.osc[0].owed = 0x8000_0123;
    let mut unfolded = s.osc[0];
    render(&mut s, 0);
    assert!(s.osc[0].owed < 0x8000_0000);
    let mut folded = s.osc[0];
    unsafe {
        sid_noise_settle(&mut unfolded);
        sid_noise_settle(&mut folded);
    }
    assert_eq!(folded.sr, unfolded.sr);
}

fn loudest_peak_of_three_resonant_voices(model: sid_model, volume: u8) -> (i32, String) {
    let mut worst = (0, String::new());
    for (wave, control) in [("pulse", 0x41u8), ("saw", 0x21)] {
        for (lo, hi) in [(0x00u8, 0x08u8), (0x45, 0x1d)] {
            for (mode, mode_bits, routing) in [
                ("unfiltered", 0x00u8, 0x00u8),
                ("LP", 0x10, 0x07),
                ("BP", 0x20, 0x07),
                ("HP", 0x40, 0x07),
                ("LP+BP+HP", 0x70, 0x07),
            ] {
                for fc_hi in [0x00u8, 0x08, 0x20, 0x40, 0x80, 0xc0, 0xff] {
                    let mut s = chip(model);
                    for v in 0..3u8 {
                        let base = 7 * v;
                        write(&mut s, base, lo);
                        write(&mut s, base + 1, hi);
                        write(&mut s, base + 2, 0x00);
                        write(&mut s, base + 3, 0x08);
                        write(&mut s, base + 5, 0x00);
                        write(&mut s, base + 6, 0xf0);
                    }
                    write(&mut s, 0x15, 0x07);
                    write(&mut s, 0x16, fc_hi);
                    write(&mut s, 0x17, 0xf0 | routing);
                    write(&mut s, 0x18, mode_bits | volume);
                    for v in 0..3u8 {
                        write(&mut s, 7 * v + 4, control);
                    }
                    let p = peak(&render(&mut s, 22050));
                    if p > worst.0 {
                        let at = format!("{wave} at {hi:02x}{lo:02x}, {mode}, FC hi {fc_hi:#04x}");
                        worst = (p, at);
                    }
                }
            }
        }
    }
    worst
}

#[test]
fn three_resonant_voices_leave_the_6581_headroom_and_clip_the_8580_like_residfp() {
    for (name, model, max_db) in [
        ("6581", SID_MODEL_6581, -0.2),
        ("8580", SID_MODEL_8580, 1.2),
    ] {
        let (p, at) = loudest_peak_of_three_resonant_voices(model, 7);
        let peak15 = f64::from(p) * 15.0 / 7.0;
        let db = 20.0 * (peak15 / 32767.0).log10();
        println!("{name}: worst peak at volume 15 before the clamp {peak15:.0} ({at}): {db:+.2} dB re full scale");
        assert!(
            db < max_db,
            "{name}: {db:+.2} dB re full scale ({at}), over {max_db:+.1}"
        );
    }
    use sidou_tests::render::{render_residfp, RegWrite};
    let r = |reg, value| RegWrite {
        frame: 0,
        reg,
        value,
    };
    let mut writes = Vec::new();
    for v in 0..3u8 {
        let b = 7 * v;
        writes.extend([
            r(b, 0x45),
            r(b + 1, 0x1d),
            r(b + 3, 0x08),
            r(b + 5, 0x00),
            r(b + 6, 0xf0),
        ]);
    }
    writes.extend([r(0x15, 0x07), r(0x16, 0x08), r(0x17, 0xf7), r(0x18, 0x4f)]);
    writes.extend((0..3u8).map(|v| r(7 * v + 4, 0x41)));
    let reference = render_residfp(SID_MODEL_8580, &writes, 25, false);
    let peak = reference.iter().fold(0.0f32, |m, &v| m.max(v.abs())) * 32768.0;
    assert!(peak >= 32700.0, "reSIDfp's 8580 peaks at {peak:.0} here");
}

fn saw_lead_voice_3_released_to_zero(gate_reset: i32) -> Box<sid> {
    let mut s = chip(SID_MODEL_6581);
    unsafe { sid_set_gate_reset(&mut *s, gate_reset) };
    write(&mut s, 0x13, 0x09);
    write(&mut s, 0x14, 0xa6);
    write(&mut s, 0x12, 0x21);
    clock(&mut s, 100_000);
    write(&mut s, 0x12, 0x20);
    clock(&mut s, 4_000_000);
    assert_eq!(read(&mut s, 0x1c), 0);
    s
}

fn cycles_until_env3_rises(s: &mut sid, limit: u32) -> Option<u32> {
    write(s, 0x12, 0x21);
    (1..=limit).find(|_| {
        clock(s, 1);
        read(s, 0x1c) != 0
    })
}

#[test]
fn sid_init_leaves_the_gate_reset_off_on_every_voice() {
    let mut s = chip(SID_MODEL_8580);
    assert!(s.env.iter().all(|e| e.gate_reset == 0));
    unsafe { sid_set_gate_reset(&mut *s, 1) };
    assert!(s.env.iter().all(|e| e.gate_reset == 1));
    unsafe { sid_set_gate_reset(&mut *s, 0) };
    assert!(s.env.iter().all(|e| e.gate_reset == 0));
    unsafe { sid_set_gate_reset(&mut *s, 7) };
    assert!(s.env.iter().all(|e| e.gate_reset == 1), "any nonzero is on");
    unsafe { sid_init(&mut *s, SID_MODEL_8580, SID_CLOCK_PAL, FS) };
    assert!(
        s.env.iter().all(|e| e.gate_reset == 0),
        "sid_init clears it"
    );
}

#[test]
fn with_the_gate_reset_env3_rises_within_an_attack_period_at_every_phase() {
    let released = saw_lead_voice_3_released_to_zero(1);
    for phase in 0..RELEASE_6_PERIOD {
        let mut s = Box::new(*released);
        clock(&mut s, phase);
        let rise = cycles_until_env3_rises(&mut s, 100).expect("ENV3 never rose");
        assert!(rise <= RISE_MAX, "phase {phase}: {rise} cycles");
    }
}

#[test]
fn without_the_gate_reset_most_phases_wait_for_the_lfsr() {
    let released = saw_lead_voice_3_released_to_zero(0);
    let waits: Vec<u32> = (0..RELEASE_6_PERIOD)
        .map(|phase| {
            let mut s = Box::new(*released);
            clock(&mut s, phase);
            cycles_until_env3_rises(&mut s, 40_000).expect("ENV3 never rose")
        })
        .collect();
    assert_eq!(
        waits.iter().filter(|&&w| w <= RISE_MAX).count(),
        8,
        "{waits:?}"
    );
    assert!(
        waits.iter().all(|&w| w <= RISE_MAX || w > 32_000),
        "{waits:?}"
    );
}

fn saw_lead_voice_3_held_in_sustain(gate_reset: i32) -> Box<sid> {
    let mut s = chip(SID_MODEL_6581);
    unsafe { sid_set_gate_reset(&mut *s, gate_reset) };
    write(&mut s, 0x13, 0x09);
    write(&mut s, 0x14, 0xa6);
    write(&mut s, 0x12, 0x21);
    clock(&mut s, 200_000);
    assert_eq!(read(&mut s, 0x1c), 0xaa);
    s
}

fn cycles_until_env3_falls_below_0xaa(s: &mut sid, limit: u32) -> Option<u32> {
    write(s, 0x12, 0x20);
    (1..=limit).find(|_| {
        clock(s, 1);
        read(s, 0x1c) < 0xaa
    })
}

#[test]
fn with_the_gate_reset_env3_falls_within_a_release_period_at_every_phase() {
    let held = saw_lead_voice_3_held_in_sustain(1);
    for phase in 0..DECAY_9_PERIOD {
        let mut s = Box::new(*held);
        clock(&mut s, phase);
        let fall = cycles_until_env3_falls_below_0xaa(&mut s, 1000).expect("ENV3 never fell");
        assert!(fall <= FALL_MAX, "phase {phase}: {fall} cycles");
    }
}

#[test]
fn without_the_gate_reset_most_note_offs_wait_for_the_lfsr() {
    let held = saw_lead_voice_3_held_in_sustain(0);
    let waits: Vec<u32> = (0..DECAY_9_PERIOD)
        .step_by(7)
        .map(|phase| {
            let mut s = Box::new(*held);
            clock(&mut s, phase);
            cycles_until_env3_falls_below_0xaa(&mut s, 40_000).expect("ENV3 never fell")
        })
        .collect();
    assert_eq!(
        waits.iter().filter(|&&w| w <= FALL_MAX).count(),
        38,
        "{waits:?}"
    );
    assert!(
        waits.iter().all(|&w| w <= FALL_MAX || w > 32_000),
        "{waits:?}"
    );
}

unsafe extern "C" fn record_accumulators_tap(ctx: *mut std::ffi::c_void, s: *const sid) {
    let seen = unsafe { &mut *ctx.cast::<Vec<[u32; 3]>>() };
    let o = unsafe { &(*s).osc };
    seen.push([o[0].acc, o[1].acc, o[2].acc]);
}

#[test]
fn a_tapped_render_is_the_plain_render_with_the_voices_at_each_chunk() {
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let (mut a, mut b, mut c) = (chip(model), chip(model), chip(model));
        for s in [&mut a, &mut b, &mut c] {
            saw_a4(s);
            write(s, 0x17, 0xf1);
            write(s, 0x16, 0x40);
            write(s, 0x18, 0x1f);
        }
        let mut seen: Vec<[u32; 3]> = Vec::new();
        let mut expect: Vec<[u32; 3]> = Vec::new();
        for frames in [128u32, 40, 128] {
            let plain = render(&mut a, frames);
            let mut tapped = vec![0i16; frames as usize];
            unsafe {
                sid_render_tap(
                    &mut *b,
                    tapped.as_mut_ptr(),
                    frames,
                    16,
                    Some(record_accumulators_tap),
                    (&raw mut seen).cast(),
                );
            }
            assert_eq!(plain, tapped, "model {model}, {frames} frames");
            let mut k = 0;
            while k < frames {
                let n = (frames - k).min(16);
                render(&mut c, n);
                expect.push([c.osc[0].acc, c.osc[1].acc, c.osc[2].acc]);
                k += n;
            }
        }
        assert_eq!(seen.len(), 8 + 3 + 8);
        assert_eq!(seen, expect, "model {model}");
        assert!(
            peak(&render(&mut b, 128)) > 1000,
            "silent: the test would be vacuous"
        );
    }
}

#[test]
fn the_6581_non_linearity_is_a_per_chip_amount() {
    let mut s = chip(SID_MODEL_6581);
    let full = s.nl;
    assert!(full.level > 0.0 && full.load > 0.0 && full.floor > 0.0);
    unsafe { sid_set_filter_nonlinearity(&mut *s, 0.5) };
    assert_eq!(s.nl.level, full.level * 0.5);
    assert_eq!(s.nl.load, full.load * 0.5);
    assert_eq!(s.nl.floor, full.floor * 0.5);
    assert_eq!(s.nl.level_ref, full.level_ref);
    assert_eq!(s.nl.load_ref, full.load_ref);
    unsafe { sid_set_filter_nonlinearity(&mut *s, 0.0) };
    assert_eq!((s.nl.level, s.nl.load, s.nl.floor), (0.0, 0.0, 0.0));
    let other = chip(SID_MODEL_6581);
    assert_eq!(other.nl.level, full.level, "per chip");
}

#[test]
fn the_8580_ignores_the_6581_non_linearity() {
    let play = |amount: f32| {
        let mut s = chip(SID_MODEL_8580);
        unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
        saw_a4(&mut s);
        write(&mut s, 0x17, 0xf1);
        write(&mut s, 0x16, 0x10);
        write(&mut s, 0x18, 0x1f);
        render(&mut s, 8820)
    };
    assert_eq!(play(0.0), play(1.0));
    assert_eq!(play(1.0), play(3.0));
}

fn correlation_of_filtered_with_unfiltered(model: sid_model, mode: u8, fc_hi: u8) -> f64 {
    let run = |filtered: bool| {
        let mut s = chip(model);
        write(&mut s, 0x00, 0xff);
        write(&mut s, 0x01, 0xff);
        write(&mut s, 0x05, 0x00);
        write(&mut s, 0x06, 0x40);
        write(&mut s, 0x16, fc_hi);
        write(&mut s, 0x17, if filtered { 0x01 } else { 0x00 });
        write(&mut s, 0x18, mode | 0x0f);
        write(&mut s, 0x04, 0x81);
        render(&mut s, 22050)[4410..]
            .iter()
            .map(|&v| f64::from(v))
            .collect::<Vec<_>>()
    };
    let (d, f) = (run(false), run(true));
    let md = d.iter().sum::<f64>() / d.len() as f64;
    let mf = f.iter().sum::<f64>() / f.len() as f64;
    let c: f64 = d.iter().zip(&f).map(|(a, b)| (a - md) * (b - mf)).sum();
    let nd = d.iter().map(|a| (a - md).powi(2)).sum::<f64>().sqrt();
    let nf = f.iter().map(|b| (b - mf).powi(2)).sum::<f64>().sqrt();
    c / nd / nf
}

#[test]
fn the_6581_low_and_high_pass_are_inverted() {
    assert!(correlation_of_filtered_with_unfiltered(SID_MODEL_6581, 0x10, 0xe0) < -0.5);
    assert!(correlation_of_filtered_with_unfiltered(SID_MODEL_6581, 0x20, 0x30) > 0.3);
    assert!(correlation_of_filtered_with_unfiltered(SID_MODEL_6581, 0x40, 0x10) < -0.5);
    assert!(correlation_of_filtered_with_unfiltered(SID_MODEL_8580, 0x10, 0xe0) > 0.5);
    assert!(correlation_of_filtered_with_unfiltered(SID_MODEL_8580, 0x40, 0x10) > 0.5);
}

#[test]
fn the_6581_non_linearity_amount_is_clamped_to_0_to_2() {
    let nl = |amount: f32| {
        let mut s = chip(SID_MODEL_6581);
        unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
        let n = s.nl;
        (n.level, n.level_off, n.load, n.floor)
    };
    assert_eq!(nl(5.0), nl(2.0));
    assert_eq!(nl(f32::INFINITY), nl(2.0));
    assert_eq!(nl(-0.6), nl(0.0));
    assert_eq!(nl(-1.0), nl(0.0));
    assert_eq!(nl(f32::NEG_INFINITY), nl(0.0));
    assert_eq!(nl(f32::NAN), nl(0.0));
    assert_eq!(nl(0.0), (0.0, 0.0, 0.0, 0.0));
    assert_ne!(nl(2.0), nl(1.0));
    assert_eq!(nl(2.0).0, 2.0 * nl(1.0).0);
}

#[test]
fn the_6581_filter_is_stable_across_the_non_linearity_range() {
    for step in 0..=8 {
        let amount = step as f32 * 0.25;
        for fc in [0u16, 64, 300, 1024, 2047] {
            for mode in [0x1fu8, 0x2f, 0x4f] {
                let mut s = chip(SID_MODEL_6581);
                unsafe { sid_set_filter_nonlinearity(&mut *s, amount) };
                for (v, (freq, wave)) in [(0x1d45u16, 0x21u8), (0x0b00, 0x81), (0x2b00, 0x41)]
                    .into_iter()
                    .enumerate()
                {
                    let b = 7 * v as u8;
                    write(&mut s, b, (freq & 0xff) as u8);
                    write(&mut s, b + 1, (freq >> 8) as u8);
                    write(&mut s, b + 2, 0x00);
                    write(&mut s, b + 3, 0x08);
                    write(&mut s, b + 5, 0x00);
                    write(&mut s, b + 6, 0xf0);
                    write(&mut s, b + 4, wave);
                }
                write(&mut s, 0x15, (fc & 7) as u8);
                write(&mut s, 0x16, (fc >> 3) as u8);
                write(&mut s, 0x17, 0xf7);
                write(&mut s, 0x18, mode);
                let out = render(&mut s, 44_100 / 4);
                let f = s.svf;
                for x in [f.ic1, f.ic2, f.lp, f.bp, f.hp, f.bpn, s.out_lp] {
                    assert!(
                        x.is_finite() && x.abs() < 64.0,
                        "amount {amount}, FC {fc}, mode {mode:#x}: the state is {x}"
                    );
                }
                let pinned = out
                    .iter()
                    .filter(|&&x| x == i16::MAX || x == i16::MIN)
                    .count();
                assert!(
                    pinned < out.len() / 100,
                    "amount {amount}, FC {fc}, mode {mode:#x}: {pinned} samples at full scale"
                );
            }
        }
    }
}

#[test]
fn the_6581_band_pass_mix_follows_the_damping_at_every_filter_write() {
    let mut s = chip(SID_MODEL_6581);
    write(&mut s, 0x17, 0xf1);
    write(&mut s, 0x18, 0x2f);
    crate::settle_filter_coefficients(&mut s);
    assert_eq!(s.mix_bpn, 0.8125 / s.svf.k);
    for reg in [0x15u8, 0x16] {
        let new_curve_damping = 1.5 + f32::from(reg - 0x15);
        s.res_k.fill(new_curve_damping);
        write(&mut s, reg, 0x40);
        crate::settle_filter_coefficients(&mut s);
        assert_eq!(s.svf.k, 1.5 + f32::from(reg - 0x15));
        assert_eq!(s.mix_bpn, 0.8125 / s.svf.k, "after ${reg:02x}");
    }
}

#[test]
fn the_6581_lowest_cutoff_follows_the_filter_table() {
    let floor = |curve: u32, amount: f32| {
        let mut s = chip(SID_MODEL_6581);
        unsafe {
            sid_set_filter_curve(&mut *s, curve);
            sid_set_filter_nonlinearity(&mut *s, amount);
        }
        (s.nl.floor, s.cutoff_g[0])
    };
    let (f0, g0) = floor(0, 1.0);
    assert_eq!(f0, chip(SID_MODEL_6581).nl.floor, "the default, unchanged");
    assert_eq!(floor(0, 0.5).0, 0.5 * f0);
    assert_eq!(
        floor(0, 2.0).0,
        f0,
        "a stronger non-linearity keeps the chip's floor"
    );
    let last = unsafe { sid_filter_curves(SID_MODEL_6581) } - 1;
    let (f, g) = floor(last, 1.0);
    assert!(g < g0 && f < f0, "a darker curve setting: {f} against {f0}");
    assert!(
        (f / f0 - g / g0).abs() < 1e-6,
        "in proportion to the table at FC 0"
    );
}
