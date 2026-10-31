// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::cpu::{documented, Access, Cpu6502, Mos6502, Ram, Regs, Sidou6502, SED};
use crate::{pack, Options, Packed, PLAYER_ZPGHOSTREGS};

pub const SENTINEL: u16 = 0xfff0;
pub const STEP_LIMIT: u32 = 200_000;

pub const SID_WINDOW: std::ops::RangeInclusive<u16> = 0xd400..=0xd7ff;
pub const RETURN_SLOT: std::ops::RangeInclusive<u16> = 0x01fe..=0x01ff;

pub const GHOST_REVERSE: [u8; 25] = [
    0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, 0x10, 0x0f, 0x0e, 0x0d, 0x0c, 0x0b, 0x0a, 0x09,
    0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x00,
];
pub const GHOST_ALT: [u8; 25] = [
    0x15, 0x16, 0x17, 0x18, 0x04, 0x00, 0x01, 0x02, 0x03, 0x05, 0x06, 0x0b, 0x07, 0x08, 0x09, 0x0a,
    0x0c, 0x0d, 0x12, 0x0e, 0x0f, 0x10, 0x11, 0x13, 0x14,
];

#[derive(Clone, Copy, Debug)]
pub struct Ghost {
    pub base: u16,
    pub order: &'static [u8; 25],
}

