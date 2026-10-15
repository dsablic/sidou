// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the chip's character (M1c Task 3) from TRS MIDI CCs on the firmware. The
//! profile CC resets the chip into the worn 6581, the four controls change it without a
//! reset, the rendered chip in emulated RAM has them all (the spare got them before the
//! swap), and the audio matches the host build sample for sample.

mod common;

use fm1_core::Bus;

use common::{build_dir, max_diff, Firmware, Machine, HALF_FRAMES, POT, SAMPLE_RATE, SLOTS_SEED};
use sidou_sys::{ilp32, SID_CHIP_6581_WORN, SID_MODEL_6581};

/// The A4 key (Felucca's id) and its note.
const A4_KEY: u8 = 30;
const A4: u8 = 69;
const MAX_SILENT_HALVES: usize = 4;
/// How many blocks after it was sent a CC may take effect: the MIDI drain, then live_tick
/// (every 10 ms, about 3.4 blocks) applies it.
const LATE_BLOCKS: usize = 8;
/// A burst of CCs can straddle a tick: a later one takes effect in the same block as the one
/// before it or up to this many blocks after (the next tick).
const SPREAD: usize = 5;

/// docs/reference/midi.md's Character rows: profile (steps: 127 is the last, worn 6581),
/// age/heat, DAC sloppiness, output thump, filter bleed.
const CCS: [(u8, u8); 5] = [(85, 127), (86, 100), (87, 127), (89, 64), (90, 90)];

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message("the character test");
        return None;
    };
    Some(Firmware::load(&build))
}

/// Every way the burst can take effect: the first CC in `first`, each later one in the same
/// block as the one before or within SPREAD blocks of it.
fn schedules(first: usize) -> Vec<Vec<usize>> {
    let mut all: Vec<Vec<usize>> = vec![vec![first]];
    for _ in 1..CCS.len() {
        let mut next = Vec::new();
        for s in &all {
            let last = *s.last().unwrap();
            for b in last..=last + SPREAD {
                let mut t = s.clone();
                t.push(b);
                next.push(t);
            }
        }
        all = next;
    }
    all
}

/// The firmware's instrument at boot rendered by the host build (common::render_host), with
/// CC i taking effect before block blocks[i] (live_cc, then live_tick, which applies the
/// character and the profile), and the note on before `note_block`.
fn render_host(frames: usize, blocks: &[usize], note_block: usize) -> Vec<i16> {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut dac = Box::<sid_dac>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dac: &mut *dac,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    // SAFETY: everything C keeps a pointer to is boxed and outlives the calls.
    unsafe {
        sid_slots_init(&mut *slots, SAMPLE_RATE, SLOTS_SEED, &mut *pool);
        fx_live_init(&mut *fx, &mut *slots);
        live_init(
            &mut *l,
            Some(fx_live_sid_sink),
            t.cast::<std::ffi::c_void>(),
            SID_MODEL_6581,
        );
        live_set_master(&mut *l, POT);
        for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            for (k, &(cc, value)) in CCS.iter().enumerate() {
                if blocks[k] == i {
                    live_cc(&mut *l, cc, value);
                }
            }
            // Preset 0 has no LFO or vibrato: a tick writes only what MIDI asked for.
            live_tick(&mut *l);
            if i == note_block {
                live_note_on(&mut *l, A4, 100);
            }
            let n = half.len() / 2;
            sid_render(&mut *chip, mono.as_mut_ptr(), n as u32);
            sid_slots_process(&mut *slots, mono.as_ptr(), half.as_mut_ptr(), n as u32);
        }
    }
    out
}

/// The rendered chip's address in emulated RAM.
fn active_sid(m: &mut Machine, fw: &Firmware) -> u32 {
    let base = fw.chips();
    let active = m.soc.read8(base + ilp32::reg_ring_chips::active as u32);
    base + (ilp32::reg_ring_chips::chip + usize::from(active) * ilp32::sid::SIZE) as u32
}

fn read_f32(m: &mut Machine, addr: u32) -> f32 {
    f32::from_bits(m.soc.read32(addr))
}

