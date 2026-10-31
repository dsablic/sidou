// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use mos6502::cpu::CPU;
use mos6502::instruction::Nmos6502;
use mos6502::memory::Bus;
use mos6502::registers::{StackPointer, Status};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regs {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub p: u8,
    pub pc: u16,
}

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

    pub fn load(&mut self, at: u16, data: &[u8]) -> Result<(), String> {
        let at = at as usize;
        if data.len() > 0x10000 - at {
            return Err(format!("{} bytes at ${at:04X} run past $FFFF", data.len()));
        }
        self.bytes[at..at + data.len()].copy_from_slice(data);
        Ok(())
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

pub trait Cpu6502 {
    fn regs(&self) -> Regs;
    fn set_regs(&mut self, r: Regs);
    fn step(&mut self) -> bool;
    fn ram(&self) -> &Ram;
    fn ram_mut(&mut self) -> &mut Ram;
    fn cycles(&self) -> Option<u64>;
}

pub struct Mos6502 {
    cpu: CPU<Ram, Nmos6502>,
    overcount: u64,
}

pub const RMW_ABSOLUTE_X: [u8; 6] = [0x1e, 0x3e, 0x5e, 0x7e, 0xde, 0xfe];

impl Mos6502 {
    pub fn new(ram: Ram) -> Mos6502 {
        Mos6502 {
            cpu: CPU::new(ram, Nmos6502),
            overcount: 0,
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
        let opcode = self.cpu.memory.bytes[self.cpu.registers.program_counter as usize];
        let before = self.cpu.cycles;
        match self.cpu.fetch_next_and_decode() {
            Some(d) => {
                self.cpu.execute_instruction(d);
                if RMW_ABSOLUTE_X.contains(&opcode) && self.cpu.cycles - before == 8 {
                    self.overcount += 1;
                }
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

    fn cycles(&self) -> Option<u64> {
        Some(self.cpu.cycles - self.overcount)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    HookAll,
    Device,
    HookInline,
}

pub struct Sidou6502 {
    c: sidou_sys::cpu6502,
    ram: Ram,
    access: Access,
}

unsafe extern "C" fn hook_rd(user: *mut c_void, addr: u32, _cycle: u32) -> u8 {
    // SAFETY: `user` is the `Ram` of the `Sidou6502` being stepped (set before each run).
    let ram = unsafe { &*(user as *const Ram) };
    ram.bytes[addr as usize]
}

unsafe extern "C" fn hook_wr(user: *mut c_void, addr: u32, value: u32, _cycle: u32) {
    // SAFETY: as `hook_rd`.
    let ram = unsafe { &mut *(user as *mut Ram) };
    ram.set_sid(addr as u16, value as u8);
}

impl Sidou6502 {
    pub fn new(mut ram: Ram, access: Access) -> Sidou6502 {
        assert!(
            access != Access::Device || (0xd000..=0xd7ff).contains(&ram.sid),
            "the device build reaches the SID only in the I/O page"
        );
        ram.bytes[1] = 0x35;
        let c = sidou_sys::cpu6502 {
            rd: Some(hook_rd),
            wr: Some(hook_wr),
            io: u8::from(access == Access::Device),
            ..sidou_sys::cpu6502::default()
        };
        Sidou6502 { c, ram, access }
    }
}

impl Cpu6502 for Sidou6502 {
    fn regs(&self) -> Regs {
        Regs {
            a: self.c.a,
            x: self.c.x,
            y: self.c.y,
            sp: self.c.s,
            p: self.c.p,
            pc: self.c.pc,
        }
    }

    fn set_regs(&mut self, r: Regs) {
        self.c.a = r.a;
        self.c.x = r.x;
        self.c.y = r.y;
        self.c.s = r.sp;
        self.c.p = r.p;
        self.c.pc = r.pc;
        self.c.state = sidou_sys::CPU6502_RUN as u8;
    }

    fn step(&mut self) -> bool {
        self.c.mem = self.ram.bytes.as_mut_ptr();
        self.c.user = (&mut self.ram as *mut Ram).cast();
        let limit = self.c.cycles.wrapping_add(1);
        // SAFETY: `mem` is the 64 KiB `Ram` owns, `user` is that `Ram`, both valid for the call.
        let state = unsafe {
            match self.access {
                Access::Device => sidou_sys::cpu6502_run(&mut self.c, limit, 0),
                Access::HookAll => sidou_sys::cpu6502_run_hook_all(&mut self.c, limit, 0),
                Access::HookInline => sidou_sys::cpu6502_run_hook_inline(&mut self.c, limit, 0),
            }
        };
        state != sidou_sys::CPU6502_JAM as i32
    }

    fn ram(&self) -> &Ram {
        &self.ram
    }

    fn ram_mut(&mut self) -> &mut Ram {
        &mut self.ram
    }

    fn cycles(&self) -> Option<u64> {
        Some(u64::from(self.c.cycles))
    }
}

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

pub const SED: u8 = 0xf8;
