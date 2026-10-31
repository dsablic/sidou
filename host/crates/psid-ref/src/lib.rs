// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::{c_int, c_void};

use sidou_sys::{psid_header, psid_parse, PSID_OK};

pub mod compare;
pub mod sidou;

pub const ORACLE_CLOCK: u32 = 985_248;
pub const ORACLE_FRAME: u32 = 19_656;
pub const ORACLE_INIT_CAP: u32 = 5 * ORACLE_CLOCK;
pub const ORACLE_PLAY_CAP: u32 = 2 * ORACLE_FRAME;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    pub cycle: u32,
    pub reg: u8,
    pub value: u8,
    pub chip: u8,
    pub addr: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    pub cycle: u32,
    pub addr: u16,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoWrite {
    pub cycle: u32,
    pub addr: u16,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Returned,
    Cut,
    Stopped(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub start: u32,
    pub cycles: u32,
    pub bank: u8,
    pub end: End,
    pub writes: Vec<Write>,
    pub reads: Vec<Read>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub calls: Vec<Call>,
}

pub fn header(file: &[u8]) -> Result<psid_header, i32> {
    let mut h = psid_header::default();
    let r = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut h) };
    if r == PSID_OK as i32 {
        Ok(h)
    } else {
        Err(r)
    }
}

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

pub fn run(file: &[u8], song: u32, seconds: u32) -> Result<Run, String> {
    let h = header(file).map_err(|code| format!("SIDOU's parser refuses it ({code})"))?;
    if song < 1 || song > u32::from(h.songs) {
        return Err(format!("song {song} of {}", h.songs));
    }
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
