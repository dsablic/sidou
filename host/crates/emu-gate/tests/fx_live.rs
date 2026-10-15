// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the effect slots in the firmware (platform/fm1/audio.c runs them after the
//! chip, in stereo), switched on from the front panel's EDIT layer and over TRS MIDI CCs, and
//! their cost in the audio interrupt with every slot's heaviest algorithm.

mod common;

use common::{
    build_dir, max_diff, peak, power_above, render_host, Firmware, Machine, AUDIO_TARGET,
    HALF_FRAMES,
};
use sidou_sys::{
    LIVE_CC_SLOT_FIRST, LIVE_SLOT_FIELDS, SID_DIST_COUNT, SID_DIST_OVERDRIVE, SID_FXSLOT_COUNT,
    SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_REVERB_LUSH, SID_MOD_CHORUS, SID_MOD_COUNT,
};

/// Felucca's ids: the buttons of the effect layer (app/panel.h), K1 and K4, the A4 key.
const ENV: u8 = 4;
const EDIT: u8 = 6;
const K1: u8 = 2;
const K4: u8 = 5;
const A4_KEY: u8 = 30;
const A4: u8 = 69;
/// A block of 128 frames.
const BLOCK_FRAMES: f64 = 128.0;
/// The DMA plays the zeroed buffer before the first half the interrupt filled.
const MAX_SILENT_HALVES: usize = 4;
/// How many blocks after it was sent a CC or a key press may reach the interrupt.
const LATE_BLOCKS: usize = 8;
/// How many blocks after the first a later CC of the same burst may reach the interrupt:
/// over TRS a CC takes about 1 ms at 31,250 baud and a block 2.9 ms, and the main loop
/// commits what has arrived, so a burst of CCs spreads over a few blocks.
const CC_SPREAD: usize = 3;

/// Every way the CCs of a burst can land: the first in `first`, each later one in the same
/// block as the one before it or a later one, within CC_SPREAD blocks of the first.
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

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

/// Past the boot animation (app/boot.h), which a control would skip instead of playing:
/// until the boot view is gone from the screen (common::run_past_boot).
fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
    m
}

