// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs the pi32v2 SID benchmark in fm1-emu and enforces the M0 budget.
//! Instructions are not cycles: pi32v2 mostly issues one per cycle, but XIP loads
//! and divides take longer, so treat the result as a lower bound.

mod common;

use std::path::Path;

use common::{build_dir, read, step};
use fm1_core::Cpu;
use fm1_soc::{Soc, RAM_BASE, XIP_BASE};
use object::{Object, ObjectSymbol};
use serde_json::Value;
use sidou_sys::{sid_model, SID_MODEL_6581, SID_MODEL_8580};

const RESULT_ADDR: u32 = 0x01C8_0000;
const RESULT_MAGIC: u32 = 0x5349_4442;
/// bench_main.c reads the scenario number here before it starts.
const SCENARIO_ADDR: u32 = RESULT_ADDR + 8;
/// And the model here: 0 for the 6581, 1 for the 8580.
const MODEL_ADDR: u32 = RESULT_ADDR + 12;
const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
/// Design doc section 8: at most 900 instructions per sample per chip.
const BUDGET_PER_CHIP: f64 = 900.0;
const MAX_STEPS: u64 = 1_000_000_000;
/// workload.c's block size and frame length in samples.
const WORKLOAD_BLOCK: u32 = 128;
const SAMPLES_PER_FRAME: u32 = 882;

struct Run {
    /// Instructions per sample in the most expensive block.
    worst_block: f64,
    init: u64,
    render: u64,
    samples: Vec<i16>,
    /// Instructions run by `step_block_load_dec` instead of fm1-core.
    emulated_block_loads: u64,
    /// Render instructions per function, when profiling.
    profile: Vec<(String, u64)>,
    /// The same for the most expensive block alone.
    worst_block_profile: Vec<(String, u64)>,
    /// Instructions per sample, block by block, when profiling.
    blocks: Vec<f64>,
}

/// SIDOU_GATE_PROFILE=1 counts the render instructions per function (slower).
fn profiling() -> bool {
    std::env::var_os("SIDOU_GATE_PROFILE").is_some_and(|v| v == "1")
}

fn read_u32(soc: &Soc, addr: u32) -> u32 {
    let o = addr - RAM_BASE;
    u32::from_le_bytes([
        soc.ram.read8(o),
        soc.ram.read8(o + 1),
        soc.ram.read8(o + 2),
        soc.ram.read8(o + 3),
    ])
}

