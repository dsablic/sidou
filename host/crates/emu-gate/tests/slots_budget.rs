// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect slots in fm1-emu (`bench_slots_main.c`, `workload_slots.h`): the cost of
//! the slot frame and of every algorithm with its controls moving all the time, of every
//! slot's heaviest together, and per 32-sample block through preset changes that switch
//! every slot. Asserts the budgets of docs/decisions/0004 and
//! bit-exactness with the host build. Instructions are a lower bound on cycles.

mod common;

use common::{AUDIO_TARGET, TWO_CHIPS_PLUS_EFFECTS};

const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
const FRAMES: u64 = sidou_sys::WORKLOAD_SLOTS_FRAMES as u64;

/// 0004: Distortion 150, Modulation 200, FX 450, the frame 100, 900 for all slots.
const FRAME_BUDGET: f64 = 100.0;
const SLOTS_BUDGET: f64 = 900.0;
/// 0004's switching allowance: while a preset change switches slots (one at a time), a
/// 32-sample block may cost up to this much (the switching slot's per-sample path, and
/// a new algorithm's one-off setup)...
const SWITCH_BLOCK_ALLOWANCE: f64 = 1000.0;
/// ...for at most this many of the scenario's blocks above SLOTS_BUDGET (two preset
/// changes: ~30 ms each).
const SWITCH_BLOCKS_OVER_BUDGET: usize = 80;
/// What the audio target holds today besides the slots, block by block: two chips at the
/// SID's worst block, the LIVE view's tap (on at power-on) and the ISR's overhead. What is
/// left of the target is the headroom for the 6502 (M3), which does not exist yet (decision
/// record 0004, "Worst blocks").
const AUDIO_REST_AT_WORST: f64 =
    common::SID_WORST_BLOCK_TWO_CHIPS + common::TAP_PER_SAMPLE + common::ISR_OVERHEAD;
/// Two chips at the SID's worst block, with the LIVE view's tap.
const TWO_CHIPS_AT_WORST: f64 = common::SID_WORST_BLOCK_TWO_CHIPS + common::TAP_PER_SAMPLE;

/// Scenario names and slot budgets, in workload_slots.c's order (0 is the frame alone,
/// the last every slot's heaviest).
const ALGORITHMS: [(&str, &str, f64); 15] = [
    ("tape", "distortion", 150.0),
    ("overdrive", "distortion", 150.0),
    ("fuzz", "distortion", 150.0),
    ("crush", "distortion", 150.0),
    ("lofi", "distortion", 150.0),
    ("chorus", "modulation", 200.0),
    ("flanger", "modulation", 200.0),
    ("phaser", "modulation", 200.0),
    ("tremolo", "modulation", 200.0),
    ("autopan", "modulation", 200.0),
    ("wow", "modulation", 200.0),
    ("delay", "fx", 450.0),
    ("pingpong", "fx", 450.0),
    ("reverb_light", "fx", 450.0),
    ("reverb_lush", "fx", 450.0),
];
/// workload_slots.h's WORKLOAD_SLOTS_HEAVIEST_*, by name.
const HEAVIEST: [&str; 3] = ["overdrive", "chorus", "reverb_lush"];

