// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: chip B rendered beside chip A on the firmware (M2b Task 3,
//! platform/fm1/audio.c). Until M2b Task 4 nothing on the device writes chip B or puts it in
//! use, so these tests do it as the main loop would: chip B's register writes go into the
//! register ring in emulated RAM (`ring`, entries with chip B's bit, app/reg_ring.h), and its
//! use is `b_want` (audio_chip_b_use), both between two instructions of the main loop.

mod common;

use fm1_core::Bus;

use common::{
    build_dir, max_diff, render_host_chips, ChipEvent, Firmware, Machine, AUDIO_TARGET,
    HALF_FRAMES, ISR_OVERHEAD, SID_WORST_BLOCK_TWO_CHIPS, TAP_PER_SAMPLE,
};
use sidou_sys::ilp32;

const BLOCK_FRAMES: f64 = 128.0;
/// Chip B's bit on an entry's register (app/reg_ring.h REG_RING_CHIP_B).
const B: u8 = sidou_sys::REG_RING_CHIP_B as u8;
/// The audio interrupt's mean instructions a sample with chip B out of use, a held chord on
/// LIVE (the tap on): measured on main at 2e1bc4d, before chip B could render (M1's path).
/// Chip B out of use must cost the same within 1 a sample.
const M1_CHORD_PER_SAMPLE: f64 = 397.87;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m
}

/// Runs on until the main loop is outside the audio interrupt with nothing pushed and not
/// committed, so an injection lands whole before the next block.
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

/// Pushes entries into the device's register ring and commits them, as reg_ring_sink and
/// reg_ring_commit would (settle first).
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

