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
    let s = sid_of(&mut m, &fw, 1);
    assert_eq!(
        m.soc.read8(s + ilp32::sid::mode_vol as u32),
        0x0f,
        "chip B was not written"
    );
}

/// slots_budget.rs's worst switching block (docs/plans/2026-10-15-m2-overview.md: 964).
const SLOTS_WORST_SWITCH: f64 = 964.0;

/// A step of a script: the ms after the last, chip A's writes, chip B's, chip B's use.
type Step = (u64, Vec<(u8, u8)>, Vec<(u8, u8)>, Option<bool>);

/// Chip B in and out of use with both chips playing, against the host's render
/// (render_host_chips) sample for sample: chip B goes in use with only its volume written
/// (its own level, the volume step's, faded in over the block), then a note on each chip,
/// chip B out of use while its note sounds, and A's release. Neither fade steps the output by
/// more than chip B's own motion and a quarter of its level in any sample (a click test, on
/// the host's render the device matches).
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
        // Out of use while B's note sounds (its level the note's), then the release on A.
        (200, vec![], vec![], Some(false)),
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
    // The halves played lag the blocks rendered: the first block an injection reaches is a
    // few halves on; search the lag as the other host comparisons do.
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
    // The clicks: in each fade block, the mix's step against chip A's own.
    for (what, e) in [("in", &events[0]), ("out", &events[2])] {
        let from = e.block * HALF_FRAMES;
        let b = &host.b[from..from + HALF_FRAMES];
        // What switching chip B in or out at once would step the output by, at worst: its
        // largest sample in the block.
        let level = b.iter().map(|&v| i64::from(v).abs()).max().unwrap();
        let mixed: Vec<i32> = (from..from + HALF_FRAMES)
            .map(|i| i32::from(host.out[2 * i]) - i32::from(host.a[i]))
            .collect();
        let step = mixed.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap();
        // B's own motion: the fade may add at most a quarter of B's level to it.
        let own = b
            .windows(2)
            .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
            .max()
            .unwrap();
        println!(
            "fade {what}: chip B's level {level}, its own largest step {own}, the mix's {step}"
        );
        assert!(
            level > 100,
            "fade {what}: chip B has no level to click with ({level})"
        );
        assert!(
            i64::from(step - own).max(0) * 4 <= level,
            "fade {what}: a step of {step} (B's own {own}) against chip B's level {level}"
        );
    }
}
