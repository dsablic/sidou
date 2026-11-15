// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ops::Range;
use std::path::Path;

use fm1_core::{Bus, Cpu};
use fm1_soc::flaky::FlakyCore1;
use fm1_soc::machine::{released_cpu1, Core1Bus};
use fm1_soc::{Soc, RAM_BASE, XIP_BASE};
use object::{Object, ObjectSymbol};

use super::{read, step};

const RESULT_ADDR: u32 = 0x01C8_0000;
const RESULT_MAGIC: u32 = 0x5349_4442;
const SAMPLES_OFFSET: u32 = 0x40;
const RESULT_WORDS: usize = 12;
const C1_CON: u32 = 0x01EE_E004;
const C1_CON_RUN: u32 = 8;
const C1_CON_HOLD: u32 = 2;
const RAM_SIZE: usize = 1 << 20;
const MAX_STEPS: u64 = 2_000_000_000;

const DBG_C1_PC_LIMIT: u32 = 10;
const POLLING_STEPS: u64 = 64;
const FPU_PREFIX: [u8; 2] = [0x3f, 0xe5];
const FPU_PREFIX_WIDE: [u8; 2] = [0x3f, 0xf5];

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Misreads {
    #[default]
    Anywhere,
    InJobs,
    InTheLoop,
}

#[derive(Clone, Copy, Default)]
pub struct Core1Fate {
    pub flaky: Option<(f64, u64)>,
    pub misreads_mailbox_after_jobs: Option<u32>,
    pub stalls_after: Option<u64>,
    pub misreads: Misreads,
    pub flaky_after_jobs: Option<u32>,
    pub stalls_after_jobs: Option<u32>,
    pub faults_at_fpu: bool,
}

#[derive(Default)]
pub struct Core1 {
    cpu: Option<Cpu>,
    pub fate: Core1Fate,
    pub mailbox_loop: Range<u32>,
    flaky: Option<FlakyCore1>,
    armed: bool,
    faulted: bool,
    jobs: u32,
    pub job_steps: u64,
    idle_steps: u64,
}

fn is_fpu(soc: &mut Soc, pc: u32) -> bool {
    let op = [soc.read8(pc), soc.read8(pc + 1)];
    op == FPU_PREFIX || op == FPU_PREFIX_WIDE
}

impl Core1 {
    pub fn in_job(&self) -> bool {
        self.cpu
            .as_ref()
            .is_some_and(|c| !self.faulted && !self.mailbox_loop.contains(&c.pc))
    }

    pub fn reset(&mut self) {
        self.cpu = None;
        self.faulted = false;
    }

    pub fn cpu(&mut self) -> &mut Option<Cpu> {
        &mut self.cpu
    }

    pub fn drop_if_held(&mut self, soc: &mut Soc) {
        if self.cpu.is_some() && core1_held(soc) {
            self.cpu = None;
        }
    }

    fn arm(&mut self) {
        if self.armed {
            return;
        }
        let Some((rate, seed)) = self.fate.flaky else {
            return;
        };
        if self.fate.flaky_after_jobs.is_none_or(|n| self.jobs >= n) {
            self.armed = true;
            self.flaky = Some(FlakyCore1::new(rate, seed));
        }
    }

    pub fn step(&mut self, soc: &mut Soc) {
        if self.cpu.is_none() {
            if !soc.cpu1_release {
                return;
            }
            self.cpu = released_cpu1(soc, RAM_SIZE);
        }
        self.arm();
        let in_job = self.in_job();
        if self.faulted
            || core1_held(soc)
            || (self.idle_steps >= POLLING_STEPS
                && self.fate.stalls_after_jobs.is_some_and(|n| self.jobs >= n))
        {
            return;
        }
        let Some(c1) = self.cpu.as_mut() else { return };
        if in_job && self.fate.faults_at_fpu && is_fpu(soc, c1.pc) {
            self.faulted = true;
            soc.guard_trip(DBG_C1_PC_LIMIT);
            return;
        }
        let exposed = match self.fate.misreads {
            Misreads::Anywhere => true,
            Misreads::InJobs => in_job,
            Misreads::InTheLoop => !in_job,
        };
        soc.flaky_core1 = if exposed { self.flaky.take() } else { None };
        let mut bus = Core1Bus {
            soc: &mut *soc,
            budget: u64::MAX,
            until: None,
            spent: 0,
        };
        let stepped = c1.step(&mut bus);
        if let Some(f) = soc.flaky_core1.take() {
            self.flaky = Some(f);
        }
        if stepped.is_err() {
            self.faulted = true;
            return;
        }
        if in_job {
            self.job_steps += 1;
            self.idle_steps = 0;
        } else if self.in_job() {
            self.jobs += 1;
        } else {
            self.idle_steps += 1;
        }
    }
}

