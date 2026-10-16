// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A host 6502 behind a small trait, so M3's own 6502 can later run the same traces and be
//! checked against this one. [`Mos6502`] is the `mos6502` crate's NMOS core (0.10.1,
//! BSD-3-Clause) over [`Ram`]: 64 KiB whose writes to the SID's 25 registers are logged in
//! order.

use mos6502::cpu::CPU;
use mos6502::instruction::Nmos6502;
use mos6502::memory::Bus;
use mos6502::registers::{StackPointer, Status};

/// The registers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regs {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    /// NV-BDIZC
    pub p: u8,
    pub pc: u16,
}

/// 64 KiB of RAM; writes to `[sid, sid + 0x18]` are logged as (register, value) while
/// `logging` is on.
pub struct Ram {
    pub bytes: Vec<u8>,
    pub sid: u16,
    pub logging: bool,
    pub log: Vec<(u8, u8)>,
}

impl Ram {
    pub fn new(sid: u16) -> Ram {
        Ram {
            bytes: vec![0; 0x10000],
            sid,
            logging: false,
            log: Vec::new(),
        }
    }

    pub fn load(&mut self, at: u16, data: &[u8]) {
        let at = at as usize;
        let n = data.len().min(0x10000 - at);
        self.bytes[at..at + n].copy_from_slice(&data[..n]);
    }
}

impl Bus for Ram {
    fn get_byte(&mut self, address: u16) -> u8 {
        self.bytes[address as usize]
    }

    fn set_byte(&mut self, address: u16, value: u8) {
        self.bytes[address as usize] = value;
        if self.logging && address.wrapping_sub(self.sid) <= 0x18 {
            self.log.push(((address - self.sid) as u8, value));
        }
    }
}

/// What a harness needs of a 6502.
pub trait Cpu6502 {
    fn regs(&self) -> Regs;
    fn set_regs(&mut self, r: Regs);
    /// Executes one instruction; false if the core cannot decode the opcode at PC.
    fn step(&mut self) -> bool;
    fn ram(&self) -> &Ram;
    fn ram_mut(&mut self) -> &mut Ram;
}

pub struct Mos6502 {
    cpu: CPU<Ram, Nmos6502>,
}

impl Mos6502 {
    pub fn new(ram: Ram) -> Mos6502 {
        Mos6502 {
            cpu: CPU::new(ram, Nmos6502),
        }
    }
}

impl Cpu6502 for Mos6502 {
    fn regs(&self) -> Regs {
        let r = &self.cpu.registers;
        Regs {
            a: r.accumulator,
            x: r.index_x,
            y: r.index_y,
            sp: r.stack_pointer.0,
            p: r.status.bits(),
            pc: r.program_counter,
        }
    }

    fn set_regs(&mut self, v: Regs) {
        let r = &mut self.cpu.registers;
        r.accumulator = v.a;
        r.index_x = v.x;
        r.index_y = v.y;
        r.stack_pointer = StackPointer(v.sp);
        r.status = Status::from_bits_truncate(v.p);
        r.program_counter = v.pc;
    }

    fn step(&mut self) -> bool {
        match self.cpu.fetch_next_and_decode() {
            Some(d) => {
                self.cpu.execute_instruction(d);
                true
            }
            None => false,
        }
    }

    fn ram(&self) -> &Ram {
        &self.cpu.memory
    }

    fn ram_mut(&mut self) -> &mut Ram {
        &mut self.cpu.memory
    }
}

/// The 151 documented NMOS 6502 opcodes (MOS's programming manual); every other opcode is
/// undocumented ("illegal").
pub fn documented(opcode: u8) -> bool {
    const LEGAL: [u8; 151] = [
        0x00, 0x01, 0x05, 0x06, 0x08, 0x09, 0x0a, 0x0d, 0x0e, 0x10, 0x11, 0x15, 0x16, 0x18, 0x19,
        0x1d, 0x1e, 0x20, 0x21, 0x24, 0x25, 0x26, 0x28, 0x29, 0x2a, 0x2c, 0x2d, 0x2e, 0x30, 0x31,
        0x35, 0x36, 0x38, 0x39, 0x3d, 0x3e, 0x40, 0x41, 0x45, 0x46, 0x48, 0x49, 0x4a, 0x4c, 0x4d,
        0x4e, 0x50, 0x51, 0x55, 0x56, 0x58, 0x59, 0x5d, 0x5e, 0x60, 0x61, 0x65, 0x66, 0x68, 0x69,
        0x6a, 0x6c, 0x6d, 0x6e, 0x70, 0x71, 0x75, 0x76, 0x78, 0x79, 0x7d, 0x7e, 0x81, 0x84, 0x85,
        0x86, 0x88, 0x8a, 0x8c, 0x8d, 0x8e, 0x90, 0x91, 0x94, 0x95, 0x96, 0x98, 0x99, 0x9a, 0x9d,
        0xa0, 0xa1, 0xa2, 0xa4, 0xa5, 0xa6, 0xa8, 0xa9, 0xaa, 0xac, 0xad, 0xae, 0xb0, 0xb1, 0xb4,
        0xb5, 0xb6, 0xb8, 0xb9, 0xba, 0xbc, 0xbd, 0xbe, 0xc0, 0xc1, 0xc4, 0xc5, 0xc6, 0xc8, 0xc9,
        0xca, 0xcc, 0xcd, 0xce, 0xd0, 0xd1, 0xd5, 0xd6, 0xd8, 0xd9, 0xdd, 0xde, 0xe0, 0xe1, 0xe4,
        0xe5, 0xe6, 0xe8, 0xe9, 0xea, 0xec, 0xed, 0xee, 0xf0, 0xf1, 0xf5, 0xf6, 0xf8, 0xf9, 0xfd,
        0xfe,
    ];
    LEGAL.binary_search(&opcode).is_ok()
}

/// SED: decimal mode on.
pub const SED: u8 = 0xf8;
