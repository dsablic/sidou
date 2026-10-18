// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the chip's character (M1c Task 3) from TRS MIDI CCs on the firmware. The
//! profile CC resets the chip into the worn 6581, the four controls ramp it there without a
//! reset, a tick at a time, the rendered chip in emulated RAM has them all (the spare got them
//! before the swap), and the audio matches the host build sample for sample, its ticks where
//! the device's were (each tick's REG_RING_CHARACTER counted by the drain, read after every
//! block). The CHIP view (M1c Task 4) does the same from the panel: K1 on its chip turns the
//! age, end to end.

mod common;

use fm1_core::Bus;

use common::{
    build_dir, master_stage, max_diff, tail_stage, Firmware, Machine, HALF_FRAMES, POT,
    SAMPLE_RATE, SLOTS_SEED,
};
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

/// docs/reference/midi.md's Character rows: profile (steps over the five profiles and the eight
/// USER slots: 43 is 4, worn 6581), age/heat, DAC sloppiness, output thump, filter bleed.
const CCS: [(u8, u8); 5] = [(85, 43), (86, 100), (87, 127), (89, 64), (90, 90)];

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
/// CC i sent to live before block blocks[i] (live_cc), ticks[i] live_ticks before block i
/// (the device's), and the note on before `note_block`.
fn render_host(frames: usize, blocks: &[usize], note_block: usize, ticks: &[u32]) -> Vec<i16> {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut spare = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    let mut gate = tail_gate::default();
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
        // The device's preparer (platform/fm1/main.c): a profile step's reset at its tick.
        let sid_target: *mut live_sid_target = &mut (*t).sid;
        live_set_preparer(&mut *l, Some(live_sid_prepare), sid_target.cast());
        live_set_master(&mut *l, POT);
        for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            for (k, &(cc, value)) in CCS.iter().enumerate() {
                if blocks[k] == i {
                    live_cc(&mut *l, cc, value);
                }
            }
            // Preset 0 has no LFO or vibrato: a tick writes only the character's.
            for _ in 0..ticks.get(i).copied().unwrap_or(0) {
                live_tick(&mut *l);
            }
            if i == note_block {
                live_note_on(&mut *l, A4, 100);
            }
            let n = half.len() / 2;
            sid_render(&mut *chip, mono.as_mut_ptr(), n as u32);
            tail_stage(&mut gate, &chip, None, &mut mono, n);
            sid_slots_process(&mut *slots, mono.as_ptr(), half.as_mut_ptr(), n as u32);
            master_stage(half, POT);
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
fn expert_nrpn_changes_a_held_notes_sound_without_a_reset() {
    let Some(fw) = fixture() else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, &fw);
    m.key(A4_KEY, true);
    m.run_ms(400);
    let s = active_sid(&mut m, &fw);
    let pole = read_f32(&mut m, s + ilp32::sid::out_c as u32);
    let energy = |audio: &[i16]| -> f64 {
        audio[audio.len().saturating_sub(8192)..]
            .iter()
            .map(|&v| f64::from(v).powi(2))
            .sum::<f64>()
            / 8192.0
    };
    let before = energy(&m.soc.periph.audio);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    m.midi(&[
        0xb0,
        99,
        2,
        98,
        (5 + sidou_sys::SID_EXPERT_EXTERNAL_LP) as u8,
        6,
        0,
        38,
        0,
    ]);
    m.run_ms(2000);
    assert_eq!(
        active_sid(&mut m, &fw),
        s,
        "expert edits must not swap/reset a chip"
    );
    assert!(read_f32(&mut m, s + ilp32::sid::out_c as u32) < pole);
    let gate = s + ilp32::sid::env as u32 + ilp32::sid_env::gate as u32;
    assert_eq!(m.soc.read8(gate), 1, "the held gate was lost");
    let after = energy(&m.soc.periph.audio);
    println!("EXPERT external LP NRPN: held-note energy {before:.1} -> {after:.1}");
    assert!(before > 0.0 && after > 0.0);
    assert!(
        (after / before - 1.0).abs() > 0.02,
        "the NRPN did not change the sound"
    );
    assert!(m
        .passes
        .iter()
        .all(|&pass| pass < common::POLL_INSTRUCTIONS));
    m.key(A4_KEY, false);
    m.run_ms(40);
    assert_eq!(m.soc.read8(gate), 0, "the release was lost");
}