pub struct Boot {
    pub words: [u32; RESULT_WORDS],
    pub samples: Vec<i16>,
}

#[derive(Clone, Copy, Default)]
pub struct Block {
    pub core0: u64,
    pub hand_over: u64,
    pub spin: u64,
    pub core1_job: u64,
    pub core1_mailbox: u64,
}

pub struct DualRun {
    pub boots: Vec<Boot>,
    pub blocks: Vec<Block>,
    pub core1_held_at_resets: Vec<bool>,
    pub core1_steps: u64,
    pub core1_loads: u64,
    pub core1_misreads: u64,
    pub core1_misreads_in_job: u64,
    pub core1_jobs_entered: u32,
}

struct Image {
    bin: Vec<u8>,
    chip_a: u32,
    render: u32,
    mark: u32,
    mailbox_loop: Range<u32>,
    dispatch: Range<u32>,
    hand_over: Vec<Range<u32>>,
}

fn load(build: &Path, name: &str) -> Image {
    let dir = build.join("bench/fm1");
    let elf_path = dir.join(format!("{name}.elf"));
    let elf = read(&elf_path, "bench ELF");
    let bin = read(&dir.join(format!("{name}.bin")), "bench image");
    let file = object::File::parse(&*elf)
        .unwrap_or_else(|e| panic!("parse ELF {}: {e}", elf_path.display()));
    let range = |s: &object::Symbol| s.address() as u32..(s.address() + s.size()) as u32;
    let find = |name: &str| {
        file.symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("{name} symbol"))
    };
    let on_core0 = |n: &str| {
        (n.starts_with("fm1_cpu1_") || n.starts_with("chip_b_"))
            && !matches!(n, "fm1_cpu1_main" | "fm1_cpu1_entry" | "fm1_cpu1_start")
    };
    Image {
        chip_a: find("chips").address() as u32,
        render: find("sid_render").address() as u32,
        mark: find("bench_mark").address() as u32,
        mailbox_loop: range(&find("fm1_cpu1_main")),
        dispatch: range(&find("chip_b_hand_over")),
        hand_over: file
            .symbols()
            .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
            .filter(|s| s.name().is_ok_and(on_core0))
            .map(|s| range(&s))
            .collect(),
        bin,
    }
}

fn word(soc: &Soc, addr: u32) -> u32 {
    let o = addr - RAM_BASE;
    u32::from_le_bytes(std::array::from_fn(|i| soc.ram.read8(o + i as u32)))
}

fn core1_held(soc: &mut Soc) -> bool {
    let con = soc.read32(C1_CON);
    con & C1_CON_HOLD != 0 || con & C1_CON_RUN == 0
}

fn boot_result(soc: &Soc) -> Boot {
    let words = std::array::from_fn(|i| word(soc, RESULT_ADDR + 4 * i as u32));
    let base = RESULT_ADDR + SAMPLES_OFFSET - RAM_BASE;
    let samples = (0..words[1])
        .map(|i| i16::from_le_bytes([soc.ram.read8(base + 2 * i), soc.ram.read8(base + 2 * i + 1)]))
        .collect();
    Boot { words, samples }
}

pub fn write_input(soc: &mut Soc, word: usize, value: u32) {
    soc.ram
        .write32(RESULT_ADDR + 4 * word as u32 - RAM_BASE, value);
}

