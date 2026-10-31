// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::{press, A4, A4_KEY, ALGORITHM, HOME, K1, SELECT};
use common::{
    firmware, max_diff, rendered_sid, top_view, ui_live, Firmware, Machine, HALF_FRAMES,
    SAMPLE_RATE,
};
use sidou_sys::{ilp32, SID_CHIP_6581_WORN, SID_MODEL_6581};

const MAX_SILENT_HALVES: usize = 4;
const MENU_WIPE_MS: u64 = 670;
const LATE_BLOCKS: usize = 8;
const SPREAD: usize = 5;

const PROFILE_WORN_6581: u8 = 43;
const CC_PROFILE: u8 = 85;
const CC_AGE: u8 = 86;
const CC_DAC: u8 = 87;
const CC_THUMP: u8 = 89;
const CC_BLEED: u8 = 90;
const CCS: [(u8, u8); 5] = [
    (CC_PROFILE, PROFILE_WORN_6581),
    (CC_AGE, 100),
    (CC_DAC, 127),
    (CC_THUMP, 64),
    (CC_BLEED, 90),
];

const THUMP_PER_SAMPLE: f64 = 31.0;

const GRIT_PER_SAMPLE: f64 = 19.2;

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

fn render_host(frames: usize, blocks: &[usize], note_block: usize, ticks: &[u32]) -> Vec<i16> {
    use sidou_sys::*;
    common::render_instrument(frames, |i, l| unsafe {
        for (k, &(cc, value)) in CCS.iter().enumerate() {
            if blocks[k] == i {
                live_cc(l, cc, value);
            }
        }
        for _ in 0..ticks.get(i).copied().unwrap_or(0) {
            live_tick(l);
        }
        if i == note_block {
            live_note_on(l, A4, 100);
        }
    })
}

fn read_f32(m: &mut Machine, addr: u32) -> f32 {
    f32::from_bits(m.soc.read32(addr))
}

#[test]
fn expert_nrpn_changes_a_held_notes_sound_without_a_reset() {
    let Some(fw) = firmware("the character test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, &fw);
    m.key(A4_KEY, true);
    m.run_ms(400);
    let s = rendered_sid(&mut m, &fw, 0);
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
        rendered_sid(&mut m, &fw, 0),
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
    let Some(fw) = firmware("the character test") else {
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
        let s = rendered_sid(&mut m, &fw, 0);
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
                assert_eq!(
                    rendered_sid(&mut m, &fw, 0),
                    s,
                    "the expert edit reset a SID"
                );
                let gate = s + ilp32::sid::env as u32 + ilp32::sid_env::gate as u32;
                assert_eq!(m.soc.read8(gate), 1, "the held gate was lost");
            }
        }
    }
}

