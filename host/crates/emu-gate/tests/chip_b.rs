// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::{
    booted, firmware, max_diff, render_host_chips, rendered_sid, ChipEvent, Firmware, Machine,
    AUDIO_TARGET, HALF_FRAMES, ISR_OVERHEAD, SID_WORST_BLOCK_TWO_CHIPS, TAP_PER_SAMPLE,
};
use sidou_sys::ilp32;

const BLOCK_FRAMES: f64 = 128.0;
const B: u8 = sidou_sys::REG_RING_CHIP_B as u8;
const M1_CHORD_PER_SAMPLE: f64 = 397.87;

fn settle(m: &mut Machine, fw: &Firmware) {
    let r = fw.symbol("ring");
    loop {
        let head = m.soc.read32(r + ilp32::reg_ring::head as u32);
        let stage = m.soc.read32(r + ilp32::reg_ring::stage as u32);
        if head == stage && m.cpu.irq_levels.is_empty() {
            return;
        }
        m.run_cycles(37);
    }
}

fn push(m: &mut Machine, fw: &Firmware, entries: &[(u8, u8)]) {
    let r = fw.symbol("ring");
    let size = sidou_sys::REG_RING_SIZE;
    let stage = m.soc.read32(r + ilp32::reg_ring::stage as u32);
    let tail = m.soc.read32(r + ilp32::reg_ring::tail as u32);
    assert!(
        stage - tail + entries.len() as u32 <= size,
        "the ring is full"
    );
    for (i, &(reg, value)) in entries.iter().enumerate() {
        let at = r
            + ilp32::reg_ring::e as u32
            + ((stage + i as u32) % size) * ilp32::reg_ring_entry::SIZE as u32;
        m.soc.write8(at + ilp32::reg_ring_entry::reg as u32, reg);
        m.soc
            .write8(at + ilp32::reg_ring_entry::value as u32, value);
    }
    let end = stage + entries.len() as u32;
    m.soc.write32(r + ilp32::reg_ring::stage as u32, end);
    m.soc.write32(r + ilp32::reg_ring::head as u32, end);
}

fn chip_b_use(m: &mut Machine, fw: &Firmware, on: bool) {
    m.soc.write8(fw.symbol("b_want"), u8::from(on));
}

fn note(voice: u8, freq: u16, control: u8) -> Vec<(u8, u8)> {
    let v = voice * 7;
    vec![
        (0x18, 0x0f),
        (v, freq as u8),
        (v + 1, (freq >> 8) as u8),
        (v + 5, 0x09),
        (v + 6, 0xa6),
        (v + 4, control),
    ]
}

fn with_b(w: &[(u8, u8)]) -> Vec<(u8, u8)> {
    w.iter().map(|&(r, v)| (r | B, v)).collect()
}