fn tap(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// Both channels played during the next `ms`.
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

/// The CC value that selects algorithm `alg` of a slot with `count` algorithms (the middle
/// of its band: live_cc maps value * count / 128).
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
    let Some(fw) = fixture("the effects test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(A4_KEY, true);
    m.run_ms(200);
    m.soc.periph.led_take();
    let (dry, dry_r) = record_lr(&mut m, 300);
    let edit_before = m.soc.periph.led_take()[usize::from(EDIT)];
    // Distortion is the selected slot at boot: ENV twice is OVERDRIVE (OFF, TAPE, ...).
    tap(&mut m, ENV);
    tap(&mut m, ENV);
    // fm1-emu turns a knob one click at a time: hold EDIT until every click has landed.
    m.key(EDIT, true);
    m.run_ms(30);
    m.turn(K1, 32); // Amount (drive) to the top
    m.run_ms(400);
    m.turn(K1 + 1, 32); // A (tone) to the brightest
    m.run_ms(400);
    m.turn(K4, 32); // Mix to the top
    m.run_ms(400);
    m.key(EDIT, false);
    m.run_ms(200); // past the switch (20 ms) and the smoothing
    let (wet, wet_r) = record_lr(&mut m, 300);
    // Clipping squares the wave off: the crest factor (peak / rms) falls, the level rises.
    let crest = |x: &[f32]| f64::from(peak(x)) / rms(x);
    let (cd, cw) = (crest(&dry), crest(&wet));
    println!(
        "overdrive from the panel: crest factor {cd:.2} -> {cw:.2}, rms {:.0} -> {:.0}, \
         power above 2 kHz {:.3} -> {:.3}",
        rms(&dry),
        rms(&wet),
        power_above(&dry, 2000.0),
        power_above(&wet, 2000.0)
    );
    assert_eq!(dry, dry_r, "dry is mono");
    assert_eq!(wet, wet_r, "the distortion slot is mono");
    // 0.85: SAW LEAD drives the slot 3.3 dB less since M1b Task 6 put the 6581's filter at
    // reSIDfp's level (2.01 -> 1.63 measured then; 0.8 was set on the louder M1a filter).
    assert!(cw < 0.85 * cd, "no clipping: crest factor {cd} -> {cw}");
    assert!(rms(&wet) > 2.0 * rms(&dry), "the drive did not come up");
    // EDIT's LED shows the selected slot is on (the LEDs are multiplexed: lit is above 0).
    m.soc.periph.led_take();
    m.run_ms(50);
    let lit = m.soc.periph.led_take()[usize::from(EDIT)];
    assert_eq!(edit_before, 0.0, "EDIT lit while every slot was off");
    assert!(lit > 0.0, "EDIT's LED is dark with the slot on");
}

#[test]
fn trs_ccs_turn_on_the_reverb_in_stereo_with_a_tail() {
    let Some(fw) = fixture("the effects test") else {
        return;
    };
    // The same note with the FX slot off and with the light reverb.
    let tail = |reverb: bool| {
        let mut m = booted(&fw);
        if reverb {
            let alg = algorithm_cc(SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_COUNT);
            slot_cc(&mut m, 2, 0, alg);
            slot_cc(&mut m, 2, 1, 100); // Amount: a long decay
            slot_cc(&mut m, 2, 4, 90); // Mix
            m.run_ms(200);
        }
        m.midi(&[0x90, A4, 100]);
        let (on_l, on_r) = record_lr(&mut m, 300);
        m.midi(&[0x80, A4, 0]);
        m.run_ms(400); // past the preset's release
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

/// The slot CCs of the sample-exact test: OVERDRIVE in Distortion, the light reverb in FX.
fn overdrive_and_reverb_ccs() -> Vec<(u8, u8)> {
    let cc = |slot: u32, field: u32| (LIVE_CC_SLOT_FIRST + slot * LIVE_SLOT_FIELDS + field) as u8;
    vec![
        (cc(0, 0), algorithm_cc(SID_DIST_OVERDRIVE, SID_DIST_COUNT)),
        (cc(0, 1), 100),
        (cc(0, 4), 127),
        (
            cc(2, 0),
            algorithm_cc(SID_FXSLOT_REVERB_LIGHT, SID_FXSLOT_COUNT),
        ),
        (cc(2, 1), 100),
        (cc(2, 4), 90),
    ]
}

/// The device's audio_fill with two slots on matches the host build sample for sample on
/// both channels: the slots switched on over TRS MIDI, then the A4 key pressed.
#[test]
fn the_slots_on_the_device_match_the_host_sample_exact() {
    let Some(fw) = fixture("the effects test") else {
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
    // LIVE, the root, on top all along: the audio interrupt rendered through the tap
    // (app/scope.h), which must change nothing in the audio.
    assert!(
        common::tap_on(&mut m, &fw),
        "the tap is off: the match says nothing about it"
    );

    // The blocks the CCs and the note-on reached the interrupt in depend on the MIDI drain
    // and the key scan: search a few blocks after each was sent. The chip plays before the
    // note (its silent voices leak, as reSIDfp's), so the slots' output there already shows
    // which block each CC reached: find that from the audio before the note, then the note.
    let mut found = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&s| s != 0) {
            break; // the DMA's zeros only come before the first rendered half
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
    let Some(fw) = fixture("the effects test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0x90, 57, 100, 0x90, 64, 100, 0x90, 69, 100]);
    m.run_ms(200);
    let quiet = m.alnk_blocks.len();
    // Every slot's heaviest (docs/decisions/0004: overdrive, chorus, lush) at full, switched
    // in from MIDI while the chord plays; switching included in what is measured.
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
    m.run_ms(400); // the three switches take turns: ~150 ms
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
    // Stereo out: the chorus and the plate make the sides differ.
    let l = m.left();
    let r = m.right();
    let n = l.len();
    assert!(
        l[n - 10_000..] != r[n - 10_000..],
        "the output is still mono"
    );
}
