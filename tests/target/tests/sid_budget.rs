// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs the pi32v2 SID benchmark in fm1-emu and enforces the M0 budget.
//! Instructions are not cycles: pi32v2 mostly issues one per cycle, but XIP loads
//! and divides take longer, so treat the result as a lower bound.

use fm1_core::{Bus, Cpu};
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
}

fn build_dir() -> String {
    std::env::var("SIDOU_FM1_BUILD")
        .expect("set SIDOU_FM1_BUILD to the fm1 build directory (e.g. $PWD/build/fm1)")
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

/// fm1-core at the pinned rev decodes the decrement-before block load `{rA, rB, ..} = [-rN]`
/// (`4x eb`, fm1-isa class `r_4_mov_r_4`) but has no semantics for it, and faults. clang emits
/// it in `sid_cutoff_hz` to load a curve point. Run it here with the convention fm1-core uses
/// for the `[rN+]` block moves: base in bits 0-3, register mask in bits 16-31, lowest register
/// at the lowest address; `-rN` starts 4 bytes per register below the base and does not write
/// it back (`--rN` is a separate class). The host comparison checks the result.
fn step_block_load_dec(cpu: &mut Cpu, soc: &mut Soc) -> bool {
    let raw = soc.read32(cpu.pc);
    if raw & 0x0804_fff0 != 0x0000_eb40 {
        return false;
    }
    assert!(
        cpu.rep_end == 0
            && cpu.pred_skips.is_empty()
            && cpu.pair_pending.is_none()
            && cpu.hold_irq == 0,
        "block load at {:#010x} inside a rep, predicated or paired construct",
        cpu.pc
    );
    let mask = raw >> 16;
    let mut addr = cpu.regs[(raw & 0xf) as usize].wrapping_sub(4 * mask.count_ones());
    for r in (0..16).filter(|r| mask >> r & 1 != 0) {
        cpu.regs[r] = soc.read32(addr);
        addr = addr.wrapping_add(4);
    }
    cpu.pc += 4;
    cpu.insn_count += 1;
    true
}

fn run(chips: u32) -> Run {
    let dir = format!("{}/bench/fm1", build_dir());
    let elf = std::fs::read(format!("{dir}/sid_bench_{chips}.elf"))
        .expect("bench ELF; build the fm1 preset");
    let bin = std::fs::read(format!("{dir}/sid_bench_{chips}.bin")).expect("bench image");
    let file = object::File::parse(&*elf).expect("valid ELF");
    let mark = file
        .symbols()
        .find(|s| s.name() == Ok("bench_mark"))
        .expect("bench_mark symbol")
        .address() as u32;

    let mut soc = Soc::new(1 << 20, bin);
    let mut cpu = Cpu::new(XIP_BASE);
    let mut marks = Vec::new();
    let mut steps: u64 = 0;
    while read_u32(&soc, RESULT_ADDR) != RESULT_MAGIC {
        if cpu.pc == mark {
            marks.push(steps);
        }
        let pc = cpu.pc;
        if !step_block_load_dec(&mut cpu, &mut soc) {
            cpu.step(&mut soc)
                .unwrap_or_else(|e| panic!("cpu fault at {pc:#010x}: {e:?}"));
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
    }
}

fn check(chips: u32) {
    let r = run(chips);
    let host = sidou_target_tests::render_host(chips);
    assert_eq!(r.samples.len(), host.len());
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
        "instructions_per_sample": per_sample,
        "percent_of_240mhz_core": pct_core,
        "init_instructions": r.init,
        "max_sample_diff_vs_host": worst,
    });
    println!("{report}");
    let out = format!("{}/sid_budget_{chips}.json", build_dir());
    std::fs::write(&out, report.to_string()).expect("write report");

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
