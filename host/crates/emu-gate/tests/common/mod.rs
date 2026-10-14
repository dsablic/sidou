// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared by the emulator tests: where the fm1 build is, and fm1-emu workarounds.

#![allow(dead_code)] // each test binary uses its own subset

use std::path::{Path, PathBuf};

use fm1_core::{Bus, Cpu};
use fm1_soc::Soc;

/// The fm1 build directory, made absolute, or None to skip the emulator tests.
pub fn build_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SIDOU_FM1_BUILD").filter(|d| !d.is_empty())?;
    Some(std::path::absolute(&dir).expect("resolve SIDOU_FM1_BUILD"))
}

pub fn skip_message(what: &str) {
    println!(
        "skipping {what}: set SIDOU_FM1_BUILD to the fm1 build directory \
         (e.g. $PWD/../build/fm1 from host/) after building the fm1 preset"
    );
}

pub fn read(path: &Path, what: &str) -> Vec<u8> {
    std::fs::read(path)
        .unwrap_or_else(|e| panic!("read {what} {}: {e}; build the fm1 preset", path.display()))
}

/// fm1-core at the pinned rev decodes the decrement-before block load `{rA, rB, ..} = [-rN]`
/// (`4x eb`, fm1-isa class `r_4_mov_r_4`) but has no semantics for it, and faults. clang emits
/// it in `sid_cutoff_hz` to load a curve point. Run it here with the convention fm1-core uses
/// for the `[rN+]` block moves: base in bits 0-3, register mask in bits 16-31, lowest register
/// at the lowest address; `-rN` starts 4 bytes per register below the base and does not write
/// it back (`--rN` is a separate class). The emulator gate's host comparison checks the result.
/// Returns false, doing nothing, for any other instruction.
pub fn step_block_load_dec(cpu: &mut Cpu, soc: &mut Soc) -> bool {
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

/// One instruction: the block load above, or fm1-core's step. Panics on a fault.
pub fn step(cpu: &mut Cpu, soc: &mut Soc) -> bool {
    let pc = cpu.pc;
    if step_block_load_dec(cpu, soc) {
        return true;
    }
    cpu.step(soc)
        .unwrap_or_else(|e| panic!("cpu fault at {pc:#010x}: {e:?}"));
    false
}