#[test]
fn character_ccs_reach_the_chip_and_match_the_host_sample_exact() {
    let Some(fw) = firmware("the character test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
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
    m.alnk_pcs_inside = 0;
    let parts: Vec<_> = [
        "thump_out_take",
        "thump_out_drop",
        "thump_run",
        "thump_out_apply",
    ]
    .iter()
    .map(|f| fw.function_range(f))
    .collect();
    let stage =
        parts.iter().map(|r| r.start).min().unwrap()..parts.iter().map(|r| r.end).max().unwrap();
    let size: u32 = parts.iter().map(|r| r.end - r.start).sum();
    assert!(
        stage.end - stage.start - size < 16,
        "something between the stage's functions"
    );
    m.watch_alnk_pcs(stage);
    let from = m.alnk_blocks.len();
    m.key(A4_KEY, true);
    m.run_ms(100);
    let n = (m.alnk_blocks.len() - from) as f64 * HALF_FRAMES as f64;
    let thump = m.alnk_pcs_inside as f64 / n;
    println!(
        "{}",
        serde_json::json!({ "thump_out_instructions_per_sample": thump })
    );
    assert!(
        thump <= THUMP_PER_SAMPLE,
        "the thump's stage costs {thump:.2} a sample"
    );
    m.alnk_pcs_inside = 0;
    m.watch_alnk_pcs(fw.function_range("grit"));
    let from = m.alnk_blocks.len();
    m.run_ms(100);
    let n = (m.alnk_blocks.len() - from) as f64 * HALF_FRAMES as f64;
    let grit = m.alnk_pcs_inside as f64 / n;
    println!(
        "{}",
        serde_json::json!({ "grit_instructions_per_sample": grit })
    );
    assert!(
        grit > 1.0 && grit <= GRIT_PER_SAMPLE,
        "DAC's grit costs {grit:.2} a sample"
    );
    m.run_ms(100);

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
    assert_eq!(
        m.soc
            .read32(base + ilp32::reg_ring_chips::lost_resets as u32),
        0
    );
    let s = rendered_sid(&mut m, &fw, 0);
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

#[test]
fn the_firmware_holds_the_combined_tables_its_profiles_and_expert_layer_use() {
    use residfp_sys::Model;
    use sidou_sys::{sid_chip_profile_get, SID_CHIPS, SID_MODEL_8580, SID_PROFILE_AVERAGE};
    use sidou_tablegen::{
        compiled_profiles, pulldown_entry, PulldownLayout, PulldownSource, PD_BLOCK_BYTES,
        PD_INDEX_BYTES, PROFILE_BYTES,
    };
    let Some(fw) = firmware("the character test") else {
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
    let mut sid_pd = symbol_bytes(&fw, "SID_PD");
    assert_eq!(
        sid_pd.len(),
        want.bytes.len().next_multiple_of(2),
        "SID_PD: {} index rows and {} blocks expected",
        want.rows,
        want.blocks
    );
    sid_pd.truncate(want.bytes.len());
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
                        pulldown_entry(&sid_pd, want.row_map[r], x),
                        v,
                        "{model:?} {label} row {r} entry {x}"
                    );
                }
                for k in 0..256 {
                    ours.insert(block_of(&sid_pd, want.row_map[r], k));
                    theirs.insert(block_of(&want.bytes, want.row_map[r], k));
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
    assert_eq!(r, want.row_map.len(), "SID_PD's rows");
    assert_eq!(
        all_used.len(),
        want.blocks,
        "SID_PD's pool holds blocks no profile uses"
    );
    assert_eq!(sid_pd, want.bytes);
    let base = fw.symbol("SID_PD");
    let rows: Vec<u32> = symbol_bytes(&fw, "SID_PD_ROWS")
        .chunks(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let want_rows: Vec<u32> = want
        .row_map
        .iter()
        .map(|&r| base + (r * PD_INDEX_BYTES) as u32)
        .collect();
    assert_eq!(rows, want_rows, "SID_PD_ROWS");
}

const DETENTS: i32 = 8;
const AGE: u8 = sidou_sys::PANEL_CHARACTER_DETENT as u8 * DETENTS as u8;

fn render_host_ages(
    frames: usize,
    ages: &[u8],
    split: &[bool],
    note_block: usize,
    ticks: &[u32],
) -> Vec<i16> {
    use sidou_sys::*;
    common::render_instrument(frames, |i, l| unsafe {
        let age = u32::from(ages.get(i).copied().unwrap_or(AGE));
        let n_ticks = ticks.get(i).copied().unwrap_or(0);
        if split.get(i) == Some(&true) {
            live_tick(l);
            live_param_set(l, LIVE_PARAM_AGE, age);
            live_tick(l);
        } else {
            live_param_set(l, LIVE_PARAM_AGE, age);
            for _ in 0..n_ticks {
                live_tick(l);
            }
        }
        if i == note_block {
            live_note_on(l, A4, 100);
        }
    })
}

#[test]
fn the_chip_views_k1_turns_the_chips_age_and_matches_the_host_sample_exact() {
    let Some(fw) = firmware("the character test") else {
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
    press(&mut m, HOME);
    m.run_ms(MENU_WIPE_MS);
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

    let character: Vec<u8> = (0..4)
        .map(|i| m.soc.read8(l + (ilp32::live::character + i) as u32))
        .collect();
    assert_eq!(character, [AGE, 0, 0, 0], "live's character");
    let now = base + (ilp32::reg_ring_chips::ramp + ilp32::sid_character_ramp::now) as u32;
    let settings: Vec<u8> = (0..5).map(|i| m.soc.read8(now + i)).collect();
    assert_eq!(settings[1..], [AGE, 0, 0, 0], "the ramp got to the age");
    let s = rendered_sid(&mut m, &fw, 0);
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
