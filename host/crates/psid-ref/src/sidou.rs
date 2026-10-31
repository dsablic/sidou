// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

use sidou_sys::*;

use crate::{header, Call, End, IoWrite, Read, Run, Write};

struct Side {
    env: psid_env,
    mem: [u8; 0x10000],
    header: psid_header,
    chips: [sid; 2],
    shadows: [sid_shadow; 2],
    rd: cpu6502_rd,
    wr: cpu6502_wr,
    addr: u32,
    calls: Vec<Call>,
}

impl Side {
    fn call(&mut self) -> &mut Call {
        self.calls.last_mut().expect("a call")
    }
}

unsafe extern "C" fn rd(user: *mut c_void, addr: u32, cycle: u32) -> u8 {
    let side = user as *mut Side;
    let v = unsafe {
        let env = std::ptr::addr_of_mut!((*side).env).cast::<c_void>();
        ((*side).rd.unwrap())(env, addr, cycle)
    };
    let s = unsafe { &mut *side };
    let start = s.env.call_start;
    s.call().reads.push(Read {
        cycle: cycle.wrapping_sub(start),
        addr: addr as u16,
        value: v,
    });
    v
}

unsafe extern "C" fn wr(user: *mut c_void, addr: u32, value: u32, cycle: u32) {
    let side = user as *mut Side;
    {
        let s = unsafe { &mut *side };
        s.addr = addr;
        let start = s.env.call_start;
        s.call().io_writes.push(IoWrite {
            cycle: cycle.wrapping_sub(start),
            addr: addr as u16,
            value: value as u8,
        });
    }
    unsafe {
        let env = std::ptr::addr_of_mut!((*side).env).cast::<c_void>();
        ((*side).wr.unwrap())(env, addr, value, cycle);
    }
}

unsafe extern "C" fn sink(ctx: *mut c_void, cycle: u32, chip: u32, reg: u32, value: u32) {
    let s = unsafe { &mut *(ctx as *mut Side) };
    let (start, addr) = (s.env.call_start, s.addr as u16);
    s.call().writes.push(Write {
        cycle: cycle.wrapping_sub(start),
        reg: reg as u8,
        value: value as u8,
        chip: chip as u8,
        addr,
    });
}

fn model(code: u8) -> sid_model {
    if code == 2 {
        SID_MODEL_8580
    } else {
        SID_MODEL_6581
    }
}

pub fn run(file: &[u8], song: u32, seconds: u32) -> Result<Run, String> {
    let h = header(file).map_err(|code| format!("SIDOU's parser refuses it ({code})"))?;
    let mut s: Box<Side> = Box::new(Side {
        env: psid_env::default(),
        mem: [0; 0x10000],
        header: h,
        chips: [sid::default(), sid::default()],
        shadows: [sid_shadow::default(), sid_shadow::default()],
        rd: None,
        wr: None,
        addr: 0,
        calls: Vec::new(),
    });
    let p: *mut Side = &mut *s;
    unsafe {
        let sp = &mut *p;
        psid_env_load(&mut sp.env, sp.mem.as_mut_ptr(), &sp.header, file.as_ptr());
        psid_env_set_sink(&mut sp.env, Some(sink), p.cast());
        let clock = sp.env.clock;
        sid_init(&mut sp.chips[0], model(sp.header.model1), clock, 44_100);
        sid_init(&mut sp.chips[1], model(sp.header.model2), clock, 44_100);
        let b = if sp.header.sid2 != 0 {
            &mut sp.shadows[1] as *mut sid_shadow
        } else {
            std::ptr::null_mut()
        };
        psid_env_set_shadows(&mut sp.env, &mut sp.shadows[0], b);
        sp.rd = sp.env.cpu.rd;
        sp.wr = sp.env.cpu.wr;
        sp.env.cpu.rd = Some(rd);
        sp.env.cpu.wr = Some(wr);
        sp.env.cpu.user = p.cast();
        if psid_env_start(&mut sp.env, song) != 0 {
            return Err(format!("song {song} of {}", sp.header.songs));
        }
        sid_shadow_init(&mut sp.shadows[0], &sp.chips[0], 0);
        sid_shadow_init(&mut sp.shadows[1], &sp.chips[1], 0);
    }
    s.calls.push(Call::new(0, s.mem[1]));
    let total = seconds * s.env.clock;
    let mut code = unsafe { psid_env_run(&mut s.env, 1 << 30) } as u32;
    loop {
        let now = s.env.cpu.cycles;
        let start = s.env.call_start;
        let c = s.call();
        c.cycles = now.wrapping_sub(start);
        match code {
            PSID_RUN_INIT_DONE | PSID_RUN_CALL_DONE => {}
            PSID_RUN_INIT_TOO_LONG | PSID_RUN_PLAY_HUNG | PSID_RUN_JAM => {
                c.end = End::Stopped(code);
                break;
            }
            _ => return Err(format!("psid_env_run gave {code} inside a call")),
        }
        let due = s.env.next_due;
        if due.wrapping_sub(total) as i32 >= 0 {
            break;
        }
        let begun = unsafe { psid_env_run(&mut s.env, due.wrapping_sub(now)) } as u32;
        let (start, bank) = (s.env.call_start, s.mem[1]);
        s.calls.push(Call::new(start, bank));
        code = match begun {
            PSID_RUN_MORE => (unsafe { psid_env_run(&mut s.env, 1 << 30) }) as u32,
            PSID_RUN_CALL_DONE => begun,
            _ => return Err(format!("psid_env_run gave {begun} starting a call")),
        };
    }
    Ok(Run {
        calls: std::mem::take(&mut s.calls),
    })
}
