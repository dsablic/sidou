// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use fm1_core::Bus;

use common::panel::{
    holding, note_key, press, A4, A4_KEY, ALGORITHM, ARP, EDIT, K1, K4, OCT_UP, SELECT,
};
use common::{
    booted, firmware, max_diff, peak, power_above, render_host, Machine, AUDIO_TARGET, HALF_FRAMES,
};
use sidou_sys::{
    LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, SID_DIST_COUNT, SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT,
    SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS, SID_MOD_COUNT,
};

const AMOUNT: u8 = K1;
const TONE: u8 = K1 + 1;
const MIX: u8 = K4;
const FULL_TRAVEL: i32 = sidou_sys::LIVE_SLOT_DETENTS as i32;
const KNOB_SETTLE_MS: u64 = 400;
const SWITCH_AND_SMOOTHING_MS: u64 = 200;
const DISTORTION_SLOT: u32 = 0;
const FX_SLOT: u32 = 2;
const ALGORITHM_FIELD: u32 = sidou_sys::LIVE_SLOT_ALGORITHM;
const AMOUNT_FIELD: u32 = sidou_sys::LIVE_SLOT_AMOUNT;
const MIX_FIELD: u32 = sidou_sys::LIVE_SLOT_MIX;
const CHORD_KEYS: [u8; 3] = [A4_KEY, note_key(73), note_key(76)];
const BLOCK_FRAMES: f64 = 128.0;
const MAX_SILENT_HALVES: usize = 4;
const LATE_BLOCKS: usize = 8;
const CC_SPREAD: usize = 3;

fn cc_schedules(first: usize, ccs: &[(u8, u8)]) -> Vec<Vec<(usize, u8, u8)>> {
    let mut all: Vec<Vec<(usize, u8, u8)>> = vec![Vec::new()];
    for (i, &(cc, value)) in ccs.iter().enumerate() {
        let mut next = Vec::new();
        for s in &all {
            let from = s.last().map_or(first, |c| c.0);
            let to = if i == 0 { first } else { first + CC_SPREAD };
            for b in from..=to {
                let mut t = s.clone();
                t.push((b, cc, value));
                next.push(t);
            }
        }
        all = next;
    }
    all
}

fn record_lr(m: &mut Machine, ms: u64) -> (Vec<f32>, Vec<f32>) {
    let from = m.soc.periph.audio.len() / 2;
    m.run_ms(ms);
    let f = |v: Vec<i16>| v[from..].iter().map(|&s| f32::from(s)).collect();
    (f(m.left()), f(m.right()))
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa * bb).sqrt()
}

fn algorithm_cc(alg: u32, count: u32) -> u8 {
    ((2 * alg + 1) * 128 / (2 * count)) as u8
}

fn slot_cc(m: &mut Machine, slot: u32, field: u32, value: u8) {
    m.midi(&[
        0xB0,
        (LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + field) as u8,
        value,
    ]);
}

#[test]
fn the_edit_layer_turns_on_overdrive_and_the_wave_clips() {
    let Some(fw) = firmware("the effects test") else {
        return;
    };
    let mut m = booted(&fw);
    for k in CHORD_KEYS {
        m.key(k, true);
    }
    m.run_ms(200);
    m.soc.periph.led_take();
    let (dry, dry_r) = record_lr(&mut m, 300);
    let edit_before = m.soc.periph.led_take()[usize::from(EDIT)];
    slot_algorithm(&mut m, 2);
    holding(&mut m, EDIT, |m| {
        for (knob, travel) in [
            (AMOUNT, -FULL_TRAVEL),
            (TONE, FULL_TRAVEL),
            (MIX, FULL_TRAVEL),
        ] {
            m.turn(knob, travel);
            m.run_ms(KNOB_SETTLE_MS);
        }
    });
    m.run_ms(SWITCH_AND_SMOOTHING_MS);
    let (undriven, _) = record_lr(&mut m, 300);
    holding(&mut m, EDIT, |m| {
        m.turn(AMOUNT, FULL_TRAVEL);
        m.run_ms(KNOB_SETTLE_MS);
    });
    m.run_ms(SWITCH_AND_SMOOTHING_MS);
    let (wet, wet_r) = record_lr(&mut m, 300);
    let crest = |x: &[f32]| f64::from(peak(x)) / rms(x);
    let (cd, c0, cw) = (crest(&dry), crest(&undriven), crest(&wet));
    println!(
        "overdrive from the panel: crest factor {cd:.2} dry, {c0:.2} at no drive -> {cw:.2}, \
         rms {:.0} -> {:.0}, power above 2 kHz {:.3} -> {:.3}",
        rms(&dry),
        rms(&wet),
        power_above(&dry, 2000.0),
        power_above(&wet, 2000.0)
    );
    assert_eq!(dry, dry_r, "dry is mono");
    assert_eq!(wet, wet_r, "the distortion slot is mono");
    assert!(cw < 0.8 * c0, "no clipping: crest factor {c0} -> {cw}");
    assert!(
        power_above(&wet, 2000.0) > 2.0 * power_above(&dry, 2000.0),
        "the drive's harmonics did not come up"
    );
    assert!(
        (rms(&wet) / rms(&dry)).log10().abs() * 20.0 < 6.0,
        "the drive changed the level more than the sound"
    );
    m.soc.periph.led_take();
    m.run_ms(50);
    let lit = m.soc.periph.led_take()[usize::from(EDIT)];
    assert_eq!(edit_before, 0.0, "EDIT lit while every slot was off");
    assert!(lit > 0.0, "EDIT's LED is dark with the slot on");
}