#[test]
fn character_ccs_reach_the_chip_and_match_the_host_sample_exact() {
    let Some(fw) = fixture() else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, &fw);
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    let cc_sent = halves(&m);
    let mut burst = vec![0xB0];
    for (cc, value) in CCS {
        burst.extend([cc, value]);
    }
    m.midi(&burst);
    m.run_ms(100);
    let key_sent = halves(&m);
    m.key(A4_KEY, true);
    m.run_ms(300);

    // The chip the interrupt renders has the profile and every control.
    let base = fw.chips();
    assert_eq!(
        m.soc
            .read32(base + ilp32::reg_ring_chips::lost_resets as u32),
        0
    );
    let s = active_sid(&mut m, &fw);
    assert_eq!(
        m.soc.read8(s + ilp32::sid::model as u32),
        SID_MODEL_6581 as u8
    );
    assert_eq!(
        m.soc.read8(s + ilp32::sid::filter_curve as u32),
        3,
        "worn's filter row"
    );
    let applied = base + ilp32::reg_ring_chips::applied as u32;
    let settings: Vec<u8> = (0..5).map(|i| m.soc.read8(applied + i)).collect();
    assert_eq!(
        settings,
        [SID_CHIP_6581_WORN as u8 + 1, 100, 127, 64, 90],
        "the sink applied the settings"
    );
    let dac = m.soc.read32(s + ilp32::sid::dac as u32);
    let tables = base + ilp32::reg_ring_chips::dac as u32;
    let table = ilp32::sid_dac::SIZE as u32;
    assert!(
        dac == tables || dac == tables + table,
        "the chip's DACs are the character's tables: {dac:#x}"
    );
    // The host's chip with the same character has the same output stage.
    let mut want = Box::<sidou_sys::sid>::default();
    let mut want_dac = Box::<sidou_sys::sid_dac>::default();
    let c = sidou_sys::sid_character {
        profile: SID_CHIP_6581_WORN as u8 + 1,
        age: 100,
        dac: 127,
        thump: 64,
        bleed: 90,
    };
    unsafe {
        sidou_sys::sid_init(
            &mut *want,
            SID_MODEL_6581,
            sidou_sys::SID_CLOCK_PAL,
            SAMPLE_RATE,
        );
        sidou_sys::sid_character_apply(&mut *want, &c, &mut *want_dac);
    }
    for (name, offset, value) in [
        ("thump", ilp32::sid::thump, want.thump),
        ("bleed", ilp32::sid::bleed, want.bleed),
        ("output_gain", ilp32::sid::output_gain, want.output_gain),
        ("nl.level", ilp32::sid::nl, want.nl.level),
    ] {
        assert_eq!(read_f32(&mut m, s + offset as u32), value, "{name}");
    }

    // The audio: search the blocks the burst and the note took effect in, as fx_live.rs
    // does. The chip plays before the note (its silent voices leak), so the audio before
    // the note settles the CCs' blocks first.
    let audio = m.soc.periph.audio.clone();
    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        let before_note = (key_sent - silent) * HALF_FRAMES;
        for first in (cc_sent - silent..).take(LATE_BLOCKS) {
            for blocks in schedules(first) {
                let host = render_host(before_note, &blocks, usize::MAX);
                if max_diff(&played[..2 * before_note], &host) != 0 {
                    continue;
                }
                for note_block in (key_sent - silent..).take(LATE_BLOCKS) {
                    let host = render_host(played.len() / 2, &blocks, note_block);
                    let d = max_diff(played, &host);
                    min_diff = min_diff.min(d);
                    if d == 0 {
                        found = Some((silent, blocks, note_block));
                        break 'search;
                    }
                }
            }
        }
    }
    println!(
        "character on the device vs host: (silent halves, CC blocks, note block) {found:?} \
         (sent before blocks {cc_sent}, {key_sent}); closest max difference {min_diff} LSB \
         over {} frames",
        audio.len() / 2
    );
    assert!(
        found.is_some(),
        "the firmware's audio with the worn 6581 and its controls matches no host render: \
         closest max difference {min_diff} LSB"
    );
}
