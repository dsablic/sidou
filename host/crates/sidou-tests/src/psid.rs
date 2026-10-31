// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

use sidou_sys::*;

use crate::font::repo_root;
use crate::psid_fixtures::{Psid, CLOCK_PAL};

pub const BLOCK: usize = PSID_BLOCK as usize;
pub const SAMPLE_RATE: u32 = crate::DEVICE_SAMPLE_RATE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    pub cycle: u32,
    pub chip: u8,
    pub reg: u8,
    pub value: u8,
}

pub fn header(file: &[u8]) -> psid_header {
    let mut h = psid_header::default();
    let r = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut h) };
    assert_eq!(r, PSID_OK as i32, "psid_parse");
    h
}

pub fn model_of(code: u8) -> sid_model {
    if code == 2 {
        SID_MODEL_8580
    } else {
        SID_MODEL_6581
    }
}

pub fn chips_for(h: &psid_header) -> Box<[sid; 2]> {
    let clock = if h.clock == 2 {
        PSID_CLOCK_NTSC
    } else {
        PSID_CLOCK_PAL
    };
    let mut c = Box::new([sid::default(), sid::default()]);
    unsafe {
        sid_init(&mut c[0], model_of(h.model1), clock, SAMPLE_RATE);
        sid_init(&mut c[1], model_of(h.model2), clock, SAMPLE_RATE);
    }
    c
}

unsafe extern "C" fn hook_chip(ctx: *mut c_void, chip: u32) -> *mut sid {
    let chips = ctx as *mut [sid; 2];
    unsafe { &mut (*chips)[chip as usize] }
}

unsafe extern "C" fn hook_mute(ctx: *mut c_void, chip: u32, mask: u32) {
    let chips = ctx as *mut [sid; 2];
    unsafe { sid_set_mute(&mut (*chips)[chip as usize], mask as u8) };
}

unsafe extern "C" fn render_part(s: *mut sid, out: *mut i16, n: u32, _tapped: i32) {
    unsafe { sid_render(s, out, n) };
}

pub struct Player {
    pub ctl: Box<psid_ctl>,
    pub work: Box<psid_work>,
    pub chips: Box<[sid; 2]>,
    _mem: Box<[u8; 0x10000]>,
    _header: Box<psid_header>,
    _file: Box<[u8]>,
    pub log: Vec<Write>,
    log_from: u32,
    logged: u32,
    pub blocks: Vec<BlockStart>,
    pub ahead_max: u32,
}

impl Player {
    pub fn new(file: &[u8]) -> Player {
        let header = Box::new(header(file));
        let chips = chips_for(&header);
        Player::with_chips(file, header, chips)
    }

    pub fn with_chips(file: &[u8], header: Box<psid_header>, chips: Box<[sid; 2]>) -> Player {
        let mut p = Player {
            ctl: Box::<psid_ctl>::default(),
            work: Box::<psid_work>::default(),
            chips,
            _mem: Box::new([0; 0x10000]),
            _header: header,
            _file: file.into(),
            log: Vec::new(),
            log_from: 0,
            logged: 0,
            blocks: Vec::new(),
            ahead_max: 0,
        };
        let hooks = psid_ctl_hooks {
            chip: Some(hook_chip),
            mute: Some(hook_mute),
            ctx: (&mut *p.chips as *mut [sid; 2]).cast(),
        };
        // SAFETY: everything psid_ctl keeps a pointer to is boxed and owned by the player.
        unsafe {
            psid_ctl_init(&mut *p.ctl, &mut *p.work, p._mem.as_mut_ptr(), &hooks);
            assert_eq!(psid_ctl_load(&mut *p.ctl, &*p._header, p._file.as_ptr()), 1);
        }
        p
    }

    pub fn play(&mut self, song: u32) -> bool {
        let ok = unsafe { psid_ctl_play(&mut *self.ctl, song) != 0 };
        if ok {
            self.restart_log();
        }
        ok
    }

    fn restart_log(&mut self) {
        self.log.clear();
        self.log_from = self.ctl.tail;
        self.logged = self.ctl.tail;
        self.blocks.clear();
    }

    pub fn ring_from(&mut self, x: u32) {
        self.ctl.stage = x;
        self.ctl.head = x;
        self.ctl.tail = x;
        self.restart_log();
    }

    pub fn stop(&mut self) -> bool {
        unsafe { psid_ctl_stop(&mut *self.ctl) != 0 }
    }

    pub fn pause(&mut self) -> bool {
        unsafe { psid_ctl_pause(&mut *self.ctl) != 0 }
    }

    pub fn resume(&mut self) -> bool {
        unsafe { psid_ctl_resume(&mut *self.ctl) != 0 }
    }

