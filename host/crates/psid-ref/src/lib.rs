// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The oracle for SIDOU's PSID environment (M3 Task 7; `TESTING.md`, "The PSID oracle"): FM-1
//! SID's player (`psid.c`, BSD-2-Clause, in a test-only derived copy, `c/psid_oracle.c`) on
//! floooh/chips' `m6502.h` (zlib), and SIDOU's own environment (`core/psid/psid_env.c` on
//! `core/cpu6502`, with the voice shadows) run on the same tune, each call recorded the same
//! way; [`compare`] holds them side by side, call by call, with the expected differences.
//!
//! - [`run`]: the oracle, scheduled as FM-1 SID's `psidlog.c` schedules it.
//! - [`sidou::run`]: SIDOU's side, as the player will run it (the period rule, the clock).
//! - [`compare::compare`]: by call index, the writes equal, and the first difference one of
//!   the [`compare::Rule`]s; [`compare::check_timing`]: SIDOU's call times against the period
//!   rule (E8), which the oracle's are not used for.
//!
//! Test-only: nothing here reaches the firmware.

use std::os::raw::{c_int, c_void};

use sidou_sys::{psid_header, psid_parse, PSID_OK};

pub mod compare;
pub mod sidou;

/// FM-1 SID's clock and frame (`psid.h`): the oracle plays every tune at PAL.
pub const ORACLE_CLOCK: u32 = 985_248;
pub const ORACLE_FRAME: u32 = 19_656;
/// `psid.c`'s caps: init runs at most 5 s, a play call at most two frames.
pub const ORACLE_INIT_CAP: u32 = 5 * ORACLE_CLOCK;
pub const ORACLE_PLAY_CAP: u32 = 2 * ORACLE_FRAME;

/// A SID write: its cycle in the call, the register (0-31) and the value. SIDOU's side also
/// gives the chip (0 A, 1 B) and the address the 6502 wrote; the oracle's are 0 (`psid.c`
/// reports the register only, and folds every SID into one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    pub cycle: u32,
    pub reg: u8,
    pub value: u8,
    pub chip: u8,
    pub addr: u16,
}

/// A read of the I/O page (`$D000-$DFFF` while `$01` maps it in): its cycle in the call, the
/// address and the value the 6502 got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    pub cycle: u32,
    pub addr: u16,
    pub value: u8,
}

/// A write to the I/O page, any register (SIDOU's side only: the CIA latch for the period
/// rule, a third SID's dropped writes): its cycle in the call, the address and the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoWrite {
    pub cycle: u32,
    pub addr: u16,
    pub value: u8,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// It returned (to the sentinel, or a KERNAL interrupt exit).
    Returned,
    /// The oracle stopped it at its cap ([`ORACLE_INIT_CAP`], [`ORACLE_PLAY_CAP`]).
    Cut,
    /// SIDOU stopped the tune: `PSID_RUN_INIT_TOO_LONG`, `_PLAY_HUNG` or `_JAM`.
    Stopped(u32),
}

/// One init or play call. Call 0 is init.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// Its first cycle, on the tune's running count (init's is 0).
    pub start: u32,
    /// The cycles it ran, to its return or to where it was cut or stopped.
    pub cycles: u32,
    /// `$01` as it started.
    pub bank: u8,
    pub end: End,
    pub writes: Vec<Write>,
    pub reads: Vec<Read>,
    /// SIDOU's side only.
    pub io_writes: Vec<IoWrite>,
}

impl Call {
    fn new(start: u32, bank: u8) -> Call {
        Call {
            start,
            cycles: 0,
            bank,
            end: End::Returned,
            writes: Vec::new(),
            reads: Vec::new(),
            io_writes: Vec::new(),
        }
    }
}

/// A tune's calls for one song, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub calls: Vec<Call>,
}

/// The file's header as SIDOU's parser reads it, or its refusal code. Every file goes through
/// it before either side runs it: `psid.c` reads past its buffer on headers SIDOU refuses
/// (`docs/upstream-reports.md`, 1), so those are SIDOU's alone.
pub fn header(file: &[u8]) -> Result<psid_header, i32> {
    let mut h = psid_header::default();
    let r = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut h) };
    if r == PSID_OK as i32 {
        Ok(h)
    } else {
        Err(r)
    }
}