fn run(build: &Path, chips: u32, model: sid_model, scenario: u32) -> Run {
    let dir = build.join("bench/fm1");
    let elf_path = dir.join(format!("sid_bench_{chips}.elf"));
    let elf = read(&elf_path, "bench ELF");
    let bin = read(&dir.join(format!("sid_bench_{chips}.bin")), "bench image");
    let file = object::File::parse(&*elf)
        .unwrap_or_else(|e| panic!("parse ELF {}: {e}", elf_path.display()));
    let mark = file
        .symbols()
        .find(|s| s.name() == Ok("bench_mark"))
        .expect("bench_mark symbol")
        .address() as u32;
    let render_entry = file
        .symbols()
        .find(|s| s.name() == Ok("sid_render"))
        .expect("sid_render symbol")
        .address() as u32;
    let mut render_calls = Vec::new();
    let mut functions: Vec<(u32, u32, String)> = file
        .symbols()
        .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
        .map(|s| {
            (
                s.address() as u32,
                (s.address() + s.size()) as u32,
                s.name().unwrap_or("?").to_owned(),
            )
        })
        .collect();
    functions.sort();
    let profile = profiling();
    let mut counts = vec![0u64; functions.len() + 1];
    // Per block (counted in sid_render calls of the first chip), when profiling.
    let mut block_counts: Vec<Vec<u64>> = Vec::new();

    let mut soc = Soc::new(1 << 20, bin);
    soc.ram.write32(SCENARIO_ADDR - RAM_BASE, scenario);
    soc.ram
        .write32(MODEL_ADDR - RAM_BASE, u32::from(model == SID_MODEL_8580));
    let mut cpu = Cpu::new(XIP_BASE);
    let mut marks = Vec::new();
    let mut steps: u64 = 0;
    let mut emulated_block_loads: u64 = 0;
    while read_u32(&soc, RESULT_ADDR) != RESULT_MAGIC {
        if cpu.pc == mark {
            marks.push(steps);
        }
        if cpu.pc == render_entry && marks.len() == 2 {
            render_calls.push(steps);
        }
        let pc = cpu.pc;
        if profile && marks.len() == 2 {
            let i = functions.partition_point(|f| f.0 <= pc);
            let slot = match i.checked_sub(1) {
                Some(j) if pc < functions[j].1 => j,
                _ => functions.len(),
            };
            counts[slot] += 1;
            if let Some(b) = render_calls.len().checked_sub(1) {
                let b = b / chips as usize;
                if block_counts.len() <= b {
                    block_counts.resize(b + 1, vec![0u64; functions.len() + 1]);
                }
                block_counts[b][slot] += 1;
            }
        }
        if step(&mut cpu, &mut soc) {
            emulated_block_loads += 1;
        }
        steps += 1;
        assert!(
            steps < MAX_STEPS,
            "benchmark did not finish (pc {:#010x})",
            cpu.pc
        );
    }
    assert_eq!(
        marks.len(),
        3,
        "expected 3 bench_mark calls, saw {}",
        marks.len()
    );

    let n = read_u32(&soc, RESULT_ADDR + 4) as usize;
    let base = RESULT_ADDR + 0x40 - RAM_BASE;
    let samples = (0..n)
        .map(|i| {
            let o = base + 2 * i as u32;
            i16::from_le_bytes([soc.ram.read8(o), soc.ram.read8(o + 1)])
        })
        .collect();
    let by_function = |counts: &[u64]| {
        let mut p: Vec<(String, u64)> = counts
            .iter()
            .enumerate()
            .filter(|(_, &c)| c > 0)
            .map(|(i, &c)| {
                let name = functions.get(i).map_or("(no symbol)", |f| f.2.as_str());
                (name.to_owned(), c)
            })
            .collect();
        p.sort_by_key(|p| std::cmp::Reverse(p.1));
        p
    };
    let profile_all = by_function(&counts);
    // Blocks as workload_render_samples cuts them: at most 128 samples, never across a
    // frame; each block calls sid_render once per chip.
    assert_eq!(render_calls.len() % chips as usize, 0);
    let mut starts: Vec<u64> = render_calls
        .iter()
        .step_by(chips as usize)
        .copied()
        .collect();
    starts.push(marks[2]);
    let mut worst_block = 0.0f64;
    let mut worst_index = 0;
    let mut blocks = Vec::new();
    let mut position = 0u32;
    for (i, w) in starts.windows(2).enumerate() {
        let len = (WORKLOAD_BLOCK.min(SAMPLES_PER_FRAME - position % SAMPLES_PER_FRAME))
            .min(n as u32 - position);
        let cost = (w[1] - w[0]) as f64 / f64::from(len);
        if cost > worst_block {
            worst_block = cost;
            worst_index = i;
        }
        if profile {
            blocks.push(cost);
        }
        position += len;
    }
    let worst_block_profile = block_counts
        .get(worst_index)
        .map(|c| by_function(c))
        .unwrap_or_default();
    assert_eq!(
        position as usize, n,
        "block schedule out of step with the workload"
    );
    Run {
        worst_block,
        init: marks[1] - marks[0],
        render: marks[2] - marks[1],
        samples,
        emulated_block_loads,
        profile: profile_all,
        worst_block_profile,
        blocks,
    }
}