pub type Writes = Vec<(u8, u8)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub calls: Vec<Writes>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TraceError {
    Pack(String),
    Placement(String),
    Undocumented {
        opcode: u8,
        address: u16,
        frame: Option<usize>,
    },
    Decimal {
        address: u16,
        frame: Option<usize>,
    },
    Undecodable {
        opcode: u8,
        address: u16,
        frame: Option<usize>,
    },
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
            TraceError::Placement(e) => write!(f, "placement: {e}"),
            TraceError::Undecodable {
                opcode,
                address,
                frame,
            } => write!(
                f,
                "the core cannot decode ${opcode:02X} at ${address:04X}, {}",
                at(frame)
            ),
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

#[derive(Clone, Copy, Debug)]
pub struct Setup {
    pub load: u16,
    pub init: u16,
    pub play: u16,
    pub calls_per_frame: u32,
    pub ghost: Option<Ghost>,
    pub sid: u16,
    pub trap_sed: bool,
}

impl Setup {
    pub fn of(packed: &Packed, o: &Options) -> Setup {
        Setup {
            load: packed.load,
            init: packed.init(),
            play: packed.play(),
            calls_per_frame: o.multiplier.max(1),
            ghost: (o.player_version & PLAYER_ZPGHOSTREGS != 0).then_some(Ghost {
                base: (o.zeropage & 0xff) as u16,
                order: if o.hr_adsr & 0xffff >= 0xf000 {
                    &GHOST_ALT
                } else {
                    &GHOST_REVERSE
                },
            }),
            sid: o.sid_address as u16,
            trap_sed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub init: Writes,
    pub frames: Vec<Frame>,
}

struct Call {
    writes: Writes,
    cycles: Option<u64>,
    regs: Regs,
}

fn call<C: Cpu6502>(
    cpu: &mut C,
    setup: &Setup,
    routine: u16,
    a: u8,
    frame: Option<usize>,
) -> Result<Call, TraceError> {
    let start = cpu.cycles();
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
        if steps > STEP_LIMIT {
            return Err(TraceError::Hang { address: pc, frame });
        }
        if !cpu.step() {
            return Err(TraceError::Undecodable {
                opcode: op,
                address: pc,
                frame,
            });
        }
    }
    if let Some(g) = setup.ghost {
        for &r in g.order {
            let v = cpu.ram().bytes[g.base.wrapping_add(r as u16) as usize];
            cpu.ram_mut().set_sid(setup.sid.wrapping_add(r as u16), v);
        }
    }
    let cycles = cpu.cycles().zip(start).map(|(end, start)| end - start);
    let regs = cpu.regs();
    let ram = cpu.ram_mut();
    ram.logging = false;
    Ok(Call {
        writes: std::mem::take(&mut ram.log),
        cycles,
        regs,
    })
}

impl Ram {
    pub fn set_sid(&mut self, address: u16, value: u8) {
        use mos6502::memory::Bus;
        self.set_byte(address, value);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timing {
    pub cycles: Vec<Option<u64>>,
    pub regs: Vec<Regs>,
}

pub fn run<C: Cpu6502>(
    cpu: &mut C,
    setup: &Setup,
    subtune: u8,
    frames: usize,
) -> Result<Trace, TraceError> {
    run_timed(cpu, setup, subtune, frames).map(|(t, _)| t)
}

pub fn run_timed<C: Cpu6502>(
    cpu: &mut C,
    setup: &Setup,
    subtune: u8,
    frames: usize,
) -> Result<(Trace, Timing), TraceError> {
    let mut timing = Timing::default();
    let mut note = |c: &Call| {
        timing.cycles.push(c.cycles);
        timing.regs.push(c.regs);
    };
    let first = call(cpu, setup, setup.init, subtune, None)?;
    note(&first);
    let init = first.writes;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut calls = Vec::with_capacity(setup.calls_per_frame as usize);
        for _ in 0..setup.calls_per_frame {
            let c = call(cpu, setup, setup.play, 0, Some(f))?;
            note(&c);
            calls.push(c.writes);
        }
        out.push(Frame { calls });
    }
    Ok((Trace { init, frames: out }, timing))
}

pub fn check_placement(load: u16, len: usize, sid: u16) -> Result<(), String> {
    let start = load as u32;
    let end = start + len as u32;
    if len == 0 {
        return Ok(());
    }
    if end > 0x10000 {
        return Err(format!("{len} bytes at ${load:04X} run past $FFFF"));
    }
    let over = |lo: u16, hi: u16| start <= hi as u32 && end > lo as u32;
    let sid_regs = (sid, sid.saturating_add(0x18));
    for (what, lo, hi) in [
        ("the SID's window", *SID_WINDOW.start(), *SID_WINDOW.end()),
        ("the SID's registers", sid_regs.0, sid_regs.1),
        (
            "the return address",
            *RETURN_SLOT.start(),
            *RETURN_SLOT.end(),
        ),
        ("the harness's sentinel", SENTINEL, SENTINEL),
    ] {
        if over(lo, hi) {
            return Err(format!(
                "code at ${load:04X}-${:04X} overlaps {what} (${lo:04X}-${hi:04X})",
                end - 1
            ));
        }
    }
    Ok(())
}

pub fn load(packed: &Packed, sid: u16) -> Result<Mos6502, TraceError> {
    check_placement(packed.load, packed.code.len(), sid).map_err(TraceError::Placement)?;
    let mut ram = Ram::new(sid);
    ram.load(packed.load, &packed.code)
        .map_err(TraceError::Placement)?;
    Ok(Mos6502::new(ram))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Crossed {
    pub frames: usize,
    pub writes: usize,
    pub calls: usize,
    pub timed_calls: usize,
}

pub fn cross_check(
    sng: &[u8],
    subtune: u8,
    frames: usize,
    o: &Options,
    accesses: &[Access],
) -> Result<Crossed, String> {
    let packed = pack(sng, o).map_err(|e| format!("pack: {e}"))?;
    let setup = Setup::of(&packed, o);
    let mut reference = load(&packed, setup.sid).map_err(|e| e.to_string())?;
    let (want, want_t) =
        run_timed(&mut reference, &setup, subtune, frames).map_err(|e| e.to_string())?;
    let mut crossed = Crossed {
        frames,
        ..Crossed::default()
    };
    crossed.writes = want
        .frames
        .iter()
        .flat_map(|f| &f.calls)
        .map(Vec::len)
        .sum();
    crossed.calls = want_t.regs.len();
    crossed.timed_calls = want_t.cycles.iter().flatten().count();
    for &access in accesses {
        let mut ram = Ram::new(setup.sid);
        ram.load(packed.load, &packed.code)?;
        let mut ours = Sidou6502::new(ram, access);
        let (got, got_t) = run_timed(&mut ours, &setup, subtune, frames)
            .map_err(|e| format!("{access:?}: {e}"))?;
        if got.init != want.init {
            return Err(format!(
                "{access:?}: init's writes differ: {:?} vs {:?}",
                got.init, want.init
            ));
        }
        for (f, (g, w)) in got.frames.iter().zip(&want.frames).enumerate() {
            for (k, (g, w)) in g.calls.iter().zip(&w.calls).enumerate() {
                if g != w {
                    let at = g
                        .iter()
                        .zip(w)
                        .position(|(a, b)| a != b)
                        .unwrap_or(g.len().min(w.len()));
                    return Err(format!(
                        "{access:?}: frame {f} call {k}: writes differ at #{at}: {:?} vs {:?} (of {} and {})",
                        g.get(at), w.get(at), g.len(), w.len()
                    ));
                }
            }
        }
        for (i, (g, w)) in got_t.cycles.iter().zip(&want_t.cycles).enumerate() {
            if g != w {
                return Err(format!(
                    "{access:?}: call {i} (0 is init): {g:?} cycles vs {w:?}"
                ));
            }
        }
        let mask = |r: Regs| Regs { p: r.p & 0xcf, ..r };
        for (i, (g, w)) in got_t.regs.iter().zip(&want_t.regs).enumerate() {
            if mask(*g) != mask(*w) {
                return Err(format!(
                    "{access:?}: call {i} (0 is init) returned with {g:?} vs {w:?}"
                ));
            }
        }
        let (a, b) = (&ours.ram().bytes, &reference.ram().bytes);
        if let Some(at) = (0..0x10000).find(|&i| i != 1 && a[i] != b[i]) {
            return Err(format!(
                "{access:?}: memory differs at ${at:04X}: {:#04X} vs {:#04X}",
                a[at], b[at]
            ));
        }
    }
    Ok(crossed)
}

pub fn trace(sng: &[u8], subtune: u8, frames: usize, o: &Options) -> Result<Trace, TraceError> {
    let packed = pack(sng, o).map_err(TraceError::Pack)?;
    let setup = Setup::of(&packed, o);
    let mut cpu = load(&packed, setup.sid)?;
    run(&mut cpu, &setup, subtune, frames)
}