#[test]
fn expert_filter_builds_are_sliced_and_never_reset_a_held_gate() {
    let Some(fw) = fixture() else {
        return;
    };
    for model in [SID_MODEL_6581, sidou_sys::SID_MODEL_8580] {
        let mut m = Machine::boot(&fw.image);
        common::run_past_boot(&mut m, &fw);
        if model == sidou_sys::SID_MODEL_8580 {
            m.midi(&[0xb0, 25, 127]);
            m.run_ms(400);
        }
        m.key(A4_KEY, true);
        m.run_ms(400);
        let s = active_sid(&mut m, &fw);
        m.watch_calls(fw.symbol("sid_character_filter_job_step"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        let fields: &[u32] = if model == SID_MODEL_6581 {
            &[
                sidou_sys::SID_EXPERT_FILTER_6581_CURVE,
                sidou_sys::SID_EXPERT_FILTER_6581_RANGE,
            ]
        } else {
            &[sidou_sys::SID_EXPERT_FILTER_8580_CURVE]
        };
        for &field in fields {
            for raw in [0, 127] {
                m.watched_calls.clear();
                m.passes.clear();
                m.midi(&[0xb0, 99, 2, 98, (5 + field) as u8, 6, 0, 38, raw]);
                m.run_ms(2000);
                let slice = m
                    .watched_calls
                    .iter()
                    .copied()
                    .max()
                    .expect("a filter build ran");
                let pass = m.passes.iter().copied().max().expect("the main loop ran");
                println!(
                    "EXPERT model {model} field {field} raw {raw}: slice {slice}, pass {pass}"
                );
                assert!(slice < u64::from(sidou_sys::REG_RING_PREPARE_SLICE));
                assert!(pass < common::POLL_INSTRUCTIONS);
                assert_eq!(active_sid(&mut m, &fw), s, "the expert edit reset a SID");
                let gate = s + ilp32::sid::env as u32 + ilp32::sid_env::gate as u32;
                assert_eq!(m.soc.read8(gate), 1, "the held gate was lost");
            }
        }
    }
}

#[test]
fn character_ccs_reach_the_chip_and_match_the_host_sample_exact() {
    let Some(fw) = fixture() else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    // After every block, the drain's tick count (its low byte): the ticks before each block.
    let base = fw.chips();
    m.snapshot_bytes = vec![base + ilp32::reg_ring_chips::ticks as u32];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
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

    // Each block's ticks, from the counts after each block (one ALNK0 interrupt a block).
    let mut ticks = Vec::new();
    let mut count = 0u8;
    for snap in &m.buffer_snapshots {
        ticks.push(u32::from(snap.bytes[0].wrapping_sub(count)));
        count = snap.bytes[0];
    }
    assert!(
        ticks.iter().all(|&n| n <= 2),
        "more than two ticks a block: {ticks:?}"
    );
    assert_eq!(
        m.soc
            .read32(base + ilp32::reg_ring_chips::ticks as u32)
            .wrapping_sub(u32::from(count))
            % 256,
        0
    );
    // The chip the interrupt renders has the profile and every control.
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
    let now = base + (ilp32::reg_ring_chips::ramp + ilp32::sid_character_ramp::now) as u32;
    let settings: Vec<u8> = (0..5).map(|i| m.soc.read8(now + i)).collect();
    assert_eq!(
        settings,
        [SID_CHIP_6581_WORN as u8 + 1, 100, 127, 64, 90],
        "the ramp got to the settings"
    );
    let dac = m.soc.read32(s + ilp32::sid::dac as u32);
    let tables = base + ilp32::reg_ring_chips::dac as u32;
    let table = ilp32::sid_dac::SIZE as u32;
    assert!(
        (0..3).any(|i| dac == tables + i * table),
        "the chip's DACs are one of the character's tables: {dac:#x}"
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
        ..Default::default()
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
        ("filter_t", ilp32::sid::filter_t, want.filter_t),
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
                let host = render_host(before_note, &blocks, usize::MAX, &ticks);
                if max_diff(&played[..2 * before_note], &host) != 0 {
                    continue;
                }
                for note_block in (key_sent - silent..).take(LATE_BLOCKS) {
                    let host = render_host(played.len() / 2, &blocks, note_block, &ticks);
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

/// A data symbol's bytes in the firmware ELF.
fn symbol_bytes(fw: &Firmware, name: &str) -> Vec<u8> {
    use object::{Object, ObjectSection, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let sym = file
        .symbols()
        .find(|s| s.name() == Ok(name))
        .unwrap_or_else(|| panic!("no symbol {name} in the firmware ELF"));
    let (addr, size) = (sym.address(), sym.size());
    file.sections()
        .find_map(|sec| sec.data_range(addr, size).ok().flatten())
        .unwrap_or_else(|| panic!("{name}'s {size} bytes at {addr:#x} are in no section"))
        .to_vec()
}

/// The device compiles all three strengths for both models: built-in profiles and the
/// independent expert selector use them, as SID_PD's shared blocks (gen/pulldown.h).
/// Each of those profiles reads
/// back as reSIDfp's table, its blocks are the ones sidou-tablegen lays out for it, and the
/// pool holds no block they leave unused. The host keeps all six (sidou-sys).
#[test]
fn the_firmware_holds_the_combined_tables_its_profiles_and_expert_layer_use() {
    use residfp_sys::Model;
    use sidou_sys::{sid_chip_profile_get, SID_CHIPS, SID_MODEL_8580, SID_PROFILE_AVERAGE};
    use sidou_tablegen::{
        compiled_profiles, pulldown_entry, PulldownLayout, PulldownSource, PD_BLOCK_BYTES,
        PD_INDEX_BYTES, PROFILE_BYTES,
    };
    let Some(fw) = fixture() else {
        return;
    };
    for id in 0..SID_CHIPS {
        let p = unsafe { &*sid_chip_profile_get(id) };
        if u32::from(p.model) == SID_MODEL_8580 {
            assert_eq!(
                u32::from(p.combined),
                SID_PROFILE_AVERAGE,
                "the built-in R5 keeps its AVERAGE default"
            );
        }
    }
    let src = PulldownSource::new();
    let want = PulldownLayout::new(&src.compiled_rows(true, true));
    let sid_pd = symbol_bytes(&fw, "SID_PD");
    assert_eq!(
        sid_pd.len(),
        want.bytes.len(),
        "SID_PD: {} rows and {} blocks expected",
        want.rows,
        want.blocks
    );
    let block_of = |l: &[u8], r: usize, k: usize| {
        let at = r * PD_INDEX_BYTES + k * 2;
        r * PD_INDEX_BYTES + usize::from(u16::from_le_bytes([l[at], l[at + 1]]))
    };
    let (mut r, mut all_used) = (0, std::collections::HashSet::new());
    for (model, all) in [(Model::Mos6581, true), (Model::Csg8580, true)] {
        for (cws, label) in compiled_profiles(all) {
            let (mut ours, mut theirs) = (
                std::collections::HashSet::new(),
                std::collections::HashSet::new(),
            );
            for row in src.profile(model, cws) {
                for (x, &v) in row.iter().enumerate() {
                    assert_eq!(
                        pulldown_entry(&sid_pd, r, x),
                        v,
                        "{model:?} {label} row {r} entry {x}"
                    );
                }
                for k in 0..256 {
                    ours.insert(block_of(&sid_pd, r, k));
                    theirs.insert(block_of(&want.bytes, r, k));
                }
                r += 1;
            }
            println!(
                "{model:?} {label}: {} blocks, {} bytes of them (unshared, {PROFILE_BYTES})",
                ours.len(),
                ours.len() * PD_BLOCK_BYTES
            );
            assert_eq!(ours, theirs, "{model:?} {label}'s blocks");
            all_used.extend(ours);
        }
    }
    assert_eq!(r, want.rows, "SID_PD's rows");
    assert_eq!(
        all_used.len(),
        want.blocks,
        "SID_PD's pool holds blocks no profile uses"
    );
    assert_eq!(sid_pd, want.bytes);
}

// ---- the CHIP view's knobs (M1c Task 4) ----

/// app/panel.h: the buttons and encoders the CHIP view is reached and played with.
const HOME: u8 = 8;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const K1: u8 = 2;
/// K1's detents on the CHIP view's chip: the age, 4 a detent (app/panel.h
/// PANEL_CHARACTER_DETENT).
const DETENTS: i32 = 8;
const AGE: u8 = 4 * DETENTS as u8;

/// The firmware's instrument at boot rendered by the host build, as render_host, with the age
/// set to `ages[i]` before block i's ticks (`split[i]`: between its two ticks instead) and
/// the note on before `note_block`.
fn render_host_ages(
    frames: usize,
    ages: &[u8],
    split: &[bool],
    note_block: usize,
    ticks: &[u32],
) -> Vec<i16> {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut spare = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    let mut gate = tail_gate::default();
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
        // The device's preparer (platform/fm1/main.c): a profile step's reset at its tick.
        let sid_target: *mut live_sid_target = &mut (*t).sid;
        live_set_preparer(&mut *l, Some(live_sid_prepare), sid_target.cast());
        live_set_master(&mut *l, POT);
        for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            let age = u32::from(ages.get(i).copied().unwrap_or(AGE));
            let n_ticks = ticks.get(i).copied().unwrap_or(0);
            if split.get(i) == Some(&true) {
                live_tick(&mut *l);
                live_param_set(&mut *l, LIVE_PARAM_AGE, age);
                live_tick(&mut *l);
            } else {
                live_param_set(&mut *l, LIVE_PARAM_AGE, age);
                for _ in 0..n_ticks {
                    live_tick(&mut *l);
                }
            }
            if i == note_block {
                live_note_on(&mut *l, A4, 100);
            }
            let n = half.len() / 2;
            sid_render(&mut *chip, mono.as_mut_ptr(), n as u32);
            tail_stage(&mut gate, &chip, None, &mut mono, n);
            sid_slots_process(&mut *slots, mono.as_ptr(), half.as_mut_ptr(), n as u32);
            master_stage(half, POT);
        }
    }
    out
}

/// The views' instrument (platform/fm1/ui.c `ui`, its live pointer).
fn ui_live(m: &mut Machine, fw: &Firmware) -> u32 {
    m.soc.read32(fw.symbol("ui") + ilp32::views::l as u32)
}

/// The view on top (platform/fm1/ui.c `ui`, its screen's stack).
fn top_view(m: &mut Machine, fw: &Firmware) -> u32 {
    common::top_view(m, fw)
}

/// End to end, a control round trip on the panel: CHIP from the menu (THE CHIP's card skipped
/// by SELECT, which does nothing else), K1 turned on the chip's focus, and the age reaches the
/// instrument (8 detents, 4 each), the chip's ramp and the chip the interrupt renders, which
/// has what the host's chip with that character has; the audio, a note after it, matches the
/// host build sample for sample, the age changing where the device's drain took it (the
/// ramp's settings read after every block).
#[test]
fn the_chip_views_k1_turns_the_chips_age_and_matches_the_host_sample_exact() {
    let Some(fw) = fixture() else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    let base = fw.chips();
    let target_age = base
        + (ilp32::reg_ring_chips::ramp
            + ilp32::sid_character_ramp::target
            + ilp32::sid_character::age) as u32;
    m.snapshot_bytes = vec![base + ilp32::reg_ring_chips::ticks as u32, target_age];
    m.snapshot_buffer(Some((fw.symbol("abuf"), 1)));
    common::run_past_boot(&mut m, &fw);
    // HOME, then ALGORITHM on the menu's first item, CHIP; SELECT skips the card.
    m.key(HOME, true);
    m.run_ms(30);
    m.key(HOME, false);
    m.run_ms(700);
    m.turn(ALGORITHM, 1);
    m.run_ms(300);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("title_card_view"));
    m.turn(SELECT, 1);
    m.run_ms(700);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("chip_view_view"));
    let l = ui_live(&mut m, &fw);
    assert_eq!(
        m.soc.read8(l + ilp32::live::fx_select as u32),
        0,
        "SELECT on the card moved the focus"
    );
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    m.turn(K1, DETENTS);
    m.run_ms(300);
    let key_sent = halves(&m);
    m.key(A4_KEY, true);
    m.run_ms(300);

    // The instrument, the ramp and the rendered chip.
    let character: Vec<u8> = (0..4)
        .map(|i| m.soc.read8(l + (ilp32::live::character + i) as u32))
        .collect();
    assert_eq!(character, [AGE, 0, 0, 0], "live's character");
    let now = base + (ilp32::reg_ring_chips::ramp + ilp32::sid_character_ramp::now) as u32;
    let settings: Vec<u8> = (0..5).map(|i| m.soc.read8(now + i)).collect();
    assert_eq!(settings[1..], [AGE, 0, 0, 0], "the ramp got to the age");
    let s = active_sid(&mut m, &fw);
    let mut want = Box::<sidou_sys::sid>::default();
    let mut want_dac = Box::<sidou_sys::sid_dac>::default();
    let c = sidou_sys::sid_character {
        profile: settings[0],
        age: AGE,
        dac: 0,
        thump: 0,
        bleed: 0,
        ..Default::default()
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
    assert_ne!(want.filter_t, 0.0, "an age the filter does not show");
    for (name, offset, value) in [
        ("filter_t", ilp32::sid::filter_t, want.filter_t),
        ("nl.level", ilp32::sid::nl, want.nl.level),
        ("output_gain", ilp32::sid::output_gain, want.output_gain),
    ] {
        assert_eq!(read_f32(&mut m, s + offset as u32), value, "{name}");
    }

    // Each block's ticks and the age its drain had, from the snapshots after every block.
    let mut ticks = Vec::new();
    let mut ages = Vec::new();
    let mut count = 0u8;
    for snap in &m.buffer_snapshots {
        ticks.push(u32::from(snap.bytes[0].wrapping_sub(count)));
        count = snap.bytes[0];
        ages.push(snap.bytes[1]);
    }
    assert!(ticks.iter().all(|&n| n <= 2), "more than two ticks a block");
    assert_eq!(*ages.last().unwrap(), AGE);
    let steps = ages.windows(2).filter(|w| w[1] != w[0]).count();
    assert!(
        steps >= 2,
        "the age took {steps} steps: one jump is not a turn"
    );
    // A block with two ticks whose age changed: it changed at either tick.
    let ambiguous: Vec<usize> = (1..ages.len())
        .filter(|&i| ticks[i] == 2 && ages[i] != ages[i - 1])
        .collect();
    assert!(ambiguous.len() <= 6, "{} ambiguous blocks", ambiguous.len());

    let audio = m.soc.periph.audio.clone();
    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&v| v != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        let before_note = (key_sent - silent) * HALF_FRAMES;
        for variant in 0..1u32 << ambiguous.len() {
            let mut split = vec![false; ages.len()];
            for (k, &i) in ambiguous.iter().enumerate() {
                split[i] = variant >> k & 1 != 0;
            }
            let host = render_host_ages(before_note, &ages, &split, usize::MAX, &ticks);
            let d = max_diff(&played[..2 * before_note], &host);
            min_diff = min_diff.min(d);
            if d != 0 {
                continue;
            }
            for note_block in (key_sent - silent..).take(LATE_BLOCKS) {
                let host = render_host_ages(played.len() / 2, &ages, &split, note_block, &ticks);
                let d = max_diff(played, &host);
                min_diff = min_diff.min(d);
                if d == 0 {
                    found = Some((silent, variant, note_block));
                    break 'search;
                }
            }
        }
    }
    println!(
        "the CHIP view's K1 on the device vs host: (silent halves, split variant, note block) \
         {found:?}; the age in {steps} steps, {} ambiguous blocks; closest max difference \
         {min_diff} LSB over {} frames",
        ambiguous.len(),
        audio.len() / 2
    );
    assert!(
        found.is_some(),
        "the firmware's audio with the age the CHIP view's K1 set matches no host render: \
         closest max difference {min_diff} LSB"
    );
}