fn check(chips: u32) {
    let Some(build) = common::build_dir() else {
        println!(
            "skipping the emulator gate: set SIDOU_FM1_BUILD to the fm1 build directory \
             (e.g. $PWD/../build/fm1 from host/) after building the fm1 preset"
        );
        return;
    };
    let count = ALGORITHMS.len() + 2;
    let blocks = sidou_sys::WORKLOAD_SLOTS_SWITCH_BLOCKS as usize;
    let r = common::run(
        &build,
        &format!("sid_bench_slots_{chips}"),
        3 + 2 * count + 1 + blocks,
    );
    let host = emu_gate::render_host_slots(chips);
    assert_eq!(r.samples.len(), host.len());
    let diffs = r.samples.iter().zip(&host).filter(|(a, b)| a != b).count();
    let first = r.samples.iter().zip(&host).position(|(a, b)| a != b);

    let sid = r.phases[1] as f64 / sidou_sys::WORKLOAD_SAMPLES as f64;
    // phases: SID init, SID render, then per scenario its start and its measured run.
    let cost = |s: usize| r.phases[3 + 2 * s] as f64 / FRAMES as f64;
    let frame = cost(0);
    let heaviest_all = cost(count - 1);
    // The switch scenario: per block of SWITCH_BLOCK.
    let block = f64::from(sidou_sys::WORKLOAD_SLOTS_SWITCH_BLOCK);
    let switch_blocks: Vec<f64> = (0..blocks)
        .map(|b| r.phases[3 + 2 * count + b] as f64 / block)
        .collect();
    let switch_worst = switch_blocks.iter().fold(0.0f64, |m, &v| m.max(v));
    let mut per = serde_json::Map::new();
    let mut slot_max: [(&str, f64); 3] = [("", 0.0); 3];
    for (i, &(name, slot, _)) in ALGORITHMS.iter().enumerate() {
        let c = cost(i + 1) - frame;
        per.insert(
            name.into(),
            serde_json::json!({ "slot": slot, "instructions_per_sample": c }),
        );
        let k = ["distortion", "modulation", "fx"]
            .iter()
            .position(|&s| s == slot)
            .unwrap();
        if c > slot_max[k].1 {
            slot_max[k] = (name, c);
        }
    }
    let report = serde_json::json!({
        "chips": chips,
        "metric": "instructions per sample (lower bound on cycles; assumes 1 IPC)",
        "sid_instructions_per_sample": sid,
        "frame_instructions_per_sample": frame,
        "algorithms_above_the_frame": per,
        "all_heaviest_slots_instructions_per_sample": heaviest_all,
        "switch_worst_block_instructions_per_sample": switch_worst,
        "switch_blocks_over_slots_budget": switch_blocks.iter().filter(|&&v| v > SLOTS_BUDGET).count(),
        "switch_blocks_instructions_per_sample": switch_blocks,
        "sid_plus_slots_instructions_per_sample": sid + heaviest_all,
        "sid_plus_slots_percent_of_240mhz_core": (sid + heaviest_all) * SAMPLE_RATE / CORE_HZ * 100.0,
        "two_chips_at_worst_plus_slots_instructions_per_sample": TWO_CHIPS_AT_WORST + heaviest_all,
        "audio_rest_at_worst_plus_switch_instructions_per_sample": AUDIO_REST_AT_WORST + switch_worst,
        "headroom_for_6502_instructions_per_sample": AUDIO_TARGET - AUDIO_REST_AT_WORST - switch_worst,
        "samples_differing_from_host": diffs,
        "emulated_block_loads": r.emulated_block_loads,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    let out = build.join(format!("slots_budget_{chips}.json"));
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    if let Some(i) = first {
        panic!(
            "target output differs from host build in {diffs} samples; first at {i} (scenario {}, \
             {count} is the switch): target {} host {}",
            i / (2 * FRAMES as usize),
            r.samples[i],
            host[i]
        );
    }
    let peak = host.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    assert!(
        peak > 1000,
        "the slots' output peaks at {peak}; the comparison would be vacuous"
    );
    for (i, &(name, slot, budget)) in ALGORITHMS.iter().enumerate() {
        let c = cost(i + 1) - frame;
        assert!(
            c <= budget,
            "{name} costs {c:.0} instructions/sample, over the {slot} budget of {budget}"
        );
    }
    assert!(
        frame <= FRAME_BUDGET,
        "the slot frame costs {frame:.0}, over {FRAME_BUDGET}"
    );
    for (k, &(name, _)) in slot_max.iter().enumerate() {
        assert_eq!(
            name,
            HEAVIEST[k],
            "the heaviest {} is {name}: update WORKLOAD_SLOTS_HEAVIEST_* and HEAVIEST",
            ["distortion", "modulation", "fx"][k]
        );
    }
    assert!(
        heaviest_all <= SLOTS_BUDGET,
        "every slot's heaviest together costs {heaviest_all:.0}, over {SLOTS_BUDGET}"
    );
    let over = switch_blocks.iter().filter(|&&v| v > SLOTS_BUDGET).count();
    println!("switching: worst block {switch_worst:.0}, {over} blocks above {SLOTS_BUDGET}");
    println!(
        "headroom for the 6502 (M3): {:.0} instructions/sample (target {AUDIO_TARGET} - two \
         chips' worst block {} - the LIVE tap {} - ISR {} - worst switching block \
         {switch_worst:.0})",
        AUDIO_TARGET - AUDIO_REST_AT_WORST - switch_worst,
        common::SID_WORST_BLOCK_TWO_CHIPS,
        common::TAP_PER_SAMPLE,
        common::ISR_OVERHEAD
    );
    assert!(
        switch_worst <= SWITCH_BLOCK_ALLOWANCE,
        "a switching block costs {switch_worst:.0}, over the allowance of {SWITCH_BLOCK_ALLOWANCE}"
    );
    assert!(
        over <= SWITCH_BLOCKS_OVER_BUDGET,
        "{over} switching blocks over {SLOTS_BUDGET}, more than {SWITCH_BLOCKS_OVER_BUDGET}"
    );
    assert!(
        AUDIO_REST_AT_WORST + switch_worst <= AUDIO_TARGET,
        "a switch with two chips at their worst block, the tap and the ISR would cost {:.0}, \
         over the audio target {AUDIO_TARGET}",
        AUDIO_REST_AT_WORST + switch_worst
    );
    // Two chips at the SID's worst block and the LIVE view's tap plus every slot's heaviest (the
    // workload's chips, measured above, cost less).
    assert!(
        TWO_CHIPS_AT_WORST + heaviest_all <= TWO_CHIPS_PLUS_EFFECTS,
        "two chips at the SID's worst block and the tap plus the slots cost {:.0}, over \
         {TWO_CHIPS_PLUS_EFFECTS}",
        TWO_CHIPS_AT_WORST + heaviest_all
    );
}

#[test]
fn slots_one_chip_fit_their_budgets_and_match_host() {
    check(1);
}

#[test]
fn slots_two_chips_fit_their_budgets_and_match_host() {
    check(2);
}