    pub fn subtune(&mut self, song: u32) -> bool {
        unsafe { psid_ctl_subtune(&mut *self.ctl, song) != 0 }
    }

    pub fn mute(&mut self, mask: u32) -> bool {
        unsafe { psid_ctl_mute(&mut *self.ctl, mask) != 0 }
    }

    pub fn state(&self) -> u32 {
        unsafe { psid_ctl_state(&*self.ctl) }
    }

    pub fn ahead(&self) -> u32 {
        unsafe { psid_ctl_ahead(&*self.ctl) }
    }

    pub fn pass(&mut self) -> bool {
        let running = |s: u32| matches!(s, PSID_INIT | PSID_PLAYING | PSID_PAUSED);
        let before = self.state();
        let ran = unsafe { psid_ctl_poll(&mut *self.ctl) != 0 };
        if !running(before) && running(self.state()) {
            self.restart_log();
        }
        while self.logged != self.ctl.head {
            let i = (self.logged & (PSID_RING - 1)) as usize;
            let e = u32::from(self.ctl.ring_entry[i]);
            self.log.push(Write {
                cycle: self.ctl.ring_cycle[i],
                chip: (e >> 13) as u8,
                reg: ((e >> 8) & 0x1f) as u8,
                value: e as u8,
            });
            self.logged = self.logged.wrapping_add(1);
        }
        if matches!(self.state(), PSID_INIT | PSID_PLAYING | PSID_PAUSED) {
            self.ahead_max = self.ahead_max.max(self.ahead());
        }
        ran
    }

    pub fn block(&mut self) -> ([i16; BLOCK], [i16; BLOCK], u32) {
        let (mut sa, mut sb) = ([0i16; BLOCK], [0i16; BLOCK]);
        let at = BlockStart(self.ctl.clock[0], self.ctl.clock[1], self.ctl.head);
        let chips = self.chips.as_mut_ptr();
        let r = unsafe {
            psid_ctl_block(
                &mut *self.ctl,
                chips,
                chips.add(1),
                sa.as_mut_ptr(),
                sb.as_mut_ptr(),
                1,
                Some(render_part),
            )
        };
        if r != 0 {
            self.blocks.push(at);
        }
        (sa, sb, r)
    }

    pub fn run(&mut self, blocks: usize, passes: usize) -> (Vec<i16>, Vec<i16>) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..blocks {
            for _ in 0..passes {
                self.pass();
            }
            let (sa, sb, _) = self.block();
            a.extend_from_slice(&sa);
            b.extend_from_slice(&sb);
        }
        (a, b)
    }

    pub fn late(&self) -> u32 {
        self.ctl.late
    }

    pub fn as_applied(&self) -> Vec<Write> {
        let mut v = self.log.clone();
        let mut b = 0;
        for (i, w) in v.iter_mut().enumerate() {
            let idx = self.log_from.wrapping_add(i as u32);
            while b < self.blocks.len() && self.blocks[b].2.wrapping_sub(idx) as i32 <= 0 {
                b += 1;
            }
            let start = |k: usize| self.blocks[k].clock(w.chip);
            let mut k = b;
            while k + 1 < self.blocks.len() && w.cycle.wrapping_sub(start(k + 1)) as i32 >= 0 {
                k += 1;
            }
            if k < self.blocks.len() && (w.cycle.wrapping_sub(start(k)) as i32) < 0 {
                w.cycle = start(k);
            }
        }
        v
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BlockStart(pub u32, pub u32, pub u32);

impl BlockStart {
    fn clock(&self, chip: u8) -> u32 {
        if chip == 0 {
            self.0
        } else {
            self.1
        }
    }
}

pub fn reference(chips: &mut [sid; 2], writes: &[Write], blocks: usize) -> (Vec<i16>, Vec<i16>) {
    let a = reference_chip(&mut chips[0], writes.iter().filter(|w| w.chip == 0), blocks);
    let b = reference_chip(&mut chips[1], writes.iter().filter(|w| w.chip == 1), blocks);
    (a, b)
}

fn reference_chip<'a>(
    s: &mut sid,
    writes: impl Iterator<Item = &'a Write>,
    blocks: usize,
) -> Vec<i16> {
    let mut out = vec![0i16; blocks * BLOCK];
    let mut it = writes.peekable();
    let (mut pos, mut now) = (0usize, 0u32);
    let cps = s.cycles_per_sample_q16;
    for b in 0..blocks {
        let end = (b + 1) * BLOCK;
        loop {
            let (mut m, mut q, mut t) = (0usize, s.cycle_frac_q16, now);
            while pos + m < end {
                let step = q.wrapping_add(cps) >> 16;
                if let Some(w) = it.peek() {
                    if (w.cycle.wrapping_sub(t.wrapping_add(step)) as i32) < 0 {
                        break;
                    }
                }
                t = t.wrapping_add(step);
                q = q.wrapping_add(cps) & 0xffff;
                m += 1;
            }
            if m > 0 {
                unsafe { sid_render(s, out[pos..].as_mut_ptr(), m as u32) };
                pos += m;
                now = t;
            }
            if pos == end {
                break;
            }
            let w = it.next().unwrap();
            let d = w.cycle.wrapping_sub(now) as i32;
            unsafe {
                if d > 0 {
                    sid_advance(s, d as u32);
                    now = w.cycle;
                }
                sid_write(s, w.reg, w.value);
            }
        }
    }
    out
}