pub fn run_two_cores(
    build: &Path,
    name: &str,
    boots: usize,
    fate: Core1Fate,
    setup: impl FnOnce(&mut Soc),
) -> DualRun {
    let image = load(build, name);
    let mut soc = Soc::new(RAM_SIZE, image.bin.clone());
    setup(&mut soc);
    soc.flaky_core1 = fate.flaky.map(|(rate, seed)| FlakyCore1::new(rate, seed));
    let mut cpu = Cpu::new(XIP_BASE);
    soc.take_boot(&cpu);
    let mut core1: Option<Cpu> = None;
    let mut run = DualRun {
        boots: Vec::new(),
        blocks: Vec::new(),
        core1_held_at_resets: Vec::new(),
        core1_steps: 0,
        core1_loads: 0,
        core1_misreads: 0,
        core1_misreads_in_job: 0,
        core1_jobs_entered: 0,
    };
    let mut mailbox_misread_due = fate.misreads_mailbox_after_jobs;
    let mut marks = 0;
    let mut block: Option<Block> = None;
    let mut magic_seen = false;
    let mut steps = 0u64;
    loop {
        let pc = cpu.pc;
        if pc == image.mark {
            marks += 1;
            if marks % 3 == 0 {
                run.blocks.extend(block.take());
            } else if marks % 3 == 1 {
                run.blocks.clear();
            }
        }
        if pc == image.render && cpu.regs[0] == image.chip_a && marks % 3 == 2 {
            run.blocks.extend(block.replace(Block::default()));
        }
        let c1_in_job = core1
            .as_ref()
            .is_some_and(|c1| !image.mailbox_loop.contains(&c1.pc));
        if let Some(b) = block.as_mut() {
            b.core0 += 1;
            if image.hand_over.iter().any(|r| r.contains(&pc)) {
                b.hand_over += 1;
                if c1_in_job {
                    b.spin += 1;
                }
            }
        }
        if soc.periph.tick() {
            soc.advance(0);
        }
        step(&mut cpu, &mut soc);
        steps += 1;
        assert!(
            steps < MAX_STEPS,
            "the bench did not finish (pc {:#010x})",
            cpu.pc
        );

        let done = word(&soc, RESULT_ADDR) == RESULT_MAGIC;
        if done && !magic_seen {
            run.boots.push(boot_result(&soc));
            if run.boots.len() == boots {
                break;
            }
        }
        magic_seen = done;

        if soc.reset_due() {
            let held = core1_held(&mut soc);
            run.core1_held_at_resets.push(held);
            soc.chip_reset(&mut cpu);
            core1 = None;
            continue;
        }
        if core1.is_none() {
            core1 = released_cpu1(&mut soc, RAM_SIZE);
        }
        let Some(c1) = core1.as_mut() else { continue };
        if core1_held(&mut soc) || fate.stalls_after.is_some_and(|n| run.core1_steps >= n) {
            continue;
        }
        let misreads = soc.flaky_core1.as_ref().map_or(0, |f| f.misreads);
        let in_job = !image.mailbox_loop.contains(&c1.pc);
        let misread_now = !in_job
            && image.dispatch.contains(&cpu.pc)
            && mailbox_misread_due.is_some_and(|j| run.core1_jobs_entered >= j);
        if misread_now {
            soc.flaky_core1 = Some(FlakyCore1::new(1.0, 0));
        }
        let mut bus = Core1Bus {
            soc: &mut soc,
            budget: u64::MAX,
            until: None,
            spent: 0,
        };
        let stepped = c1.step(&mut bus);
        if misread_now && soc.flaky_core1.take().is_some_and(|f| f.misreads > 0) {
            mailbox_misread_due = None;
            run.core1_misreads += 1;
        }
        if stepped.is_err() {
            core1 = None;
            continue;
        }
        run.core1_steps += 1;
        if !in_job && !image.mailbox_loop.contains(&c1.pc) {
            run.core1_jobs_entered += 1;
        }
        if let Some(b) = block.as_mut() {
            if in_job {
                b.core1_job += 1;
            } else {
                b.core1_mailbox += 1;
            }
        }
        if soc.flaky_core1.as_ref().map_or(0, |f| f.misreads) > misreads && in_job {
            run.core1_misreads_in_job += 1;
        }
    }
    if let Some(f) = &soc.flaky_core1 {
        run.core1_loads = f.loads;
        run.core1_misreads += f.misreads;
    }
    run
}