#[test]
fn trs_ccs_turn_on_the_reverb_in_stereo_with_a_tail() {
    let Some(fw) = firmware("the effects test") else {
        return;
    };
    let tail = |reverb: bool| {
        let mut m = booted(&fw);
        if reverb {
            let alg = algorithm_cc(SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_COUNT);
            slot_cc(&mut m, FX_SLOT, ALGORITHM_FIELD, alg);
            slot_cc(&mut m, FX_SLOT, AMOUNT_FIELD, 100);
            slot_cc(&mut m, FX_SLOT, MIX_FIELD, 90);
            m.run_ms(200);
        }
        m.midi(&[0x90, A4, 100]);
        let (on_l, on_r) = record_lr(&mut m, 300);
        m.midi(&[0x80, A4, 0]);
        m.run_ms(400);
        let (l, r) = record_lr(&mut m, 300);
        (on_l, on_r, l, r)
    };
    let (dl, dr, dtl, _) = tail(false);
    let (wl, wr, wtl, wtr) = tail(true);
    let rho = correlation(&wl, &wr);
    println!(
        "reverb by CC: L/R correlation {rho:.3} (dry {:.3}); tail rms {:.1} dry, {:.1} wet",
        correlation(&dl, &dr),
        rms(&dtl),
        rms(&wtl)
    );
    assert_eq!(dl, dr, "dry is mono");
    assert!(
        rho < 0.95,
        "the reverb is not stereo: L/R correlation {rho}"
    );
    assert!(
        rms(&wtl) > 10.0 * rms(&dtl).max(1.0),
        "no tail after the note"
    );
    assert!(correlation(&wtl, &wtr) < 0.8, "the tail is not wide");
}

fn overdrive_and_reverb_ccs() -> Vec<(u8, u8)> {
    let cc = |slot: u32, field: u32| (LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + field) as u8;
    vec![
        (
            cc(DISTORTION_SLOT, ALGORITHM_FIELD),
            algorithm_cc(SID_DIST_OVERDRIVE, SID_DIST_COUNT),
        ),
        (cc(DISTORTION_SLOT, AMOUNT_FIELD), 100),
        (cc(DISTORTION_SLOT, MIX_FIELD), 127),
        (
            cc(FX_SLOT, ALGORITHM_FIELD),
            algorithm_cc(SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_COUNT),
        ),
        (cc(FX_SLOT, AMOUNT_FIELD), 100),
        (cc(FX_SLOT, MIX_FIELD), 90),
    ]
}

#[test]
fn the_slots_on_the_device_match_the_host_sample_exact() {
    let Some(fw) = firmware("the effects test") else {
        return;
    };
    let ccs = overdrive_and_reverb_ccs();
    let mut m = booted(&fw);
    let halves = |m: &Machine| m.soc.periph.audio.len() / (2 * HALF_FRAMES);
    let cc_sent = halves(&m);
    for &(cc, value) in &ccs {
        m.midi(&[0xB0, cc, value]);
    }
    m.run_ms(100);
    let key_sent = halves(&m);
    m.key(A4_KEY, true);
    m.run_ms(400);
    let audio = m.soc.periph.audio.clone();
    assert!(m.left() != m.right(), "the reverb's output is mono");
    assert!(
        common::tap_on(&mut m, &fw),
        "the tap is off: the match says nothing about it"
    );

    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&s| s != 0) {
            break;
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        let before_note = (key_sent - silent) * HALF_FRAMES;
        for first in (cc_sent - silent..).take(LATE_BLOCKS) {
            for schedule in cc_schedules(first, &ccs) {
                let host = render_host(before_note, A4, usize::MAX, &schedule);
                if max_diff(&played[..2 * before_note], &host) != 0 {
                    continue;
                }
                for note_block in (key_sent - silent..).take(LATE_BLOCKS) {
                    let host = render_host(played.len() / 2, A4, note_block, &schedule);
                    let d = max_diff(played, &host);
                    min_diff = min_diff.min(d);
                    if d == 0 {
                        let blocks: Vec<usize> = schedule.iter().map(|c| c.0).collect();
                        found = Some((silent, blocks, note_block));
                        break 'search;
                    }
                }
            }
        }
    }
    println!(
        "slots on the device vs host: (silent halves, CC blocks, note block) {found:?} \
         (sent before blocks {cc_sent}, {key_sent}), closest max difference {min_diff} LSB \
         over {} frames",
        audio.len() / 2
    );
    assert!(
        found.is_some(),
        "the firmware's audio with OVERDRIVE and the reverb on matches no host render: \
         closest max difference {min_diff} LSB"
    );
}