#[test]
fn the_zeroed_chip_b_is_never_rendered() {
    let Some(fw) = firmware("the chip B boot test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    let b_on = fw.symbol("b_on");
    let mut first_on = None;
    for ms in 0..3000u64 {
        chip_b_use(&mut m, &fw, true);
        m.run_ms(1);
        if m.soc.read8(b_on) != 0 {
            let s = rendered_sid(&mut m, &fw, 1);
            let rate = m.soc.read32(s + ilp32::sid::sample_rate as u32);
            assert_ne!(rate, 0, "at {ms} ms chip B rendered from a zeroed sid");
            first_on.get_or_insert(ms);
        }
    }
    println!("chip B in use from {first_on:?} ms after power-on");
    assert!(first_on.is_some(), "chip B never went in use");
}

#[test]
fn chip_b_out_of_use_costs_the_interrupt_what_m1_did() {
    let Some(fw) = firmware("the chip B cost test") else {
        return;
    };
    let mut m = booted(&fw);
    m.pot(1023);
    m.midi(&[0x90, 57, 100, 0x90, 64, 100, 0x90, 69, 100]);
    m.run_ms(300);
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let blocks = &m.alnk_blocks[from..];
    let mean = blocks.iter().sum::<u64>() as f64 / blocks.len() as f64 / BLOCK_FRAMES;
    println!("chip B out of use, a chord on LIVE: {mean:.2} instructions a sample (M1 {M1_CHORD_PER_SAMPLE:.2})");
    assert!(
        mean <= M1_CHORD_PER_SAMPLE + 1.0,
        "chip B out of use costs {mean:.2} a sample, M1 {M1_CHORD_PER_SAMPLE:.2}"
    );
}

#[test]
fn two_chips_at_their_worst_with_every_slot_heaviest_fit_the_audio_target() {
    use sidou_sys::{
        LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, SID_DIST_COUNT, SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT,
        SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS, SID_MOD_COUNT,
    };
    let Some(fw) = firmware("the two-chip budget test") else {
        return;
    };
    let mut m = booted(&fw);
    let alg = |a: u32, n: u32| ((2 * a + 1) * 128 / (2 * n)) as u8;
    for (slot, a, n) in [
        (0, SID_DIST_OVERDRIVE, SID_DIST_COUNT),
        (1, SID_MOD_CHORUS, SID_MOD_COUNT),
        (2, SID_FXSLOT_REVERB_LUSH, SID_FXSLOT_COUNT),
    ] {
        let cc = |f: u32| (LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + f) as u8;
        m.midi(&[0xB0, cc(0), alg(a, n)]);
        for f in 1..5 {
            m.midi(&[0xB0, cc(f), 127]);
        }
    }
    m.run_ms(500);
    let worst: Vec<(u8, u8)> = (0..3u8).flat_map(|v| note(v, 0x0800, 0x95)).collect();
    settle(&mut m, &fw);
    let mut both = worst.clone();
    both.extend(with_b(&worst));
    push(&mut m, &fw, &both);
    chip_b_use(&mut m, &fw, true);
    m.watch_alnk_pcs(fw.function_range("chip_mix"));
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let blocks = &m.alnk_blocks[from..];
    let worst_block = blocks.iter().copied().max().unwrap() as f64 / BLOCK_FRAMES;
    let mix = m.alnk_pcs_inside as f64 / (blocks.len() as f64 * BLOCK_FRAMES);
    let headroom_gates = AUDIO_TARGET
        - SID_WORST_BLOCK_TWO_CHIPS
        - TAP_PER_SAMPLE
        - ISR_OVERHEAD
        - common::SLOTS_SWITCH_WORST;
    let report = serde_json::json!({
        "worst_block_instructions_per_sample": worst_block,
        "chip_mix_instructions_per_sample": mix,
        "gates_headroom_for_6502": headroom_gates,
        "headroom_for_6502_with_the_mix": headroom_gates - mix,
        "audio_target": AUDIO_TARGET,
    });
    println!("{report}");
    assert!(
        worst_block <= AUDIO_TARGET,
        "two chips at their worst and every slot's heaviest: {worst_block:.1} a sample"
    );
    assert!(mix < 10.0, "chip B's mix costs {mix:.2} a sample");
    m.alnk_pcs_inside = 0;
    m.watch_alnk_pcs(fw.function_range("master_gain_apply"));
    let from = m.alnk_blocks.len();
    m.run_ms(500);
    let n = (m.alnk_blocks.len() - from) as f64 * BLOCK_FRAMES;
    let constant = m.alnk_pcs_inside as f64 / n;
    m.alnk_pcs_inside = 0;
    m.pot(300);
    let from = m.alnk_blocks.len();
    m.run_ms(500);
    let n = (m.alnk_blocks.len() - from) as f64 * BLOCK_FRAMES;
    let moved = m.alnk_pcs_inside as f64 / n;
    println!(
        "{}",
        serde_json::json!({
            "master_gain_instructions_per_sample": constant,
            "master_gain_instructions_per_sample_with_the_pot_moved_once": moved,
        })
    );
    assert!(
        constant <= MASTER_PER_SAMPLE,
        "MASTER's gain costs {constant:.2} a sample"
    );
    let s = rendered_sid(&mut m, &fw, 1);
    assert_eq!(
        m.soc.read8(s + ilp32::sid::mode_vol as u32),
        0x0f,
        "chip B was not written"
    );
}

const MASTER_PER_SAMPLE: f64 = 12.5;

type Step = (u64, Vec<(u8, u8)>, Vec<(u8, u8)>, Option<bool>);

#[test]
fn chip_b_fades_in_plays_and_fades_out_bit_exact_with_the_host() {
    let Some(fw) = firmware("the chip B mix test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(300);
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    let script: Vec<Step> = vec![
        (0, vec![], note(0, 0x1167, 0x21), Some(true)),
        (150, note(0, 0x1cd6, 0x41), vec![], None),
        (200, vec![], vec![(0x04, 0x20)], None),
        (600, vec![], vec![], Some(false)),
        (100, vec![(0x04, 0x40)], vec![], None),
    ];
    let mut at = Vec::new();
    for (ms, a, b, u) in &script {
        m.run_ms(*ms);
        settle(&mut m, &fw);
        let mut e = a.clone();
        e.extend(with_b(b));
        push(&mut m, &fw, &e);
        if let Some(u) = u {
            chip_b_use(&mut m, &fw, *u);
        }
        at.push(halves(&m));
    }
    m.run_ms(200);
    let audio = m.soc.periph.audio.clone();
    let frames = audio.len() / 2;
    let mut found = None;
    'search: for silent in 0..=4usize {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        for lag in 0..6usize {
            let events: Vec<ChipEvent> = script
                .iter()
                .zip(&at)
                .map(|((_, a, b, u), &h)| ChipEvent {
                    block: h + lag - silent,
                    a: a.clone(),
                    b: b.clone(),
                    b_use: *u,
                })
                .collect();
            let host = render_host_chips(frames - silent * HALF_FRAMES, &events);
            if max_diff(&audio[2 * silent * HALF_FRAMES..], &host.out) == 0 {
                found = Some((silent, lag, events, host));
                break 'search;
            }
        }
    }
    let Some((silent, lag, events, host)) = found else {
        panic!("the device's two chips match no host render");
    };
    println!("chip B on the device vs host: silent halves {silent}, lag {lag}: bit-exact");
    for (what, e) in [("in", &events[0]), ("out", &events[3])] {
        let from = e.block * HALF_FRAMES;
        let share = |i: usize| i32::from(host.out[2 * i]) - i32::from(host.a[i]);
        let b = |i: usize| i32::from(host.b[i]);
        let mut worst = 0;
        for i in from..=from + HALF_FRAMES {
            let step = (share(i) - share(i - 1)).abs();
            let bound = (b(i) - b(i - 1)).abs() + b(i).abs() / 128 + 1;
            worst = worst.max(step - bound);
            assert!(
                step <= bound,
                "fade {what}: sample {i} steps {step}, bound {bound}"
            );
        }
        let level = (from..from + HALF_FRAMES)
            .map(|i| b(i).abs())
            .max()
            .unwrap();
        println!(
            "fade {what}: chip B's largest sample {level}, the steps within the bound by {}",
            -worst
        );
        if what == "in" {
            assert!(
                level > 100,
                "fade in: chip B has no level to click with ({level})"
            );
        } else {
            let envs = &host.b[from..from + HALF_FRAMES];
            assert!(
                envs.iter().all(|&v| i32::from(v).abs() < 64),
                "fade out: B not DC-only"
            );
        }
    }
    assert!(events[3].block > events[2].block + 10);
}