/// Runs one scenario; returns its report and any failure.
fn check_scenario(
    build: &Path,
    chips: u32,
    model: sid_model,
    scenario: u32,
    scenario_name: &str,
) -> (Value, Vec<String>) {
    let model_name = if model == SID_MODEL_8580 {
        "8580"
    } else {
        "6581"
    };
    let name = &format!("{scenario_name} ({model_name})");
    let r = run(build, chips, model, scenario);
    let host = emu_gate::render_host(chips, model, scenario);
    let mut failures = Vec::new();
    assert_eq!(r.samples.len(), host.len());
    // Equal outputs prove nothing if both are silent. The idle scenario is near-silent
    // by design; it is there for its cost.
    let host_peak = host.iter().map(|&s| i32::from(s).abs()).max().unwrap_or(0);
    if scenario_name != "idle" && host_peak <= 1000 {
        failures.push(format!(
            "{name}: host output peaks at {host_peak}; the comparison would be vacuous"
        ));
    }
    let diffs: Vec<i32> = r
        .samples
        .iter()
        .zip(&host)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .collect();
    let worst = *diffs.iter().max().unwrap();
    if let Some(i) = diffs.iter().position(|&d| d > 1) {
        failures.push(format!(
            "{name}: target output differs from host build by {worst} LSB; first at sample \
             {i}: target {} host {}",
            r.samples[i], host[i]
        ));
    }
    let per_sample = r.render as f64 / r.samples.len() as f64;
    let per_chip = per_sample / f64::from(chips);
    let budget = BUDGET_PER_CHIP * f64::from(chips);
    if per_chip > BUDGET_PER_CHIP {
        failures.push(format!(
            "{name}: {per_sample:.0} instructions/sample exceeds the {chips}-chip budget of \
             {budget}"
        ));
    }
    // The budget holds block by block too: audio is produced a block at a time.
    if r.worst_block > budget {
        failures.push(format!(
            "{name}: its worst block takes {:.0} instructions/sample, over the {chips}-chip \
             budget of {budget}",
            r.worst_block
        ));
    }
    let mut report = serde_json::json!({
        "scenario": scenario_name,
        "model": model_name,
        "instructions_per_sample": per_sample,
        "instructions_per_sample_per_chip": per_chip,
        "worst_block_instructions_per_sample": r.worst_block,
        "percent_of_240mhz_core": per_sample * SAMPLE_RATE / CORE_HZ * 100.0,
        "init_instructions": r.init,
        "max_sample_diff_vs_host": worst,
        "emulated_block_loads": r.emulated_block_loads,
    });
    if !r.profile.is_empty() {
        let n = r.samples.len() as f64;
        report["profile_instructions_per_sample"] = r
            .profile
            .iter()
            .take(12)
            .map(|(f, c)| (f.clone(), Value::from(*c as f64 / n)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        report["worst_block_profile_instructions"] = r
            .worst_block_profile
            .iter()
            .take(12)
            .map(|(f, c)| (f.clone(), Value::from(*c)))
            .collect::<serde_json::Map<_, _>>()
            .into();
        report["blocks_instructions_per_sample"] = r.blocks.iter().map(|&b| b.round()).collect();
    }
    (report, failures)
}

/// Runs every scenario on both models with `chips` chips. The top-level fields of the
/// JSON report are the triad's on the 6581, comparable with earlier milestones;
/// "scenarios" has them all.
fn check(chips: u32) {
    let Some(build) = build_dir() else {
        common::skip_message("the emulator gate");
        return;
    };
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        for (scenario, name) in emu_gate::scenarios() {
            let (report, f) = check_scenario(&build, chips, model, scenario, &name);
            println!("{report}");
            reports.push(report);
            failures.extend(f);
        }
    }
    let worst = reports
        .iter()
        .map(|r| r["instructions_per_sample"].as_f64().unwrap())
        .fold(0.0, f64::max);
    let worst_per_chip = worst / f64::from(chips);
    if worst_per_chip > common::SID_WORST_PER_CHIP {
        failures.push(format!(
            "the worst scenario takes {worst_per_chip:.1} instructions/sample per chip, over \
             common::SID_WORST_PER_CHIP ({}), which the effect gates assume",
            common::SID_WORST_PER_CHIP
        ));
    }
    let mut report = reports[0].clone();
    report["chips"] = chips.into();
    report["metric"] = "instructions (lower bound on cycles; assumes 1 IPC)".into();
    report["max_instructions_per_sample"] = worst.into();
    report["scenarios"] = reports.into();
    let out = build.join(format!("sid_budget_{chips}.json"));
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn one_chip_fits_budget_and_matches_host() {
    check(1);
}

#[test]
fn two_chips_fit_budget_and_match_host() {
    check(2);
}
