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

const RESULT_ADDR: u32 = 0x01C8_0000;
const RESULT_MAGIC: u32 = 0x5349_4442;
const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
/// Design doc section 8: at most 900 instructions per sample per chip.
const BUDGET_PER_CHIP: f64 = 900.0;
const MAX_STEPS: u64 = 1_000_000_000;

struct Run {
    init: u64,
    render: u64,
    samples: Vec<i16>,
    /// Instructions run by `step_block_load_dec` instead of fm1-core.
    emulated_block_loads: u64,
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

fn run(build: &Path, chips: u32) -> Run {
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

    let mut soc = Soc::new(1 << 20, bin);
    let mut cpu = Cpu::new(XIP_BASE);
    let mut marks = Vec::new();
    let mut steps: u64 = 0;
    let mut emulated_block_loads: u64 = 0;
    while read_u32(&soc, RESULT_ADDR) != RESULT_MAGIC {
        if cpu.pc == mark {
            marks.push(steps);
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
    Run {
        init: marks[1] - marks[0],
        render: marks[2] - marks[1],
        samples,
        emulated_block_loads,
    }
}

fn check(chips: u32) {
    let Some(build) = build_dir() else {
        common::skip_message("the emulator gate");
        return;
    };
    let r = run(&build, chips);
    let host = emu_gate::render_host(chips);
    assert_eq!(r.samples.len(), host.len());
    // Equal outputs prove nothing if both are silent.
    let host_peak = host.iter().map(|&s| i32::from(s).abs()).max().unwrap_or(0);
    assert!(
        host_peak > 1000,
        "host output peaks at {host_peak}; the comparison would be vacuous"
    );
    let diffs: Vec<i32> = r
        .samples
        .iter()
        .zip(&host)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .collect();
    let worst = *diffs.iter().max().unwrap();

    let per_sample = r.render as f64 / r.samples.len() as f64;
    let pct_core = per_sample * SAMPLE_RATE / CORE_HZ * 100.0;
    let report = serde_json::json!({
        "chips": chips,
        "metric": "instructions (lower bound on cycles; assumes 1 IPC)",
        "instructions_per_sample": per_sample,
        "percent_of_240mhz_core": pct_core,
        "init_instructions": r.init,
        "max_sample_diff_vs_host": worst,
        "emulated_block_loads": r.emulated_block_loads,
    });
    println!("{report}");
    let out = build.join(format!("sid_budget_{chips}.json"));
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    if let Some(i) = diffs.iter().position(|&d| d > 1) {
        panic!(
            "target output differs from host build by {worst} LSB; first at sample {i}: target {} host {}",
            r.samples[i], host[i]
        );
    }
    assert!(
        per_sample <= BUDGET_PER_CHIP * chips as f64,
        "{per_sample:.0} instructions/sample exceeds the {chips}-chip budget of {}",
        BUDGET_PER_CHIP * chips as f64
    );
}

#[test]
fn one_chip_fits_budget_and_matches_host() {
    check(1);
}

#[test]
fn two_chips_fit_budget_and_match_host() {
    check(2);
}