const SPREAD_PER_SAMPLE: f64 = 51.0;

fn block_costs(m: &Machine, from: usize) -> (f64, f64) {
    let blocks = &m.alnk_blocks[from..];
    let mean = blocks.iter().sum::<u64>() as f64 / blocks.len() as f64 / BLOCK_FRAMES;
    let worst = blocks.iter().copied().max().unwrap() as f64 / BLOCK_FRAMES;
    (mean, worst)
}

fn goertzel(x: &[i16], hz: f64, fs: f64) -> f64 {
    let n = x.len() as f64;
    let c = 2.0 * (2.0 * std::f64::consts::PI * hz / fs).cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
        let s0 = f64::from(v) * w + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - c * s1 * s2
}

fn rms(x: &[i16]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

#[test]
fn spread_sets_chip_a_left_and_chip_b_right_within_the_audio_target() {
    use sidou_sys::{
        LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, MIDI_MAP_CC_SPREAD, SID_DIST_COUNT,
        SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT, SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS,
        SID_MOD_COUNT,
    };
    let Some(fw) = firmware("the spread test") else {
        return;
    };
    let mut m = booted(&fw);
    let spread = |m: &mut Machine, v: u8| m.midi(&[0xB0, MIDI_MAP_CC_SPREAD as u8, v]);
    m.run_ms(300);
    settle(&mut m, &fw);
    push(&mut m, &fw, &with_b(&[(0x18, 0x0f)]));
    chip_b_use(&mut m, &fw, true);
    m.run_ms(200);
    let sides = |m: &mut Machine, a: bool| {
        if a {
            m.midi(&[0x90, 57, 100]);
        } else {
            settle(m, &fw);
            push(m, &fw, &with_b(&note(0, 0x15ed, 0x21)));
        }
        m.run_ms(150);
        let n = m.left().len();
        let (l, r) = (
            m.left()[n - 4096..].to_vec(),
            m.right()[n - 4096..].to_vec(),
        );
        if a {
            m.midi(&[0x80, 57, 0]);
        } else {
            settle(m, &fw);
            push(m, &fw, &with_b(&[(0x04, 0x20)]));
        }
        m.run_ms(600);
        (rms(&l), rms(&r))
    };
    let mono = sides(&mut m, true);
    println!("SPREAD 0, chip A's saw: L {:.0} R {:.0}", mono.0, mono.1);
    assert!(
        mono.0 > 200.0 && mono.0 == mono.1,
        "mono: both sides the same"
    );
    spread(&mut m, 127);
    m.run_ms(50);
    let (al, ar) = sides(&mut m, true);
    let (bl, br) = sides(&mut m, false);
    println!("SPREAD 100 alone: chip A's saw L {al:.0} R {ar:.0}, chip B's L {bl:.0} R {br:.0}");
    assert!(al == ar && bl == br, "a chip alone is centred");
    assert!((al / mono.0 - 1.0).abs() < 0.02, "at mono's level");
    m.midi(&[0x90, 57, 100]);
    m.run_ms(100);
    settle(&mut m, &fw);
    push(&mut m, &fw, &with_b(&note(0, 0x15ed, 0x21)));
    m.run_ms(300);
    let n = m.left().len();
    let (l, r) = (
        m.left()[n - 4096..].to_vec(),
        m.right()[n - 4096..].to_vec(),
    );
    m.midi(&[0x80, 57, 0]);
    settle(&mut m, &fw);
    push(&mut m, &fw, &with_b(&[(0x04, 0x20)]));
    m.run_ms(600);
    let tone = |x: &[i16], hz: f64| goertzel(x, hz, f64::from(common::SAMPLE_RATE));
    let b_hz = f64::from(0x15ed) * 985_248.0 / 16_777_216.0;
    let (a_l, a_r) = (tone(&l, 220.0), tone(&r, 220.0));
    let (b_l, b_r) = (tone(&l, b_hz), tone(&r, b_hz));
    println!(
        "SPREAD 100, both: chip A's 220 Hz L {a_l:.3e} R {a_r:.3e} ({:.1} dB), chip B's {b_hz:.1} \
         Hz L {b_l:.3e} R {b_r:.3e} ({:.1} dB)",
        10.0 * (a_l / a_r).log10(),
        10.0 * (b_r / b_l).log10()
    );
    assert!(
        a_l > 1000.0 * a_r && b_r > 1000.0 * b_l,
        "both sounding: A on the left, B on the right"
    );
    let alg = |a: u32, n: u32| ((2 * a + 1) * 128 / (2 * n)) as u8;
    for (slot, a, n) in [
        (0, SID_DIST_OVERDRIVE, SID_DIST_COUNT),
        (1, SID_MOD_CHORUS, SID_MOD_COUNT),
        (2, SID_FXSLOT_REVERB_LUSH, SID_FXSLOT_COUNT),
    ] {
        let cc = |f: u32| (LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + f) as u8;
        m.midi(&[0xB0, cc(0), alg(a, n)]);
        for f in 1..5 {
            m.midi(&[0xB0, cc(f), 127]);
        }
    }
    m.run_ms(500);
    let worst: Vec<(u8, u8)> = (0..3u8).flat_map(|v| note(v, 0x0800, 0x95)).collect();
    settle(&mut m, &fw);
    let mut both = worst.clone();
    both.extend(with_b(&worst));
    push(&mut m, &fw, &both);
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let (_, worst_block) = block_costs(&m, from);
    spread(&mut m, 0);
    m.run_ms(100);
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let (mono_mean, mono_worst) = block_costs(&m, from);
    spread(&mut m, 127);
    m.run_ms(100);
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let (mean, steady_worst) = block_costs(&m, from);
    let span = |names: &[&str]| {
        let r: Vec<_> = names.iter().map(|n| fw.function_range(n)).collect();
        r.iter().map(|x| x.start).min().unwrap()..r.iter().map(|x| x.end).max().unwrap()
    };
    let mut stage = 0.0;
    for range in [
        span(&[
            "chip_spread_side",
            "chip_spread_apply",
            "chip_spread_glide_step",
        ]),
        span(&["sid_slots_dry", "slot_dry"]),
    ] {
        m.alnk_pcs_inside = 0;
        m.watch_alnk_pcs(range);
        let from = m.alnk_blocks.len();
        m.run_ms(300);
        stage += m.alnk_pcs_inside as f64 / ((m.alnk_blocks.len() - from) as f64 * BLOCK_FRAMES);
    }
    let report = serde_json::json!({
        "spread_0_steady_mean": mono_mean,
        "spread_0_steady_worst_block": mono_worst,
        "spread_100_steady_mean": mean,
        "spread_100_steady_worst_block": steady_worst,
        "spread_100_worst_block_from_the_notes": worst_block,
        "spread_100_less_spread_0_mean": mean - mono_mean,
        "spread_instructions_per_sample": stage,
        "audio_target": AUDIO_TARGET,
    });
    println!("{report}");
    assert!(
        worst_block <= AUDIO_TARGET,
        "two chips at their worst, every slot's heaviest and SPREAD 100: {worst_block:.1}"
    );
    assert!(
        stage <= SPREAD_PER_SAMPLE,
        "SPREAD's stage costs {stage:.2} a sample"
    );
}
