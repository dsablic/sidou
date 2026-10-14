// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect slots in fm1-emu (`bench_slots_main.c`, `workload_slots.h`): the cost of
//! the slot frame and of every algorithm with its controls moving all the time, and of
//! every slot's heaviest together. Asserts the budgets of docs/decisions/0004 and
//! bit-exactness with the host build. Instructions are a lower bound on cycles.

mod common;

const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
const FRAMES: u64 = sidou_sys::WORKLOAD_SLOTS_FRAMES as u64;

/// 0004: Distortion 150, Modulation 200, FX 450, the frame 100, 900 for all slots.
const FRAME_BUDGET: f64 = 100.0;
const SLOTS_BUDGET: f64 = 900.0;
/// Task 1's cap for two chips plus the effects.
const TWO_CHIPS_PLUS_EFFECTS: f64 = 2300.0;

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
    let r = common::run(&build, &format!("sid_bench_slots_{chips}"), 3 + 2 * count);
    let host = emu_gate::render_host_slots(chips);
    assert_eq!(r.samples.len(), host.len());
    let diffs = r.samples.iter().zip(&host).filter(|(a, b)| a != b).count();
    let first = r.samples.iter().zip(&host).position(|(a, b)| a != b);

    let sid = r.phases[1] as f64 / sidou_sys::WORKLOAD_SAMPLES as f64;
    // phases: SID init, SID render, then per scenario its start and its measured run.
    let cost = |s: usize| r.phases[3 + 2 * s] as f64 / FRAMES as f64;
    let frame = cost(0);
    let heaviest_all = cost(count - 1);
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
        "sid_plus_slots_instructions_per_sample": sid + heaviest_all,
        "sid_plus_slots_percent_of_240mhz_core": (sid + heaviest_all) * SAMPLE_RATE / CORE_HZ * 100.0,
        "samples_differing_from_host": diffs,
        "emulated_block_loads": r.emulated_block_loads,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    let out = build.join(format!("slots_budget_{chips}.json"));
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    if let Some(i) = first {
        panic!(
            "target output differs from host build in {diffs} samples; first at {i} (scenario {}): \
             target {} host {}",
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
    if chips == 2 {
        assert!(
            sid + heaviest_all <= TWO_CHIPS_PLUS_EFFECTS,
            "two chips plus the slots cost {:.0}, over {TWO_CHIPS_PLUS_EFFECTS}",
            sid + heaviest_all
        );
    }
}

#[test]
fn slots_one_chip_fit_their_budgets_and_match_host() {
    check(1);
}

#[test]
fn slots_two_chips_fit_their_budgets_and_match_host() {
    check(2);
}
