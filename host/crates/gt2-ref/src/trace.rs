// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs a packed GoatTracker player frame by frame and records each call's SID writes
//! (decision record 0007, "Equivalence, defined": a frame's ordered writes to the SID's 25
//! registers).
//!
//! The song is packed with the options, loaded at its address, and called through a
//! return address that ends at [`SENTINEL`]: init once with A the subtune, then play
//! `max(multiplier, 1)` times a frame. Before each instruction the harness checks the
//! opcode: any of the 105 undocumented ones fails the trace with its address and frame, so
//! a player that runs only documented opcodes is proven to by execution. A call that runs
//! past [`STEP_LIMIT`] instructions is a hang.
//!
//! With zero-page ghost registers (`PLAYER_ZPGHOSTREGS`) the player writes no SID register
//! itself; the harness then plays the main program's part, readme 5.1's copy loop
//! (registers `$18` down to 0 from the ghost registers), after each call.

use crate::cpu::{documented, Cpu6502, Mos6502, Ram, Regs, SED};
use crate::{pack, Options, Packed, PLAYER_ZPGHOSTREGS};

/// Where a call returns to: the harness stops there.
pub const SENTINEL: u16 = 0xfff0;
/// Instructions one call may take.
pub const STEP_LIMIT: u32 = 200_000;

/// A call's writes: (register 0-24, value), in order.
pub type Writes = Vec<(u8, u8)>;

/// One frame: the writes of each call of the play routine in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub calls: Vec<Writes>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TraceError {
    Pack(String),
    /// An undocumented opcode about to execute; frame None is the init call.
    Undocumented {
        opcode: u8,
        address: u16,
        frame: Option<usize>,
    },
    /// SED about to execute, when the harness traps it.
    Decimal {
        address: u16,
        frame: Option<usize>,
    },
    /// A call that did not return within STEP_LIMIT instructions.
    Hang {
        address: u16,
        frame: Option<usize>,
    },
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let at = |frame: &Option<usize>| match frame {
            Some(n) => format!("frame {n}"),
            None => "init".to_string(),
        };
        match self {
            TraceError::Pack(e) => write!(f, "pack: {e}"),
            TraceError::Undocumented {
                opcode,
                address,
                frame,
            } => write!(
                f,
                "undocumented opcode ${opcode:02X} at ${address:04X}, {}",
                at(frame)
            ),
            TraceError::Decimal { address, frame } => {
                write!(f, "SED at ${address:04X}, {}", at(frame))
            }
            TraceError::Hang { address, frame } => {
                write!(
                    f,
                    "no return after {STEP_LIMIT} instructions, at ${address:04X}, {}",
                    at(frame)
                )
            }
        }
    }
}

/// How the harness runs a packed player.
#[derive(Clone, Copy, Debug)]
pub struct Setup {
    pub load: u16,
    pub init: u16,
    pub play: u16,
    pub calls_per_frame: u32,
    /// The ghost registers' base, if the main program copies them to the SID.
    pub ghost: Option<u16>,
    pub sid: u16,
    /// Fail on SED (only if the core's decimal mode were not proven, see the crate's tests).
    pub trap_sed: bool,
}

impl Setup {
    pub fn of(packed: &Packed, o: &Options) -> Setup {
        Setup {
            load: packed.load,
            init: packed.init(),
            play: packed.play(),
            calls_per_frame: o.multiplier.max(1),
            ghost: (o.player_version & PLAYER_ZPGHOSTREGS != 0).then_some(o.zeropage as u16),
            sid: o.sid_address as u16,
            trap_sed: false,
        }
    }
}

/// The traced song: init's writes, then each frame's.
#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub init: Writes,
    pub frames: Vec<Frame>,
}

fn call<C: Cpu6502>(
    cpu: &mut C,
    setup: &Setup,
    routine: u16,
    a: u8,
    frame: Option<usize>,
) -> Result<Writes, TraceError> {
    let ret = SENTINEL.wrapping_sub(1);
    let ram = cpu.ram_mut();
    ram.bytes[0x1ff] = (ret >> 8) as u8;
    ram.bytes[0x1fe] = ret as u8;
    ram.log.clear();
    ram.logging = true;
    cpu.set_regs(Regs {
        a,
        x: 0,
        y: 0,
        sp: 0xfd,
        p: 0x24,
        pc: routine,
    });
    let mut steps = 0;
    loop {
        let pc = cpu.regs().pc;
        if pc == SENTINEL {
            break;
        }
        let op = cpu.ram().bytes[pc as usize];
        if !documented(op) {
            return Err(TraceError::Undocumented {
                opcode: op,
                address: pc,
                frame,
            });
        }
        if setup.trap_sed && op == SED {
            return Err(TraceError::Decimal { address: pc, frame });
        }
        steps += 1;
        if steps > STEP_LIMIT || !cpu.step() {
            return Err(TraceError::Hang { address: pc, frame });
        }
    }
    if let Some(base) = setup.ghost {
        // readme 5.1: LDX #$18 / copy: LDA ghostregs,X / STA $D400,X / DEX / BPL copy
        for r in (0..=0x18u16).rev() {
            let v = cpu.ram().bytes[base.wrapping_add(r) as usize];
            cpu.ram_mut().set_sid(setup.sid.wrapping_add(r), v);
        }
    }
    let ram = cpu.ram_mut();
    ram.logging = false;
    Ok(std::mem::take(&mut ram.log))
}

impl Ram {
    /// A store to the SID as the main program makes it (logged as the player's are).
    pub fn set_sid(&mut self, address: u16, value: u8) {
        use mos6502::memory::Bus;
        self.set_byte(address, value);
    }
}

/// Runs a packed player in `cpu`, already holding the code at `setup.load`.
pub fn run<C: Cpu6502>(
    cpu: &mut C,
    setup: &Setup,
    subtune: u8,
    frames: usize,
) -> Result<Trace, TraceError> {
    let init = call(cpu, setup, setup.init, subtune, None)?;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut calls = Vec::with_capacity(setup.calls_per_frame as usize);
        for _ in 0..setup.calls_per_frame {
            calls.push(call(cpu, setup, setup.play, 0, Some(f))?);
        }
        out.push(Frame { calls });
    }
    Ok(Trace { init, frames: out })
}

/// Loads a packed player into a fresh [`Mos6502`].
pub fn load(packed: &Packed, sid: u16) -> Mos6502 {
    let mut ram = Ram::new(sid);
    ram.load(packed.load, &packed.code);
    Mos6502::new(ram)
}

/// Packs `sng` with the options and traces `frames` frames of `subtune`.
pub fn trace(sng: &[u8], subtune: u8, frames: usize, o: &Options) -> Result<Trace, TraceError> {
    let packed = pack(sng, o).map_err(TraceError::Pack)?;
    let setup = Setup::of(&packed, o);
    let mut cpu = load(&packed, setup.sid);
    run(&mut cpu, &setup, subtune, frames)
}