unsafe extern "C" fn collect(ctx: *mut c_void, cycle: u32, chip: u32, reg: u32, value: u32) {
    let v = unsafe { &mut *(ctx as *mut Vec<Write>) };
    v.push(Write {
        cycle,
        chip: chip as u8,
        reg: reg as u8,
        value: value as u8,
    });
}

pub fn stream(file: &[u8], song: u32, cycles: u32) -> Vec<Write> {
    let h = Box::new(header(file));
    let chips = chips_for(&h);
    let mut mem = Box::new([0u8; 0x10000]);
    let mut env = Box::<psid_env>::default();
    let mut shadows = Box::new([sid_shadow::default(), sid_shadow::default()]);
    let mut out: Box<Vec<Write>> = Box::default();
    unsafe {
        psid_env_load(&mut *env, mem.as_mut_ptr(), &*h, file.as_ptr());
        psid_env_set_sink(
            &mut *env,
            Some(collect),
            (&mut *out as *mut Vec<Write>).cast(),
        );
        let b = if h.sid2 != 0 {
            &mut shadows[1] as *mut sid_shadow
        } else {
            std::ptr::null_mut()
        };
        psid_env_set_shadows(&mut *env, &mut shadows[0], b);
        assert_eq!(psid_env_start(&mut *env, song), 0);
        sid_shadow_init(&mut shadows[0], &chips[0], 0);
        sid_shadow_init(&mut shadows[1], &chips[1], 0);
        while (psid_env_now(&*env).wrapping_sub(cycles) as i32) < 0 {
            let r = psid_env_run(&mut *env, 10_000) as u32;
            if r >= PSID_RUN_INIT_TOO_LONG {
                break;
            }
        }
    }
    out.retain(|w| (w.cycle.wrapping_sub(cycles) as i32) < 0);
    *out
}

pub fn packed(sng: &[u8], sid: u32) -> Vec<u8> {
    let o = gt2_ref::Options {
        sid_address: sid,
        ..gt2_ref::Options::default()
    };
    let p = gt2_ref::pack(sng, &o).expect("pack");
    let mut psid = Psid {
        version: 2,
        load: p.load,
        init: p.init(),
        play: p.play(),
        songs: gt2_ref::subtunes(sng).max(1) as u16,
        start: 1,
        speed: 0,
        flags: CLOCK_PAL,
        sid2: 0,
        sid3: 0,
        data: p.code.clone(),
    };
    if sid != 0xD400 {
        psid = psid.sid2(((sid >> 4) & 0xFF) as u8);
    }
    psid.bytes()
}

pub fn songs() -> Vec<(String, Vec<u8>)> {
    let mut v = gt2_ref::examples();
    let mut own: Vec<(String, Vec<u8>)> = std::fs::read_dir(repo_root().join("assets/songs"))
        .expect("assets/songs")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            (n, std::fs::read(&p).unwrap())
        })
        .collect();
    own.sort();
    v.extend(own);
    v
}

pub const FILE_MAX: usize = 65_536;

pub fn full_size(file: &[u8]) -> Vec<u8> {
    let h = header(file);
    assert_eq!(h.data_off, 0x7C, "a v2+ header with the load address in it");
    let load = 0x7Cusize;
    let mut out = file[..0x7C].to_vec();
    out[8..10].copy_from_slice(&(load as u16).to_be_bytes());
    let mut mem: Vec<u8> = (load..0x10000)
        .map(|a| {
            if a < 0x100 {
                0
            } else {
                (a * 7 + (a >> 8)) as u8
            }
        })
        .collect();
    let from = usize::from(h.load) - load;
    mem[from..from + h.data_len as usize].copy_from_slice(&file[0x7C..0x7C + h.data_len as usize]);
    out.extend(mem);
    assert_eq!(out.len(), FILE_MAX);
    out
}