// The derived copy's interface (`psid.h` and the lines `c/psid_oracle.c` adds).
#[repr(C)]
struct Psid {
    _opaque: [u8; 0],
}
type WriteFn = unsafe extern "C" fn(user: *mut c_void, cycle: u32, reg: u32, val: u32);
type ReadFn = unsafe extern "C" fn(user: *mut c_void, cycle: u32, addr: u32, val: u32);

unsafe extern "C" {
    fn psid_size() -> u32;
    fn psid_load(p: *mut Psid, data: *const u8, len: u32, wr: WriteFn, user: *mut c_void) -> c_int;
    fn psid_init(p: *mut Psid, song: u32) -> u32;
    fn psid_period(p: *const Psid) -> u32;
    fn psid_play(p: *mut Psid) -> u32;
    fn psid_set_read_log(p: *mut Psid, rd: ReadFn, user: *mut c_void);
    fn psid_peek(p: *const Psid, ad: u16) -> u8;
}

/// The oracle's log: the call being run.
struct Log {
    call: Call,
}

unsafe extern "C" fn log_write(user: *mut c_void, cycle: u32, reg: u32, val: u32) {
    let log = unsafe { &mut *(user as *mut Log) };
    log.call.writes.push(Write {
        cycle,
        reg: reg as u8,
        value: val as u8,
        chip: 0,
        addr: 0,
    });
}

unsafe extern "C" fn log_read(user: *mut c_void, cycle: u32, addr: u32, val: u32) {
    let log = unsafe { &mut *(user as *mut Log) };
    log.call.reads.push(Read {
        cycle,
        addr: addr as u16,
        value: val as u8,
    });
}

/// The oracle: `psid.c` runs song `song` (1-based) for `seconds` of PAL time, scheduled as
/// `psidlog.c` does (init at 0; the first play call at the first frame boundary after init's
/// return, then one every `psid_period`, the next due time counted from the last one's, not
/// from its return). A file SIDOU's parser refuses is refused here, before `psid.c` sees it.
pub fn run(file: &[u8], song: u32, seconds: u32) -> Result<Run, String> {
    let h = header(file).map_err(|code| format!("SIDOU's parser refuses it ({code})"))?;
    if song < 1 || song > u32::from(h.songs) {
        return Err(format!("song {song} of {}", h.songs));
    }
    // psid_t holds the 64 KiB and m6502_t: 8-byte aligned storage.
    let mut store = vec![0u64; (unsafe { psid_size() } as usize).div_ceil(8)];
    let p = store.as_mut_ptr() as *mut Psid;
    let mut log = Box::new(Log {
        call: Call::new(0, 0),
    });
    let user = (&mut *log as *mut Log).cast::<c_void>();
    let total = seconds * ORACLE_CLOCK;
    let mut out = Run::default();
    unsafe {
        if psid_load(p, file.as_ptr(), file.len() as u32, log_write, user) != 0 {
            return Err("psid.c refuses it (not PSID)".into());
        }
        psid_set_read_log(p, log_read, user);
        log.call = Call::new(0, psid_peek(p, 1));
        let cycles = psid_init(p, song);
        out.calls.push(finish(&mut log, cycles, ORACLE_INIT_CAP));
        let mut t = (cycles / ORACLE_FRAME + 1) * ORACLE_FRAME;
        while t < total {
            log.call = Call::new(t, psid_peek(p, 1));
            let cycles = psid_play(p);
            out.calls.push(finish(&mut log, cycles, ORACLE_PLAY_CAP));
            t += psid_period(p);
        }
    }
    Ok(out)
}

fn finish(log: &mut Log, cycles: u32, cap: u32) -> Call {
    let mut c = std::mem::replace(&mut log.call, Call::new(0, 0));
    c.cycles = cycles;
    c.end = if cycles >= cap {
        End::Cut
    } else {
        End::Returned
    };
    c
}