#[test]
fn every_slots_heaviest_fits_the_audio_budget_and_runs_from_sram() {
    let Some(fw) = firmware("the effects test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0x90, 57, 100, 0x90, 64, 100, 0x90, 69, 100]);
    m.run_ms(200);
    let quiet = m.alnk_blocks.len();
    m.watch_alnk_pcs(fw.symbol("_rt_start")..fw.symbol("_rt_end"));
    let from = m.alnk_blocks.len();
    for (slot, alg, count) in [
        (0, SID_DIST_OVERDRIVE, SID_DIST_COUNT),
        (1, SID_MOD_CHORUS, SID_MOD_COUNT),
        (2, SID_FXSLOT_REVERB_LUSH, SID_FXSLOT_COUNT),
    ] {
        slot_cc(&mut m, slot, 0, algorithm_cc(alg, count));
        for field in 1..5 {
            slot_cc(&mut m, slot, field, 127);
        }
    }
    let switching_main = m.main_steps;
    m.run_ms(400);
    let switch_blocks: Vec<u64> = m.alnk_blocks[from..].to_vec();
    let main_before = m.main_steps;
    m.run_ms(1000);
    let main_rate = (m.main_steps - main_before) as f64;
    let steady: Vec<u64> = m.alnk_blocks[from + switch_blocks.len()..].to_vec();
    let per_sample = |v: &[u64]| v.iter().copied().max().unwrap_or(0) as f64 / BLOCK_FRAMES;
    let mean = steady.iter().sum::<u64>() as f64 / steady.len() as f64 / BLOCK_FRAMES;
    let outside: Vec<String> = m
        .alnk_pcs_outside
        .iter()
        .map(|&pc| fw.function_at(pc))
        .collect();
    println!(
        "audio interrupt with every slot's heaviest: {mean:.0} instructions/sample on average, \
         worst block {:.0} steady and {:.0} while switching in (audio target {AUDIO_TARGET}, \
         dry before {:.0}); main loop {:.2}M instructions/s ({} while switching)",
        per_sample(&steady),
        per_sample(&switch_blocks),
        m.alnk_blocks[..quiet].iter().copied().max().unwrap_or(0) as f64 / BLOCK_FRAMES,
        main_rate / 1e6,
        main_before - switching_main
    );
    assert!(steady.len() > 300 && switch_blocks.len() > 100);
    assert!(
        per_sample(&steady) <= AUDIO_TARGET && per_sample(&switch_blocks) <= AUDIO_TARGET,
        "over the audio target"
    );
    assert!(
        outside.is_empty(),
        "the audio interrupt runs code outside .ram_text with the slots on: {outside:?}"
    );
    assert!(
        main_rate < 5e6,
        "the main loop runs {:.2}M/s",
        main_rate / 1e6
    );
    let l = m.left();
    let r = m.right();
    let n = l.len();
    assert!(
        l[n - 10_000..] != r[n - 10_000..],
        "the output is still mono"
    );
}

fn slot_algorithm(m: &mut Machine, clicks: i32) {
    m.turn(SELECT, 1);
    m.run_ms(100);
    m.turn(ALGORITHM, clicks);
    m.run_ms(100);
    m.turn(SELECT, -1);
    m.run_ms(100);
}

#[test]
fn a_slot_algorithm_on_live_is_heard() {
    let Some(fw) = firmware("the effects test") else {
        return;
    };
    let mut m = booted(&fw);
    slot_algorithm(&mut m, 1);
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    let slot = l + sidou_sys::ilp32::live::slot as u32;
    let got: Vec<u8> = (0..5).map(|i| m.soc.read8(slot + i)).collect();
    assert_eq!(got, [1, 80, 0, 0, 127], "TAPE at its musical start");
}

#[test]
fn a_dim_led_is_lit_but_far_less_than_a_full_one() {
    let Some(fw) = firmware("the dim LED test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(100);
    m.soc.periph.led_take();
    m.run_ms(100);
    let off = m.soc.periph.led_take();
    assert_eq!(off[usize::from(ARP)], 0.0, "ARP off: dark");
    m.midi(&[0xB0, sidou_sys::LIVE_CC_ARP as u8, 127]);
    m.run_ms(100);
    m.soc.periph.led_take();
    m.run_ms(100);
    let dim = m.soc.periph.led_take()[usize::from(ARP)];
    press(&mut m, OCT_UP);
    m.run_ms(100);
    m.soc.periph.led_take();
    m.run_ms(100);
    let full = m.soc.periph.led_take()[usize::from(OCT_UP)];
    println!("ARP dim {dim:e}, OCT+ lit {full:e}");
    assert!(dim > 0.0, "ARP on and nothing held: a glow");
    assert!(full > 5.0 * dim, "lit {full} against dim {dim}");
}