/// The sid chip `x` renders (platform/fm1/audio.c `chips`).
fn sid_of(m: &mut Machine, fw: &Firmware, x: usize) -> u32 {
    let base = fw.chips();
    let active = m
        .soc
        .read8(base + (ilp32::reg_ring_chips::active + x) as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

/// A function's address range in the firmware ELF.
fn function(fw: &Firmware, name: &str) -> std::ops::Range<u32> {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let s = file
        .symbols()
        .find(|s| s.name() == Ok(name))
        .unwrap_or_else(|| panic!("no symbol {name}"));
    s.address() as u32..(s.address() + s.size()) as u32
}

/// A voice's frequency, controls, envelope and the volume: writes for one chip.
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

/// Chip B is never rendered before its boot preparation has made it (its sid is zeroed until
/// then, its sample rate 0): asked into use from the first millisecond, the interrupt takes it
/// only once that sid has its rate, and then it renders.
#[test]
fn the_zeroed_chip_b_is_never_rendered() {
    let Some(fw) = fixture("the chip B boot test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    let b_on = fw.symbol("b_on");
    let mut first_on = None;
    for ms in 0..3000u64 {
        chip_b_use(&mut m, &fw, true);
        m.run_ms(1);
        if m.soc.read8(b_on) != 0 {
            let s = sid_of(&mut m, &fw, 1);
            let rate = m.soc.read32(s + ilp32::sid::sample_rate as u32);
            assert_ne!(rate, 0, "at {ms} ms chip B rendered from a zeroed sid");
            first_on.get_or_insert(ms);
        }
    }
    println!("chip B in use from {first_on:?} ms after power-on");
    assert!(first_on.is_some(), "chip B never went in use");
}

/// Chip B out of use (as on every M1 scenario): the interrupt costs what M1's did, within 1 a
/// sample, with a chord held on LIVE (the tap on).
#[test]
fn chip_b_out_of_use_costs_the_interrupt_what_m1_did() {
    let Some(fw) = fixture("the chip B cost test") else {
        return;
    };
    let mut m = booted(&fw);
    // MASTER full: its gain stage (app/master_gain.c, measured on its own below, in
    // two_chips_at_their_worst...) does not run at unity, so this measures what chip B costs.
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

/// The real interrupt with two chips: both at their worst (noise with ring modulation at
/// 0x0800 on every voice, the bench's worst two-chip scenario, noise_tri_ring_0800), chip B in
/// use, every slot's heaviest at full: the worst block within the audio target, and chip B's
/// mix (chip_mix, measured by its own instructions) under 10 a sample, so the 6502's headroom
/// (the gates' 205.5) keeps about 200.
#[test]
fn two_chips_at_their_worst_with_every_slot_heaviest_fit_the_audio_target() {
    use sidou_sys::{
        LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, SID_DIST_COUNT, SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT,
        SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS, SID_MOD_COUNT,
    };
    let Some(fw) = fixture("the two-chip budget test") else {
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
    m.run_ms(500); // the switches
    let worst: Vec<(u8, u8)> = (0..3u8).flat_map(|v| note(v, 0x0800, 0x95)).collect();
    settle(&mut m, &fw);
    let mut both = worst.clone();
    both.extend(with_b(&worst));
    push(&mut m, &fw, &both);
    chip_b_use(&mut m, &fw, true);
    m.watch_alnk_pcs(function(&fw, "chip_mix"));
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let blocks = &m.alnk_blocks[from..];
    let worst_block = blocks.iter().copied().max().unwrap() as f64 / BLOCK_FRAMES;
    let mix = m.alnk_pcs_inside as f64 / (blocks.len() as f64 * BLOCK_FRAMES);
    let headroom_gates = AUDIO_TARGET
        - SID_WORST_BLOCK_TWO_CHIPS
        - TAP_PER_SAMPLE
        - ISR_OVERHEAD
        - SLOTS_WORST_SWITCH;
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
    // MASTER's gain stage (app/master_gain.c) at the pot's power-on reading, 900: a constant
    // gain; and its ramp after the pot moved. A bound set from this measurement (new code).
    m.alnk_pcs_inside = 0;
    m.watch_alnk_pcs(function(&fw, "master_gain_apply"));
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
    let s = sid_of(&mut m, &fw, 1);
    assert_eq!(
        m.soc.read8(s + ilp32::sid::mode_vol as u32),
        0x0f,
        "chip B was not written"
    );
}

/// MASTER's gain stage, instructions a sample (a stereo frame, as every gate here counts) at a
/// constant gain (the pot at its power-on 900, so every block of the gates runs it): measured
/// 12.04 on this image, a little over 5 an audio sample (a load, a multiply, the rounding add,
/// a shift, a store) in a loop unrolled by four. It takes that much from the gates' headroom
/// for the 6502 (205.5, less the chip B mix's 8.1: about 185 are left). A ramping block
/// measures the same.
const MASTER_PER_SAMPLE: f64 = 12.5;

/// slots_budget.rs's worst switching block (docs/plans/2026-10-15-m2-overview.md: 964).
const SLOTS_WORST_SWITCH: f64 = 964.0;

/// A step of a script: the ms after the last, chip A's writes, chip B's, chip B's use.
type Step = (u64, Vec<(u8, u8)>, Vec<(u8, u8)>, Option<bool>);

/// Chip B in and out of use with both chips playing, against the host's render
/// (render_host_chips) sample for sample: chip B goes in use with only its volume written (its
/// own level, the volume step's, faded in over the block), a note on each chip, B's release,
/// and B out of use once its envelopes are at 0 (a DC-only chip faded out), then A's release.
/// Neither fade steps the output by more than B's own step, 1/128 of its sample and 1 (the
/// rounding) at any sample, the block's boundaries included (a click test, on the host's render
/// the device matches).
#[test]
fn chip_b_fades_in_plays_and_fades_out_bit_exact_with_the_host() {
    let Some(fw) = fixture("the chip B mix test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(300);
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    let script: Vec<Step> = vec![
        (0, vec![], vec![(0x18, 0x0f)], Some(true)),
        (150, note(0, 0x1cd6, 0x41), note(0, 0x1167, 0x21), None),
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
    // The halves played lag the blocks rendered: search the lag as the other host
    // comparisons do.
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
    // The fades: chip B's share of the output (the output less chip A's own) against B's own
    // samples, from the sample before the fade's block to its last and the one after.
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
    // Chip B stayed in use through its release: out of use only from the block it was asked to.
    assert!(events[3].block > events[2].block + 10);
}

/// CHIP SPREAD's stage (app/chip_spread.c: the side, the stage, which converts the slots' output
/// in their place, and the slots' dry gain), its own instructions a sample (a stereo frame) with
/// two chips at their worst, every slot's heaviest at full and SPREAD 100: measured 50.56 on
/// this image (a bound set from the measurement: new code). The interrupt's mean rises by about
/// 33 to 35: the slots' own conversion, about 17, does not run.
const SPREAD_PER_SAMPLE: f64 = 51.0;

/// The mean of the audio interrupt's blocks from `from` on, and the worst, a sample.
fn block_costs(m: &Machine, from: usize) -> (f64, f64) {
    let blocks = &m.alnk_blocks[from..];
    let mean = blocks.iter().sum::<u64>() as f64 / blocks.len() as f64 / BLOCK_FRAMES;
    let worst = blocks.iter().copied().max().unwrap() as f64 / BLOCK_FRAMES;
    (mean, worst)
}

/// The power of a Hann-windowed block at `hz` (the Goertzel recurrence).
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

/// SPREAD on the device (CC 117, SETTINGS' SPREAD; app/chip_spread.h): at 0 both sides are the
/// same (mono, as ever); at 100 a saw on chip A sounding alone stays centred, as does one on chip
/// B alone, and with both sounding A plays on the left and B on the right. Then the cost: two chips at their worst with every slot's heaviest at full,
/// SPREAD 100 against SPREAD 0: the worst block within the audio target, the stage within its
/// bound.
#[test]
fn spread_sets_chip_a_left_and_chip_b_right_within_the_audio_target() {
    use sidou_sys::{
        LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, MIDI_MAP_CC_SPREAD, SID_DIST_COUNT,
        SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT, SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS,
        SID_MOD_COUNT,
    };
    let Some(fw) = fixture("the spread test") else {
        return;
    };
    let mut m = booted(&fw);
    let spread = |m: &mut Machine, v: u8| m.midi(&[0xB0, MIDI_MAP_CC_SPREAD as u8, v]);
    m.run_ms(300);
    settle(&mut m, &fw);
    // Chip B's volume (chip A's register 24 is live's: its filter mode), and B in use.
    push(&mut m, &fw, &with_b(&[(0x18, 0x0f)]));
    chip_b_use(&mut m, &fw, true);
    m.run_ms(200);
    // Chip A as the keys play it (a MIDI note), chip B from the ring, as in the tests above.
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
        m.run_ms(600); // the release
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
    // A chip sounding alone stays centred: the stage does not run, mono's output bit for bit.
    let (al, ar) = sides(&mut m, true);
    let (bl, br) = sides(&mut m, false);
    println!("SPREAD 100 alone: chip A's saw L {al:.0} R {ar:.0}, chip B's L {bl:.0} R {br:.0}");
    assert!(al == ar && bl == br, "a chip alone is centred");
    assert!((al / mono.0 - 1.0).abs() < 0.02, "at mono's level");
    // Both sounding (chip A's note held as the keys play it, chip B's from the ring): A on the
    // left, B on the right, each at its fundamental (A3, 220 Hz; B's E4, 329.6 Hz) more than
    // 30 dB under on the other side.
    m.midi(&[0x90, 57, 100]);
    m.run_ms(100);
    settle(&mut m, &fw);
    push(&mut m, &fw, &with_b(&note(0, 0x15ed, 0x21)));
    m.run_ms(300); // the glide (93 ms) and more
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
    // The cost: both chips at their worst, every slot's heaviest at full.
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
    m.run_ms(500); // the switches
    let worst: Vec<(u8, u8)> = (0..3u8).flat_map(|v| note(v, 0x0800, 0x95)).collect();
    settle(&mut m, &fw);
    let mut both = worst.clone();
    both.extend(with_b(&worst));
    push(&mut m, &fw, &both);
    // From the push, as two_chips_at_their_worst... measures it at SPREAD 0 (2,387.4 there).
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let (_, worst_block) = block_costs(&m, from);
    // The stage's cost: the steady blocks at SPREAD 0, then at 100 again.
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
    // The stage's own instructions: the side, the stage and the glide (adjacent in SRAM, app.ld),
    // then the dry gain, each a window of its own.
    let span = |names: &[&str]| {
        let r: Vec<_> = names.iter().map(|n| function(&fw, n)).collect();
        r.iter().map(|x| x.start).min().unwrap()..r.iter().map(|x| x.end).max().unwrap()
    };
    let mut stage = 0.0;
    for range in [
        span(&["chip_spread_side", "chip_spread_apply", "chip_spread_glide_step"]),
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
