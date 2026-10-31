// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::ffi::c_void;

use sidou_sys::*;

const FLASH: usize = 1 << 20;
const LO: usize = STORE_LO as usize;
const HI: usize = STORE_HI as usize;
const SECTOR: usize = STORE_SECTOR as usize;
const SECTORS: usize = STORE_SECTORS as usize;
const USE: usize = STORE_SECTOR_USE as usize;
const VALUE_MAX: usize = STORE_VALUE_MAX as usize;

#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

#[derive(Clone)]
pub struct Cells {
    pub mem: Vec<u8>,
    pub meant: Vec<u8>,
    pub faint: Vec<u8>,
    pub erase_cut: [bool; SECTORS],
    pub drifts_to_erased: bool,
}

impl Cells {
    fn blank() -> Self {
        Cells {
            mem: vec![0xFF; HI - LO],
            meant: vec![0xFF; HI - LO],
            faint: vec![0; HI - LO],
            erase_cut: [false; SECTORS],
            drifts_to_erased: false,
        }
    }

    fn prog(&mut self, off: usize, data: &[u8]) {
        for (i, &d) in data.iter().enumerate() {
            self.mem[off + i] &= d;
            self.meant[off + i] &= d;
            self.faint[off + i] &= d;
        }
    }

    fn erase(&mut self, sec: usize) {
        let r = sec * SECTOR..(sec + 1) * SECTOR;
        self.mem[r.clone()].fill(0xFF);
        self.meant[r.clone()].fill(0xFF);
        self.faint[r].fill(0);
        self.erase_cut[sec] = false;
    }

    fn clear_bit(&mut self, at: usize, m: u8) {
        self.mem[at] &= !m;
        self.faint[at] &= !m;
    }

    fn torn_prog(&mut self, rng: &mut Rng, off: usize, data: &[u8], tear: Tear) {
        let faint = tear.faint;
        let level = tear.shared_progress(rng);
        for (i, &d) in data.iter().enumerate() {
            let at = off + i;
            self.meant[at] &= d;
            if !faint {
                let clear = self.mem[at] & !d;
                self.mem[at] &= !(clear & rng.next() as u8);
                continue;
            }
            for bit in 0..8 {
                let m = 1u8 << bit;
                let solid0 = self.mem[at] & m == 0 && self.faint[at] & m == 0;
                if d & m != 0 || solid0 {
                    continue;
                }
                match Tear::fate(level, rng) {
                    BitFate::Done => self.clear_bit(at, m),
                    BitFate::Faint => self.faint[at] |= m,
                    BitFate::NotStarted => {}
                }
            }
        }
        if faint {
            self.reread_range(rng, off, data.len());
        }
    }

    fn torn_erase(&mut self, rng: &mut Rng, sec: usize, tear: Tear) {
        let faint = tear.faint;
        let level = tear.shared_progress(rng);
        let r = sec * SECTOR..(sec + 1) * SECTOR;
        self.erase_cut[sec] = true;
        self.meant[r.clone()].fill(0);
        if faint {
            let preprogram = rng.chance(50);
            for at in r.clone() {
                for bit in 0..8 {
                    let m = 1u8 << bit;
                    let fate = Tear::fate(level, rng);
                    if preprogram {
                        if self.mem[at] & m == 0 && self.faint[at] & m == 0 {
                            continue;
                        }
                        match fate {
                            BitFate::Done => self.clear_bit(at, m),
                            BitFate::Faint => self.faint[at] |= m,
                            BitFate::NotStarted => {}
                        }
                    } else {
                        self.clear_bit(at, m);
                        match fate {
                            BitFate::Done => self.mem[at] |= m,
                            BitFate::Faint => self.faint[at] |= m,
                            BitFate::NotStarted => {}
                        }
                    }
                }
            }
            self.reread_range(rng, r.start, SECTOR);
            return;
        }
        let s = &mut self.mem[r];
        match rng.below(3) {
            0 => s.fill(0xFF),
            1 => {
                for b in s.iter_mut() {
                    if rng.chance(50) {
                        *b = 0xFF;
                    }
                }
            }
            _ => {
                let n = rng.below(SECTOR as u64) as usize;
                s[..n].fill(0xFF);
            }
        }
    }

    fn reread_range(&mut self, rng: &mut Rng, off: usize, n: usize) {
        for at in off..off + n {
            let f = self.faint[at];
            if f != 0 {
                let ones = if self.drifts_to_erased {
                    rng.next() | rng.next()
                } else {
                    rng.next()
                };
                self.mem[at] = (self.mem[at] & !f) | (ones as u8 & f);
            }
        }
    }

    pub fn reread(&mut self, rng: &mut Rng) {
        self.reread_range(rng, 0, HI - LO);
    }
}

#[derive(Clone, Copy)]
pub struct Tear {
    pub faint: bool,
    pub pulse: bool,
}

#[derive(Clone, Copy)]
enum BitFate {
    Done,
    Faint,
    NotStarted,
}

const BIT_DONE_BELOW: u64 = 40;
const BIT_FAINT_BELOW: u64 = 75;

impl Tear {
    fn shared_progress(self, rng: &mut Rng) -> Option<u64> {
        self.pulse.then(|| rng.below(100))
    }

    fn fate(level: Option<u64>, rng: &mut Rng) -> BitFate {
        match Self::roll(level, rng) {
            r if r < BIT_DONE_BELOW => BitFate::Done,
            r if r < BIT_FAINT_BELOW => BitFate::Faint,
            _ => BitFate::NotStarted,
        }
    }

    fn roll(level: Option<u64>, rng: &mut Rng) -> u64 {
        match level {
            Some(l) => (l + rng.below(21)).saturating_sub(10).min(99),
            None => rng.below(100),
        }
    }
}

#[derive(Clone)]
pub enum Op {
    Prog(usize, Vec<u8>),
    Erase(usize),
    EraseBegin(usize),
    EraseEnd(usize),
}

pub struct Replay {
    cells: Cells,
    ops: std::vec::IntoIter<Op>,
    rng: Rng,
    tear: Tear,
}

impl Iterator for Replay {
    type Item = (Cells, bool);

    fn next(&mut self) -> Option<(Cells, bool)> {
        loop {
            let op = self.ops.next()?;
            let mut fork = self.cells.clone();
            match &op {
                Op::Prog(off, data) => {
                    fork.torn_prog(&mut self.rng, *off, data, self.tear);
                    self.cells.prog(*off, data);
                }
                Op::Erase(sec) => {
                    fork.torn_erase(&mut self.rng, *sec, self.tear);
                    self.cells.erase(*sec);
                }
                Op::EraseBegin(sec) => {
                    fork.torn_erase(&mut self.rng, *sec, self.tear);
                    self.cells = fork.clone();
                }
                Op::EraseEnd(sec) => {
                    self.cells.erase(*sec);
                    continue;
                }
            }
            return Some((fork, !matches!(op, Op::Prog(..))));
        }
    }
}

pub struct Sim {
    pub mem: Vec<u8>,
    pub cells: Cells,
    pub ops: u64,
    pub progs: u64,
    pub erases: u64,
    pub kills: u64,
    pub reclaims: u64,
    pub cut_at: Option<u64>,
    pub dead: bool,
    pub later: u64,
    pub rng: Rng,
    pub violations: Vec<String>,
    pub in_poll: u32,
    pub log_forks: bool,
    log: Option<(Cells, Vec<Op>)>,
    pub faint: bool,
    pub pulse: bool,
    pub rereads_in_session: bool,
    pub fail: u64,
    pub page_progs: Vec<u16>,
    pub page_cuts: Vec<u16>,
    pub page_progs_max: u16,
    pub failed: u64,
    pub suspend_percent: u64,
    pub erasing: Option<usize>,
    pub worn: Vec<usize>,
}

impl Sim {
    pub fn new(seed: u64) -> Box<Self> {
        Box::new(Sim {
            mem: vec![0xFF; FLASH],
            cells: Cells::blank(),
            ops: 0,
            progs: 0,
            erases: 0,
            kills: 0,
            reclaims: 0,
            cut_at: None,
            dead: false,
            later: 0,
            rng: Rng::new(seed),
            violations: Vec::new(),
            in_poll: 0,
            log_forks: false,
            log: None,
            faint: false,
            pulse: false,
            rereads_in_session: false,
            fail: 0,
            page_progs: vec![0; (HI - LO) / STORE_PAGE as usize],
            page_cuts: vec![0; (HI - LO) / STORE_PAGE as usize],
            page_progs_max: 0,
            failed: 0,
            suspend_percent: 0,
            erasing: None,
            worn: Vec::new(),
        })
    }

    pub fn with_region(seed: u64, region: &[u8], erase_cut: [bool; SECTORS]) -> Box<Self> {
        let mut sim = Sim::new(seed);
        sim.cells.mem.copy_from_slice(region);
        sim.cells.meant.copy_from_slice(region);
        sim.cells.erase_cut = erase_cut;
        sim.sync();
        sim
    }

    pub fn with_cells(seed: u64, cells: Cells, tear: Tear) -> Box<Self> {
        let mut sim = Sim::new(seed);
        sim.cells = cells;
        sim.faint = tear.faint;
        sim.pulse = tear.pulse;
        sim.sync();
        sim
    }

    fn sync(&mut self) {
        self.mem[LO..HI].copy_from_slice(&self.cells.mem);
    }

    pub fn region(&self) -> &[u8] {
        &self.mem[LO..HI]
    }

    pub fn set_faint(&mut self, off: usize, n: usize, reads: u8) {
        for at in off..off + n {
            let f = !self.cells.meant[at];
            self.cells.faint[at] = f;
            self.cells.mem[at] = (self.cells.mem[at] & !f) | (reads & f);
        }
        self.sync();
    }

    pub fn faint_reads(&mut self, reads: u8) {
        for at in 0..HI - LO {
            let f = self.cells.faint[at];
            self.cells.mem[at] = (self.cells.mem[at] & !f) | (reads & f);
        }
        self.sync();
    }

    pub fn reread(&mut self) {
        if self.faint {
            self.cells.reread(&mut self.rng);
            self.sync();
        }
    }

    pub fn replay(&mut self, seed: u64) -> Option<Replay> {
        let (cells, ops) = self.log.take()?;
        Some(Replay {
            cells,
            ops: ops.into_iter(),
            rng: Rng::new(seed),
            tear: self.tear(),
        })
    }

    pub fn tear(&self) -> Tear {
        Tear {
            faint: self.faint,
            pulse: self.pulse,
        }
    }

    fn flip(&mut self) {
        if self.rereads_in_session && self.rng.chance(2) {
            self.cells.reread(&mut self.rng);
            self.sync();
        }
    }

    fn logged(&mut self, op: Op) {
        if self.log_forks {
            let cells = &self.cells;
            self.log
                .get_or_insert_with(|| (cells.clone(), Vec::new()))
                .1
                .push(op);
        }
    }

    fn violation(&mut self, what: String) -> i32 {
        if self.violations.len() < 20 {
            self.violations.push(what);
        }
        STORE_ERROR
    }

    fn prog(&mut self, off: usize, p: *const u8, n: usize) -> i32 {
        self.in_poll += 1;
        if self.dead {
            return self.violation(format!("a program at {off:#x} after the power cut"));
        }
        if n == 0 || n > STORE_WINDOW_MAX as usize || off < LO || off + n > HI {
            return self.violation(format!("a program of {n} at {off:#x}"));
        }
        let (sec, rel) = ((off - LO) / SECTOR, (off - LO) % SECTOR);
        if rel + n > USE {
            return self.violation(format!("a program into the guard band at {off:#x}+{n}"));
        }
        if (off & 0xFF) + n > 0x100 {
            return self.violation(format!("a program across a page at {off:#x}+{n}"));
        }
        let at = self.mem.as_ptr() as usize;
        if (at..at + FLASH).contains(&(p as usize)) {
            return self.violation(format!("a program at {off:#x} from flash"));
        }
        let data = unsafe { std::slice::from_raw_parts(p, n) }.to_vec();
        if self.later > 0 && self.rng.chance(self.later) {
            return STORE_LATER;
        }
        let r = off - LO;
        let now = &self.cells.mem[r..r + n];
        let kill = rel + n <= 8 && data.iter().zip(now).all(|(&d, &m)| (!d & !m) == 0);
        if self.cells.erase_cut[sec] && !kill {
            return self.violation(format!(
                "a program at {off:#x} in sector {sec}, cut mid-erase"
            ));
        }
        let meant = &self.cells.meant[r..r + n];
        let over = !kill && meant.iter().zip(&data).any(|(&m, &d)| m != 0xFF && m != d);
        if over {
            return self.violation(format!("a program over programmed bytes at {off:#x}+{n}"));
        }
        let i = self.ops;
        self.ops += 1;
        self.progs += 1;
        self.kills += u64::from(kill && rel == 0 && data[0] == !b'S');
        let page = r / STORE_PAGE as usize;
        self.page_progs[page] += 1;
        self.logged(Op::Prog(r, data.clone()));
        let tear = self.tear();
        if self.cut_at == Some(i) {
            self.page_cuts[page] += 1;
            self.cells.torn_prog(&mut self.rng, r, &data, tear);
            self.sync();
            self.dead = true;
            self.erasing = None;
            return STORE_ERROR;
        }
        let over = self.page_progs[page] - self.page_cuts[page];
        self.page_progs_max = self.page_progs_max.max(over);
        if (self.fail > 0 && self.rng.chance(self.fail)) || self.worn.contains(&sec) {
            self.cells.torn_prog(&mut self.rng, r, &data, tear);
            self.sync();
            self.failed += 1;
            return STORE_ERROR;
        }
        self.flip();
        self.cells.prog(r, &data);
        self.mem[off..off + n].copy_from_slice(&self.cells.mem[r..r + n]);
        STORE_OK
    }

    fn erase(&mut self, off: usize) -> i32 {
        self.in_poll += 1;
        if self.dead {
            return self.violation(format!("an erase at {off:#x} after the power cut"));
        }
        if !(LO..HI).contains(&off) || !(off - LO).is_multiple_of(SECTOR) {
            return self.violation(format!("an erase at {off:#x}"));
        }
        let sec = (off - LO) / SECTOR;
        if let Some(e) = self.erasing {
            if e != sec {
                return self.violation(format!("an erase of {sec} while {e}'s is suspended"));
            }
            self.erasing = None;
            self.logged(Op::EraseEnd(sec));
            self.flip();
            self.cells.erase(sec);
            self.mem[off..off + SECTOR].fill(0xFF);
            return STORE_OK;
        }
        if self.later > 0 && self.rng.chance(self.later) {
            return STORE_LATER;
        }
        let i = self.ops;
        self.ops += 1;
        self.erases += 1;
        self.reclaims += u64::from(self.mem[off..off + SECTOR].iter().any(|&b| b != 0xFF));
        let suspend = self.suspend_percent > 0 && self.rng.chance(self.suspend_percent);
        self.logged(if suspend {
            Op::EraseBegin(sec)
        } else {
            Op::Erase(sec)
        });
        let pages = SECTOR / STORE_PAGE as usize;
        self.page_progs[sec * pages..(sec + 1) * pages].fill(0);
        self.page_cuts[sec * pages..(sec + 1) * pages].fill(0);
        let tear = self.tear();
        if self.cut_at == Some(i) {
            self.cells.torn_erase(&mut self.rng, sec, tear);
            self.sync();
            self.dead = true;
            return STORE_ERROR;
        }
        if suspend {
            self.cells.torn_erase(&mut self.rng, sec, tear);
            self.sync();
            self.erasing = Some(sec);
            return STORE_LATER;
        }
        if (self.fail > 0 && self.rng.chance(self.fail)) || self.worn.contains(&sec) {
            self.cells.torn_erase(&mut self.rng, sec, tear);
            self.sync();
            self.failed += 1;
            return STORE_ERROR;
        }
        self.flip();
        self.cells.erase(sec);
        self.mem[off..off + SECTOR].fill(0xFF);
        STORE_OK
    }
}

unsafe extern "C" fn sim_map(ctx: *mut c_void, off: u32) -> *const u8 {
    let sim = unsafe { &*ctx.cast::<Sim>() };
    unsafe { sim.mem.as_ptr().add(off as usize) }
}

unsafe extern "C" fn sim_prog(ctx: *mut c_void, off: u32, p: *const c_void, n: u32) -> i32 {
    let sim = unsafe { &mut *ctx.cast::<Sim>() };
    sim.prog(off as usize, p.cast(), n as usize)
}

unsafe extern "C" fn sim_erase(ctx: *mut c_void, off: u32) -> i32 {
    let sim = unsafe { &mut *ctx.cast::<Sim>() };
    sim.erase(off as usize)
}

pub struct Rig {
    pub sim: Box<Sim>,
    pub st: Box<store>,
    pub window: u32,
    pub work_max: u32,
    pub mount_work: (u32, u32),
    pub api_work_max: u32,
    last_put: Vec<u8>,
}

impl Rig {
    pub fn new(sim: Box<Sim>, window: u32) -> Self {
        let mut r = Rig {
            sim,
            st: Box::default(),
            window,
            work_max: 0,
            mount_work: (0, 0),
            api_work_max: 0,
            last_put: Vec::new(),
        };
        r.reset();
        r
    }

    pub fn reset(&mut self) {
        self.sim.erasing = None;
        let flash = store_flash {
            map: Some(sim_map),
            prog: Some(sim_prog),
            erase: Some(sim_erase),
            ctx: (&mut *self.sim as *mut Sim).cast(),
        };
        *self.st = Default::default();
        unsafe { store_init(&mut *self.st, &flash, self.window) };
    }

    pub fn poll(&mut self) -> bool {
        self.sim.in_poll = 0;
        let busy = unsafe { store_poll(&mut *self.st) } != 0;
        assert!(
            self.sim.in_poll <= 1,
            "{} flash operations in one store_poll",
            self.sim.in_poll
        );
        let work = self.st.work;
        assert!(work <= STORE_POLL_WORK, "a store_poll of {work} work");
        self.work_max = self.work_max.max(work);
        busy
    }

    pub fn mount(&mut self) -> store_state_id {
        self.sim.reread();
        unsafe { store_mount(&mut *self.st) };
        let mut polls = 0;
        let mut works = Vec::new();
        while self.busy() {
            self.poll();
            works.push(self.st.work);
            polls += 1;
            assert!(polls < 100_000, "the mount never ends");
        }
        assert!(polls > SECTORS, "the mount ran in {polls} polls");
        let finish = works.pop().unwrap();
        self.mount_work = (works.into_iter().max().unwrap_or(0), finish);
        self.state()
    }

    pub fn api<T>(&mut self, call: impl FnOnce(&mut store) -> T) -> T {
        self.st.work = 0;
        let v = call(&mut self.st);
        let work = self.st.work;
        assert!(work <= STORE_POLL_WORK, "an API call of {work} work");
        self.api_work_max = self.api_work_max.max(work);
        v
    }

    pub fn state(&self) -> store_state_id {
        unsafe { store_state(&*self.st) }
    }

    pub fn busy(&self) -> bool {
        unsafe { store_busy(&*self.st) != 0 }
    }

    pub fn settle(&mut self) {
        let mut polls = 0u64;
        while !self.sim.dead && self.poll() {
            polls += 1;
            assert!(polls < 2_000_000, "store_poll never rests");
        }
    }

    pub fn maintain(&mut self) {
        unsafe { store_quiet(&mut *self.st, 1) };
        self.settle();
        unsafe { store_quiet(&mut *self.st, 0) };
    }

    pub fn put(&mut self, key: u32, data: &[u8]) -> i32 {
        self.put_typed(key, 1, data)
    }

    pub fn put_typed(&mut self, key: u32, typ: u8, data: &[u8]) -> i32 {
        assert!(!self.busy(), "a put while the last write runs");
        self.last_put = data.to_vec();
        let (p, n) = (self.last_put.as_ptr().cast(), self.last_put.len() as u32);
        self.api(|st| unsafe { store_put(st, key, typ, p, n) })
    }

    pub fn delete(&mut self, key: u32) -> i32 {
        self.api(|st| unsafe { store_delete(st, key) })
    }

    pub fn collect(&mut self) -> i32 {
        self.api(|st| unsafe { store_collect(st) })
    }

    pub fn get(&self, key: u32) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; VALUE_MAX + 1];
        let n = unsafe {
            store_get(
                &*self.st,
                key,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                std::ptr::null_mut(),
            )
        };
        if n == STORE_NOT_FOUND {
            return None;
        }
        assert!((0..=VALUE_MAX as i32).contains(&n), "store_get gave {n}");
        buf.truncate(n as usize);
        Some(buf)
    }

    pub fn stats(&self) -> store_stats {
        let mut s = store_stats::default();
        unsafe { store_stats_get(&*self.st, &mut s) };
        s
    }

    pub fn erase_wanted(&mut self) -> bool {
        self.api(|st| unsafe { store_erase_wanted(st) != 0 })
    }

    pub fn put_now(&mut self, key: u32, data: &[u8]) -> i32 {
        let mut rc = self.put(key, data);
        if rc == STORE_BUSY {
            self.maintain();
            rc = self.put(key, data);
        }
        if rc == STORE_OK {
            self.settle();
        }
        rc
    }

    pub fn delete_now(&mut self, key: u32) -> i32 {
        let mut rc = self.delete(key);
        if rc == STORE_BUSY {
            self.maintain();
            rc = self.delete(key);
        }
        if rc == STORE_OK {
            self.settle();
        }
        rc
    }

    pub fn assert_clean(&self) {
        assert!(
            self.sim.violations.is_empty(),
            "the store did what it must not: {:#?}",
            self.sim.violations
        );
        assert_eq!(
            guard_band_programmed(self.sim.region(), &self.sim.cells.erase_cut),
            None
        );
    }
}

impl Rig {
    pub fn file_begin(&mut self, name: &str, len: usize) -> i32 {
        let c = std::ffi::CString::new(name).unwrap();
        self.api(|st| unsafe {
            store_file_begin(st, c.as_ptr(), STORE_FILE_SONG as u8, len as u32)
        })
    }

    pub fn file_write(&mut self, data: &[u8]) -> usize {
        unsafe { store_file_write(&mut *self.st, data.as_ptr().cast(), data.len() as u32) as usize }
    }

    pub fn file_result(&self) -> i32 {
        unsafe { store_file_result(&*self.st) }
    }

    pub fn file_stream(&mut self, data: &[u8], offer: usize, mut rng: Option<&mut Rng>) -> i32 {
        let mut at = 0;
        let mut idle = 0;
        while at < data.len() {
            let want = match rng.as_deref_mut() {
                Some(r) => 1 + r.below(offer as u64) as usize,
                None => offer,
            }
            .min(data.len() - at);
            if self.file_result() != STORE_BUSY {
                return self.file_result();
            }
            let room = STORE_WINDOW_MAX as usize - usize::from(self.st.f.fill);
            let took = self.file_write(&data[at..at + want]);
            assert_eq!(
                took,
                want.min(room),
                "store_file_write took {took} of {want} with {room} free"
            );
            at += took;
            idle = if took == 0 { idle + 1 } else { 0 };
            assert!(idle < 100 + 16 * SECTORS, "the file stopped taking bytes");
            self.poll();
            if self.sim.dead {
                return STORE_ERROR;
            }
        }
        let rc = unsafe { store_file_end(&mut *self.st) };
        if rc != STORE_OK {
            return rc;
        }
        let mut polls = 0;
        while self.file_result() == STORE_BUSY && !self.sim.dead {
            self.poll();
            polls += 1;
            assert!(polls < 1_000_000, "the file never committed");
        }
        self.file_result()
    }

    pub fn file_now(&mut self, name: &str, data: &[u8]) -> i32 {
        let mut rc = self.file_begin(name, data.len());
        if rc == STORE_BUSY {
            self.maintain();
            rc = self.file_begin(name, data.len());
        }
        if rc != STORE_OK {
            return rc;
        }
        let erases = self.sim.erases;
        let rc = self.file_stream(data, 1000, None);
        assert_eq!(self.sim.erases, erases, "an erase while a file was written");
        rc
    }

    pub fn file_open(&self, name: &str) -> Option<store_file> {
        let c = std::ffi::CString::new(name).unwrap();
        let mut f = store_file::default();
        (unsafe { store_file_open(&*self.st, c.as_ptr(), &mut f) } == STORE_OK).then_some(f)
    }

    pub fn file_read(&self, f: &store_file, off: usize, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; n];
        let got =
            unsafe { store_file_read(&*self.st, f, off as u32, buf.as_mut_ptr().cast(), n as u32) };
        if got == STORE_NOT_FOUND {
            return None;
        }
        buf.truncate(got as usize);
        Some(buf)
    }

    pub fn file(&self, name: &str) -> Option<Vec<u8>> {
        let f = self.file_open(name)?;
        let data = self.file_read(&f, 0, f.length as usize)?;
        assert_eq!(data.len(), f.length as usize);
        assert_eq!(crc32(&data), f.crc, "{name}: its CRC");
        Some(data)
    }

    pub fn dir(&self) -> Vec<(String, u8, u32)> {
        let mut f = store_file::default();
        let mut out = Vec::new();
        while unsafe { store_dir_next(&*self.st, &mut f) } == 1 {
            let name = unsafe { std::ffi::CStr::from_ptr(f.name.as_ptr()) };
            out.push((name.to_str().unwrap().to_owned(), f.type_, f.length));
            assert!(out.len() <= STORE_KEYS_MAX as usize, "the directory loops");
        }
        out
    }

    pub fn file_delete(&mut self, name: &str) -> i32 {
        let c = std::ffi::CString::new(name).unwrap();
        self.api(|st| unsafe { store_file_delete(st, c.as_ptr()) })
    }

    pub fn file_rename(&mut self, from: &str, to: &str) -> i32 {
        let (a, b) = (
            std::ffi::CString::new(from).unwrap(),
            std::ffi::CString::new(to).unwrap(),
        );
        self.api(|st| unsafe { store_file_rename(st, a.as_ptr(), b.as_ptr()) })
    }
}

pub fn guard_band_programmed(region: &[u8], erase_cut: &[bool; SECTORS]) -> Option<usize> {
    (0..SECTORS).find(|&s| {
        !erase_cut[s]
            && region[s * SECTOR + USE..(s + 1) * SECTOR]
                .iter()
                .any(|&b| b != 0xFF)
    })
}

pub fn fresh(seed: u64, window: u32) -> Rig {
    let mut r = Rig::new(Sim::new(seed), window);
    assert_eq!(r.mount(), STORE_STATE_BLANK);
    r.maintain();
    r
}

pub(crate) fn crc32(data: &[u8]) -> u32 {
    unsafe { store_crc32(0, data.as_ptr().cast(), data.len() as u32) }
}

fn rd32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn crc32_is_zlibs() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    let whole = crc32(b"hello, world");
    let part = unsafe { store_crc32(crc32(b"hello"), b", world".as_ptr().cast(), 7) };
    assert_eq!(whole, part, "a CRC continues from a previous result");
}

const STORE_RAM_BUDGET: usize = 2_000;

#[test]
fn the_constants_are_the_plans_and_the_store_fits_its_ram_budget() {
    assert_eq!(STORE_RECORDS, 3808);
    assert_eq!(STORE_POOL_MIN, 20);
    assert_eq!(STORE_POOL_TARGET, 22);
    assert_eq!(STORE_FILE_SECTOR_DATA, 3744);
    assert_eq!(STORE_CAPACITY, 198_432);
    let size = ilp32::store::SIZE;
    println!("store: {size} bytes on the device");
    assert!(size <= STORE_RAM_BUDGET, "the store struct is {size} bytes");
}

#[test]
fn a_blank_region_mounts_blank_and_formats_lazily() {
    let mut r = Rig::new(Sim::new(1), 256);
    assert_eq!(r.mount(), STORE_STATE_BLANK);
    assert_eq!(r.sim.ops, 0, "mounting writes nothing");
    assert_eq!(
        r.put(1, b"x"),
        STORE_BUSY,
        "no pool before the first refill"
    );
    assert!(r.erase_wanted());
    r.maintain();
    let s = r.stats();
    assert_eq!(s.pool, STORE_POOL_TARGET);
    assert_eq!(
        r.sim.erases,
        u64::from(STORE_POOL_TARGET),
        "it erases what the pool needs"
    );
    assert_eq!(r.state(), STORE_STATE_SIDOU);
    assert!(!r.erase_wanted());
    assert_eq!(r.put_now(1, b"x"), STORE_OK);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(1).as_deref(), Some(&b"x"[..]));
    r.assert_clean();
}

#[test]
fn values_round_trip_survive_a_power_cycle_and_deletes_stick() {
    let mut r = fresh(2, 256);
    let mut rng = Rng::new(2);
    let big = rng.bytes(VALUE_MAX);
    assert_eq!(r.put_now(7, b"seven"), STORE_OK);
    assert_eq!(r.put_now(8, &big), STORE_OK);
    assert_eq!(r.put_now(9, b""), STORE_OK);
    assert_eq!(r.put_now(7, b"SEVEN"), STORE_OK);
    assert_eq!(r.put_typed(10, STORE_TYPE_MAX as u8, b"t"), STORE_OK);
    r.settle();
    assert_eq!(r.get(7).as_deref(), Some(&b"SEVEN"[..]));
    assert_eq!(r.get(8), Some(big.clone()));
    assert_eq!(r.get(9), Some(Vec::new()));
    assert_eq!(r.get(11), None);
    let mut typ = 0u8;
    let mut buf = [0u8; 4];
    let n = unsafe { store_get(&*r.st, 10, buf.as_mut_ptr().cast(), 4, &mut typ) };
    assert_eq!((n, typ), (1, STORE_TYPE_MAX as u8));
    let n = unsafe { store_get(&*r.st, 8, buf.as_mut_ptr().cast(), 4, std::ptr::null_mut()) };
    assert_eq!((n as usize, &buf[..]), (VALUE_MAX, &big[..4]));
    assert_eq!(r.delete_now(7), STORE_OK);
    assert_eq!(r.get(7), None);
    assert_eq!(r.delete_now(7), STORE_OK, "deleting a deleted key");
    assert_eq!(r.delete_now(1234), STORE_OK, "deleting a missing key");
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(7), None);
    assert_eq!(r.get(8), Some(big));
    assert_eq!(r.get(9), Some(Vec::new()));
    assert_eq!(r.stats().corrupt, 0);
    r.assert_clean();
}

#[test]
fn bad_arguments_are_refused() {
    let mut r = fresh(3, 256);
    let big = vec![0u8; VALUE_MAX + 1];
    assert_eq!(r.put(1, &big), STORE_INVALID);
    assert_eq!(r.put(STORE_KEY_MAX + 1, b"x"), STORE_INVALID);
    assert_eq!(r.put_typed(1, STORE_TYPE_TOMB as u8, b""), STORE_INVALID);
    assert_eq!(r.delete(STORE_KEY_MAX + 1), STORE_INVALID);
    assert_eq!(r.put(1, b"x"), STORE_OK);
    assert!(r.busy());
    assert_eq!(r.delete(1), STORE_BUSY, "one write at a time");
    r.settle();
    assert_eq!(r.sim.ops, u64::from(STORE_POOL_TARGET) * 2 + 4);
}

#[test]
fn the_bytes_are_the_documented_format() {
    let mut r = fresh(4, 256);
    let region = r.sim.region().to_vec();
    let clean: Vec<usize> = (0..SECTORS)
        .filter(|&s| region[s * SECTOR..s * SECTOR + 8] == *b"SIDOUSTR")
        .collect();
    assert_eq!(clean.len(), STORE_POOL_TARGET as usize);
    for &s in &clean {
        let h = &region[s * SECTOR..(s + 1) * SECTOR];
        assert_eq!(&h[8..12], &[1, 0, 0, 0], "format 1, then zero");
        assert_eq!(rd32(h, 12), 1, "erased once");
        assert_eq!(rd32(h, 16), crc32(&h[..16]));
        assert!(h[20..].iter().all(|&b| b == 0xFF));
    }
    assert_eq!(r.put_now(0x1234_5678, b"hello"), STORE_OK);
    let region = r.sim.region().to_vec();
    let s = (0..SECTORS)
        .find(|&s| rd32(&region, s * SECTOR + 28) == 0)
        .expect("a claimed sector");
    let h = &region[s * SECTOR..(s + 1) * SECTOR];
    assert_eq!(rd32(h, 20), rd32(h, 12));
    assert_eq!(rd32(h, 24), crc32(&h[..24]));
    assert_eq!(rd32(h, 28), 0);
    let rec = &h[32..32 + 16 + 8 + 4];
    assert_eq!(&rec[..4], &[1, 0, 5, 0]);
    assert_eq!(rd32(rec, 4), 0x1234_5678);
    assert!(rd32(rec, 8) > 0, "the record's number");
    let mut covered = rec[..12].to_vec();
    covered.extend_from_slice(b"hello");
    assert_eq!(rd32(rec, 12), crc32(&covered));
    assert_eq!(&rec[16..21], b"hello");
    assert_eq!(&rec[21..24], &[0xFF; 3], "the padding stays erased");
    assert_eq!(rd32(rec, 24), 0, "the commit word");
    assert!(h[32 + 28..].iter().all(|&b| b == 0xFF));
    r.assert_clean();
}

#[test]
fn programs_keep_to_the_window_and_the_page() {
    for window in [16u32, 64, 100, 256] {
        let mut r = fresh(5, window);
        let mut rng = Rng::new(u64::from(window));
        for k in 0..40 {
            let n = rng.below(VALUE_MAX as u64 + 1) as usize;
            let v = rng.bytes(n);
            assert_eq!(r.put_now(k % 7, &v), STORE_OK);
            assert_eq!(r.get(k % 7), Some(v));
        }
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        r.assert_clean();
    }
}

#[test]
fn the_core_retries_what_the_device_defers() {
    let mut r = fresh(6, 64);
    r.sim.later = 60;
    let mut rng = Rng::new(6);
    let mut model = BTreeMap::new();
    for _ in 0..300 {
        let k = rng.below(10) as u32;
        if rng.chance(20) {
            assert_eq!(r.delete_now(k), STORE_OK);
            model.remove(&k);
        } else {
            let n = rng.below(400) as usize;
            let v = rng.bytes(n);
            assert_eq!(r.put_now(k, &v), STORE_OK);
            model.insert(k, v);
        }
        if rng.chance(10) {
            r.maintain();
        }
    }
    r.sim.later = 0;
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    for k in 0..10 {
        assert_eq!(r.get(k), model.get(&k).cloned(), "key {k}");
    }
    r.assert_clean();
}

#[test]
fn the_pool_never_goes_under_its_minimum_and_refills_to_its_target() {
    let mut r = fresh(7, 256);
    let mut rng = Rng::new(7);
    let v = rng.bytes(VALUE_MAX);
    let mut busy = 0;
    for i in 0..200u32 {
        let rc = r.put(i % 5, &v);
        assert!(
            r.stats().pool >= STORE_POOL_MIN,
            "the pool under its minimum"
        );
        if rc == STORE_BUSY {
            busy += 1;
            assert_eq!(r.stats().pool, STORE_POOL_MIN);
            r.maintain();
            assert_eq!(r.stats().pool, STORE_POOL_TARGET, "a quiet refill");
            assert_eq!(r.put(i % 5, &v), STORE_OK);
        } else {
            assert_eq!(rc, STORE_OK);
        }
        r.settle();
    }
    assert!(busy > 10, "the pool ran short {busy} times");
    r.assert_clean();
}

#[test]
fn erase_counts_stay_within_a_small_spread() {
    let mut r = fresh(8, 256);
    let mut rng = Rng::new(8);
    for k in 100..110 {
        let v = rng.bytes(VALUE_MAX);
        assert_eq!(r.put_now(k, &v), STORE_OK);
    }
    let mut n = 0u64;
    while r.sim.erases < 10_000 {
        let len = 300 + rng.below(900) as usize;
        let v = rng.bytes(len);
        assert_eq!(r.put_now(rng.below(6) as u32, &v), STORE_OK);
        n += 1;
        if n.is_multiple_of(16) {
            r.maintain();
        }
    }
    r.maintain();
    let s = r.stats();
    println!(
        "{} erases over {n} puts: erase counts {}..{}",
        r.sim.erases, s.erases_min, s.erases_max
    );
    assert!(
        s.erases_max - s.erases_min <= 8,
        "erase counts spread {}..{}",
        s.erases_min,
        s.erases_max
    );
    for k in 100..110 {
        assert!(r.get(k).is_some());
    }
    r.assert_clean();
}

#[test]
fn a_full_key_index_refuses_cleanly_and_a_delete_makes_room_after_a_collection() {
    let mut r = fresh(9, 256);
    for k in 0..STORE_KEYS_MAX {
        assert_eq!(r.put_now(k, &k.to_le_bytes()), STORE_OK);
    }
    let ops = r.sim.ops;
    assert_eq!(r.put(1000, b"one too many"), STORE_FULL);
    assert_eq!(r.sim.ops, ops, "nothing written");
    assert!(!r.busy());
    assert_eq!(r.delete_now(3), STORE_OK);
    assert_eq!(
        r.put(1000, b"one too many"),
        STORE_FULL,
        "the tombstone holds the key"
    );
    for round in 0..2u8 {
        for k in 10..STORE_KEYS_MAX {
            assert_eq!(r.put_now(k, &[k as u8 ^ round; 600]), STORE_OK);
        }
    }
    let mut collections = 0;
    while r.collect() == STORE_OK {
        collections += 1;
        r.settle();
        r.maintain();
        if r.put(1000, b"room") == STORE_OK {
            r.settle();
            break;
        }
        assert!(collections < SECTORS, "the key index never made room");
    }
    assert_eq!(r.get(1000).as_deref(), Some(&b"room"[..]));
    assert_eq!(r.get(3), None);
    r.reset();
    r.mount();
    assert_eq!(r.get(3), None);
    assert_eq!(r.get(1000).as_deref(), Some(&b"room"[..]));
    r.assert_clean();
}

fn foreign_region(seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut region = vec![0xFF; HI - LO];
    for s in 0..20 {
        let at = s * SECTOR;
        region[at..at + 4].copy_from_slice(b"FLCA");
        let n = 1000 + rng.below(3000) as usize;
        let body = rng.bytes(n);
        region[at + 4..at + 4 + body.len()].copy_from_slice(&body);
    }
    region
}

#[test]
fn foreign_data_mounts_foreign_and_nothing_is_written_until_a_format() {
    let region = foreign_region(10);
    let mut r = Rig::new(Sim::with_region(10, &region, [false; SECTORS]), 256);
    assert_eq!(r.mount(), STORE_STATE_FOREIGN);
    assert_eq!(r.put(1, b"x"), STORE_FOREIGN);
    assert_eq!(r.delete(1), STORE_FOREIGN);
    assert_eq!(r.collect(), STORE_FOREIGN);
    assert!(!r.erase_wanted());
    r.maintain();
    assert_eq!(r.sim.ops, 0);
    assert_eq!(r.sim.region(), &region[..], "not a byte changed");
    assert_eq!(r.get(1), None);
    let mut one = vec![0xFF; HI - LO];
    one[40 * SECTOR + 100] = 0x42;
    let mut r2 = Rig::new(Sim::with_region(10, &one, [false; SECTORS]), 256);
    assert_eq!(r2.mount(), STORE_STATE_FOREIGN);
    unsafe { store_format(&mut *r.st) };
    assert_eq!(r.state(), STORE_STATE_BLANK);
    assert_eq!(r.put(1, b"early"), STORE_BUSY);
    r.maintain();
    assert_eq!(r.stats().pool, SECTORS as u32, "all 73 erased");
    for sec in r.sim.region().chunks(SECTOR) {
        assert!(
            sec.iter().all(|&b| b == 0xFF) || sec[..8] == *b"SIDOUSTR",
            "a sector neither erased nor SIDOU's"
        );
    }
    r.reset();
    assert_ne!(r.mount(), STORE_STATE_FOREIGN);
    r.maintain();
    assert_eq!(r.put_now(1, b"mine"), STORE_OK);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(1).as_deref(), Some(&b"mine"[..]));
    r.assert_clean();
}

#[test]
fn a_newer_format_is_not_ours_to_touch() {
    let mut r = fresh(11, 256);
    assert_eq!(r.put_now(1, b"x"), STORE_OK);
    let mut region = r.sim.region().to_vec();
    let s = (0..SECTORS)
        .find(|&s| region[s * SECTOR..s * SECTOR + 8] == *b"SIDOUSTR")
        .unwrap();
    let h = &mut region[s * SECTOR..s * SECTOR + 20];
    h[8] = STORE_FORMAT as u8 + 1;
    let crc = crc32(&h[..16]);
    h[16..20].copy_from_slice(&crc.to_le_bytes());
    let mut r2 = Rig::new(Sim::with_region(11, &region, [false; SECTORS]), 256);
    assert_eq!(r2.mount(), STORE_STATE_FOREIGN);
    assert_eq!(r2.put(2, b"y"), STORE_FOREIGN);
}

#[derive(Clone, Debug)]
enum Inflight {
    Idle,
    Put(u32, Vec<u8>),
    Delete(u32),
}

struct Run {
    rig: Rig,
    rng: Rng,
    model: BTreeMap<u32, Vec<u8>>,
    keys: u32,
    inflight: Inflight,
    undecided: Inflight,
    undecided_key_before: Option<(u32, Option<Vec<u8>>)>,
    forks: bool,
    deep_check_every: u64,
    forks_checked: u64,
    cuts: u64,
    full: u64,
    seed: u64,
}

impl Run {
    fn new(seed: u64, keys: u32, forks: bool) -> Self {
        Run::with(seed, keys, forks, [16, 64, 256][(seed % 3) as usize], false)
    }

    fn with(seed: u64, keys: u32, forks: bool, window: u32, faint: bool) -> Self {
        let mut rng = Rng::new(seed ^ 0xC0FFEE);
        let mut rig = fresh(seed, window);
        rig.sim.log_forks = forks;
        set_tear(&mut rig, seed, faint);
        rig.sim.later = if rng.chance(30) { 25 } else { 0 };
        Run {
            rig,
            rng,
            model: BTreeMap::new(),
            keys,
            inflight: Inflight::Idle,
            undecided: Inflight::Idle,
            undecided_key_before: None,
            forks,
            deep_check_every: 8,
            forks_checked: 0,
            cuts: 0,
            full: 0,
            seed,
        }
    }

    fn allowed(&self, key: u32) -> Vec<Option<Vec<u8>>> {
        let mut a = vec![self.model.get(&key).cloned()];
        if let Some((k, v)) = &self.undecided_key_before {
            if *k == key {
                a.push(v.clone());
            }
        }
        for op in [&self.inflight, &self.undecided] {
            match op {
                Inflight::Put(k, v) if *k == key => a.push(Some(v.clone())),
                Inflight::Delete(k) if *k == key => a.push(None),
                _ => {}
            }
        }
        a
    }

    fn verify(&self, rig: &Rig, what: &str) {
        assert_eq!(
            rig.stats().corrupt,
            0,
            "seed {}: {what}: corruption seen",
            self.seed
        );
        for k in 0..self.keys {
            let got = rig.get(k);
            assert!(
                self.allowed(k).contains(&got),
                "seed {}: {what}: key {k} reads {:?}, allowed {:?} (in flight {:?})",
                self.seed,
                got.as_ref().map(|v| v.len()),
                self.allowed(k)
                    .iter()
                    .map(|v| v.as_ref().map(|v| v.len()))
                    .collect::<Vec<_>>(),
                self.inflight
            );
        }
    }

    fn check_forks(&mut self) {
        let Some(replay) = self.rig.sim.replay(self.seed ^ self.forks_checked) else {
            return;
        };
        let tear = self.rig.sim.tear();
        for (cells, erased) in replay {
            self.forks_checked += 1;
            assert_eq!(guard_band_programmed(&cells.mem, &cells.erase_cut), None);
            let mut r = Rig::new(
                Sim::with_cells(self.forks_checked, cells, tear),
                self.rig.window,
            );
            let state = r.mount();
            assert!(
                state == STORE_STATE_SIDOU || state == STORE_STATE_BLANK,
                "seed {}: a cut left the region {state}",
                self.seed
            );
            self.verify(&r, "a forked cut");
            if erased || self.forks_checked.is_multiple_of(self.deep_check_every) {
                let first: Vec<Option<Vec<u8>>> = (0..self.keys).map(|k| r.get(k)).collect();
                r.maintain();
                let k = self.keys;
                assert_eq!(r.put_now(k, b"after"), STORE_OK, "seed {}", self.seed);
                assert_eq!(r.get(k).as_deref(), Some(&b"after"[..]));
                self.verify(&r, "a forked cut, then a write");
                r.assert_clean();
                r.reset();
                assert_eq!(r.mount(), STORE_STATE_SIDOU, "seed {}", self.seed);
                for (key, want) in first.iter().enumerate() {
                    assert_eq!(
                        &r.get(key as u32),
                        want,
                        "seed {}: key {key} changed at the next mount",
                        self.seed
                    );
                }
                assert_eq!(r.get(k).as_deref(), Some(&b"after"[..]));
            }
        }
    }

    fn recover(&mut self) {
        self.cuts += 1;
        self.rig.sim.dead = false;
        self.rig.sim.cut_at = None;
        self.rig.reset();
        let state = self.rig.mount();
        assert_eq!(state, STORE_STATE_SIDOU, "seed {}", self.seed);
        self.verify(&self.rig, "a real cut");
        let undecided = std::mem::replace(&mut self.inflight, Inflight::Idle);
        if !matches!(undecided, Inflight::Idle) {
            self.undecided = undecided;
            self.undecided_key_before = None;
        }
        if let Inflight::Put(k, _) | Inflight::Delete(k) = self.undecided {
            if self.undecided_key_before.is_none() {
                self.undecided_key_before = Some((k, self.model.get(&k).cloned()));
            }
            match self.rig.get(k) {
                Some(v) => self.model.insert(k, v),
                None => self.model.remove(&k),
            };
        }
        self.inflight = Inflight::Idle;
    }

    fn finish(&mut self) {
        self.rig.settle();
        if self.forks {
            self.check_forks();
        }
        if self.rig.sim.dead {
            self.recover();
            return;
        }
        let done = std::mem::replace(&mut self.inflight, Inflight::Idle);
        if !matches!(done, Inflight::Idle) {
            self.undecided = Inflight::Idle;
            self.undecided_key_before = None;
        }
        match done {
            Inflight::Put(k, v) => {
                self.model.insert(k, v);
            }
            Inflight::Delete(k) => {
                self.model.remove(&k);
            }
            Inflight::Idle => {}
        }
    }

    fn write(&mut self, key: u32, value: Option<Vec<u8>>) {
        let attempt = |rig: &mut Rig| {
            let before = rig.stats().pool;
            let rc = match &value {
                Some(v) => rig.put(key, v),
                None => rig.delete(key),
            };
            (rc, before)
        };
        let (mut rc, mut before) = attempt(&mut self.rig);
        for _ in 0..3 {
            if rc != STORE_BUSY {
                break;
            }
            self.maintain();
            (rc, before) = attempt(&mut self.rig);
        }
        if rc == STORE_FULL {
            self.full += 1;
            return;
        }
        assert_eq!(rc, STORE_OK, "seed {}", self.seed);
        let floor = if value.is_some() {
            STORE_POOL_MIN
        } else {
            STORE_POOL_MIN - 1
        };
        let pool = self.rig.stats().pool;
        assert!(
            pool >= floor.min(before),
            "seed {}: a write took the pool from {before} to {pool}",
            self.seed
        );
        self.inflight = match value {
            Some(v) => Inflight::Put(key, v),
            None => Inflight::Delete(key),
        };
        self.finish();
    }

    fn maintain(&mut self) {
        self.rig.maintain();
        let cut = self.rig.sim.dead;
        self.finish();
        if !cut {
            assert!(
                !self.rig.erase_wanted(),
                "seed {}: maintenance rested with work",
                self.seed
            );
        }
    }

    fn step(&mut self) {
        if self.rig.sim.cut_at.is_none() && self.rng.chance(3) {
            self.rig.sim.cut_at = Some(self.rig.sim.ops + self.rng.below(40));
        }
        let key = self.rng.below(u64::from(self.keys)) as u32;
        match self.rng.below(100) {
            0..=61 => {
                let len = match self.rng.below(20) {
                    0 => self.rng.below(VALUE_MAX as u64 + 1),
                    1..=5 => 64 + self.rng.below(400),
                    _ => self.rng.below(64),
                } as usize;
                let v = self.rng.bytes(len);
                self.write(key, Some(v));
            }
            62..=79 => self.write(key, None),
            80..=86 => {
                if self.rig.collect() == STORE_OK {
                    self.rig.settle();
                    self.finish();
                }
            }
            _ => self.maintain(),
        }
        assert!(
            self.rig.sim.violations.is_empty(),
            "seed {}: {:#?}",
            self.seed,
            self.rig.sim.violations
        );
    }

    fn end(&mut self) {
        self.rig.sim.cut_at = None;
        self.rig.sim.log_forks = false;
        self.maintain();
        self.rig.reset();
        assert_eq!(self.rig.mount(), STORE_STATE_SIDOU);
        self.verify(&self.rig, "the end");
        self.rig.assert_clean();
        if self.rig.window >= 16 {
            assert!(u32::from(self.rig.sim.page_progs_max) <= STORE_PAGE_PROGS);
        }
    }
}

fn set_tear(rig: &mut Rig, seed: u64, faint: bool) {
    rig.sim.faint = faint;
    rig.sim.pulse = faint && !seed.is_multiple_of(3);
    rig.sim.rereads_in_session = rig.sim.pulse;
    rig.sim.cells.drifts_to_erased = faint && seed % 5 < 2;
    rig.sim.suspend_percent = if seed % 4 == 1 { 30 } else { 0 };
}

const WINDOWS: [u32; 9] = [1, 3, 4, 8, 12, 16, 64, 100, 256];

fn power_loss(seed: u64, steps: u32, keys: u32, forks: bool, window: u32, faint: bool) -> Run {
    let mut run = Run::with(seed, keys, forks, window, faint);
    for _ in 0..steps {
        run.step();
    }
    run.end();
    let sim = &run.rig.sim;
    assert!(
        sim.kills > 0 && sim.reclaims > 0,
        "seed {seed}: {} kills, {} erases of used sectors",
        sim.kills,
        sim.reclaims
    );
    run
}

#[test]
fn power_loss_at_every_operation_leaves_the_old_value_or_the_new() {
    let (mut forks, mut cuts, mut kills, mut reclaims) = (0, 0, 0, 0);
    for (seed, window) in WINDOWS.into_iter().enumerate() {
        let steps = if window < 8 { 120 } else { 250 };
        let run = power_loss(seed as u64, steps, 10, true, window, seed % 2 == 1);
        forks += run.forks_checked;
        cuts += run.cuts;
        kills += run.rig.sim.kills;
        reclaims += run.rig.sim.reclaims;
    }
    println!(
        "{forks} forked cuts and {cuts} real cuts checked; {kills} kills, {reclaims} sectors \
         reclaimed"
    );
    assert!(forks > 10_000 && cuts > 15);
}

#[test]
fn power_loss_with_real_cuts_over_long_runs() {
    let mut cuts = 0;
    for seed in 100..118 {
        let window = WINDOWS[seed as usize % WINDOWS.len()];
        let run = power_loss(seed, 2_500, 14, false, window, seed % 2 == 0);
        cuts += run.cuts;
    }
    println!("{cuts} real cuts checked");
    assert!(cuts > 500);
}

#[test]
#[ignore]
fn power_loss_long() {
    let (mut forks, mut cuts) = (0, 0);
    for seed in LONG_FORKED {
        let (f, c) = long_seed(seed, false);
        forks += f;
        cuts += c;
    }
    for seed in LONG_REAL {
        cuts += long_seed(seed, false).1;
    }
    println!("{forks} forked cuts and {cuts} real cuts checked");
}

const LONG_FORKED: std::ops::Range<u64> = 1_000..1_400;
const LONG_REAL: std::ops::Range<u64> = 10_000..13_000;
const LONG_FILES_FORKED: std::ops::Range<u64> = 1_000..1_060;
const LONG_FILES_REAL: std::ops::Range<u64> = 10_000..10_300;

fn long_seed(seed: u64, forked: bool) -> (u64, u64) {
    let window = 1 + (seed % 256) as u32;
    let run = if forked || LONG_FORKED.contains(&seed) {
        power_loss(seed, 1_500, 12, true, window, !seed.is_multiple_of(4))
    } else {
        power_loss(seed, 3_000, 16, false, window, !seed.is_multiple_of(4))
    };
    (run.forks_checked, run.cuts)
}

fn long_files_seed(seed: u64, forked: bool) -> (u64, u64) {
    let run = if forked || LONG_FILES_FORKED.contains(&seed) {
        let window = WINDOWS[seed as usize % WINDOWS.len()];
        let max_len = (window as usize * 100).clamp(1_000, 20_000);
        file_power_loss(seed, 200, true, max_len, window, !seed.is_multiple_of(4))
    } else {
        let window = 1 + (seed % 256) as u32;
        file_power_loss(seed, 600, false, FILE_MAX, window, !seed.is_multiple_of(4))
    };
    (run.forks_checked, run.cuts)
}

#[test]
#[ignore]
fn power_loss_sweep() {
    let Some(arg) = std::env::var_os("SIDOU_STORE_SWEEP") else {
        return;
    };
    let arg = arg.into_string().unwrap();
    let (kind, seed) = arg.split_once(':').expect("kind:seed");
    let seed: u64 = seed.parse().unwrap();
    let (forks, cuts) = match kind {
        "values" => long_seed(seed, false),
        "files" => long_files_seed(seed, false),
        "values-forked" => long_seed(seed, true),
        "files-forked" => long_files_seed(seed, true),
        _ => panic!("{kind}: values, files, values-forked or files-forked"),
    };
    println!("{kind} seed {seed}: {forks} forked cuts, {cuts} real cuts");
}

#[test]
fn a_deletion_survives_its_sector_being_collected_first() {
    let mut run = Run::new(12, 24, true);
    run.rig.sim.later = 0;
    run.write(1, Some(vec![0xA1; 40]));
    for k in 10..13 {
        run.write(k, Some(vec![k as u8; 1100]));
    }
    for _ in 0..3 {
        run.write(2, Some(vec![2; 1200]));
    }
    run.write(1, None);
    for _ in 0..12 {
        run.write(2, Some(vec![3; 1200]));
    }
    let mut first = false;
    for _ in 0..6 {
        let tombs = sectors_holding(run.rig.sim.region(), 1, STORE_TYPE_TOMB as u8);
        if run.rig.collect() == STORE_OK {
            run.rig.settle();
            run.finish();
        }
        let region = run.rig.sim.region();
        let gone = tombs
            .iter()
            .any(|&s| !sectors_holding(region, 1, STORE_TYPE_TOMB as u8).contains(&s));
        first |= gone && !sectors_holding(region, 1, 1).is_empty();
        run.maintain();
        assert_eq!(run.rig.get(1), None);
    }
    assert!(
        first,
        "the deletion's sector was not collected before the value's"
    );
    for k in 10..13 {
        run.write(k, Some(vec![0; 8]));
    }
    for _ in 0..10 {
        run.write(2, Some(vec![4; 1200]));
        if run.rig.collect() == STORE_OK {
            run.rig.settle();
            run.finish();
        }
        run.maintain();
    }
    assert_eq!(run.rig.get(1), None);
    run.end();
    assert_eq!(run.rig.get(1), None);
    assert!(run.forks_checked > 200);
}

const SECTOR_HEADER_LEN: usize = 32;
const RECORD_HEADER_LEN: usize = 16;
const COMMIT_WORD_LEN: usize = 4;
const VALUE_RECORD: u8 = 1;

fn record_len(h: &[u8], off: usize) -> usize {
    let data = u16::from_le_bytes([h[off + 2], h[off + 3]]) as usize;
    RECORD_HEADER_LEN + data.div_ceil(4) * 4 + COMMIT_WORD_LEN
}

fn record_offsets(h: &[u8]) -> impl Iterator<Item = usize> + '_ {
    let mut off = SECTOR_HEADER_LEN;
    std::iter::from_fn(move || {
        if off + RECORD_HEADER_LEN + COMMIT_WORD_LEN > USE || h[off] == 0xFF {
            return None;
        }
        let at = off;
        off += record_len(h, at);
        Some(at)
    })
}

fn sectors_holding(region: &[u8], key: u32, typ: u8) -> Vec<usize> {
    let mut out = Vec::new();
    for s in 0..SECTORS {
        let h = &region[s * SECTOR..(s + 1) * SECTOR];
        if h[..8] != *b"SIDOUSTR" || rd32(h, 28) != 0 {
            continue;
        }
        if record_offsets(h).any(|off| h[off] == typ && rd32(h, off + 4) == key) {
            out.push(s);
        }
    }
    out
}

pub(crate) fn fixture_value(k: u32, round: u32) -> Vec<u8> {
    let len = ((k * 37 + round * 11) % 120) as usize;
    (0..len)
        .map(|i| (i as u32 * 7 + k * 13 + round) as u8)
        .collect()
}

pub(crate) fn fixture_file(seed: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u32 * 31 + seed * 17 + (i as u32 >> 8)) as u8)
        .collect()
}

pub(crate) const FIXTURE_FILES: [(&str, u32, usize); 3] =
    [("NEW", 4, 400), ("SONG A", 1, 5000), ("SONG B", 3, 2600)];

pub(crate) const FIXTURE_KEYS: u32 = 16;
pub(crate) const FIXTURE_ROUNDS: u32 = 12;
pub(crate) const FIXTURE_DELETED: [u32; 3] = [3, 8, 13];

pub(crate) fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/store-v1.bin")
}

fn fixture_region() -> Vec<u8> {
    let mut r = fresh(13, 64);
    for round in 0..FIXTURE_ROUNDS {
        for k in 0..FIXTURE_KEYS {
            assert_eq!(r.put_now(k, &fixture_value(k, round)), STORE_OK);
        }
        match round {
            2 => assert_eq!(r.file_now("SONG A", &fixture_file(1, 5000)), STORE_OK),
            3 => assert_eq!(r.file_now("SONG B", &fixture_file(2, 1000)), STORE_OK),
            4 => assert_eq!(r.file_now("GONE", &fixture_file(5, 700)), STORE_OK),
            5 => assert_eq!(r.file_now("OLD", &fixture_file(4, 400)), STORE_OK),
            6 => assert_eq!(r.file_now("SONG B", &fixture_file(3, 2600)), STORE_OK),
            7 => {
                assert_eq!(r.file_delete("GONE"), STORE_OK);
                r.settle();
                assert_eq!(r.file_rename("OLD", "NEW"), STORE_OK);
                r.settle();
            }
            _ => {}
        }
        if round % 4 == 3 {
            let _ = r.collect();
            r.settle();
            r.maintain();
        }
    }
    for k in FIXTURE_DELETED {
        assert_eq!(r.delete_now(k), STORE_OK);
    }
    r.sim.cut_at = Some(r.sim.ops + 2);
    assert_eq!(r.put(0, &[0x55; 300]), STORE_OK);
    r.settle();
    assert!(r.sim.dead);
    r.sim.region().to_vec()
}

fn encode_runs(region: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < region.len() {
        if region[i] == 0xFF {
            i += 1;
            continue;
        }
        let (start, mut end) = (i, i + 1);
        let mut j = i + 1;
        while j < region.len() && j - start < 0xFFFF && j - end < 8 {
            if region[j] != 0xFF {
                end = j + 1;
            }
            j += 1;
        }
        out.extend_from_slice(&((LO + start) as u32).to_le_bytes());
        out.extend_from_slice(&((end - start) as u16).to_le_bytes());
        out.extend_from_slice(&region[start..end]);
        i = end;
    }
    out
}

pub(crate) fn decode_runs(file: &[u8]) -> Vec<u8> {
    let mut region = vec![0xFF; HI - LO];
    let mut i = 0;
    while i < file.len() {
        let off = rd32(file, i) as usize - LO;
        let len = u16::from_le_bytes([file[i + 4], file[i + 5]]) as usize;
        region[off..off + len].copy_from_slice(&file[i + 6..i + 6 + len]);
        i += 6 + len;
    }
    region
}

#[test]
fn a_format_1_region_mounts_to_the_same_values() {
    let path = fixture_path();
    if std::env::var_os("SIDOU_STORE_FIXTURE_UPDATE").is_some() {
        assert_eq!(
            STORE_FORMAT, 1,
            "the v1 fixture is written by format 1 only"
        );
        let region = fixture_region();
        let file = encode_runs(&region);
        assert_eq!(decode_runs(&file), region);
        std::fs::write(&path, file).unwrap();
    }
    let file = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let region = decode_runs(&file);
    let mut r = Rig::new(Sim::with_region(14, &region, [false; SECTORS]), 256);
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    for k in 0..FIXTURE_KEYS {
        let want = (!FIXTURE_DELETED.contains(&k)).then(|| fixture_value(k, FIXTURE_ROUNDS - 1));
        assert_eq!(r.get(k), want, "key {k}");
    }
    let want: Vec<(String, u8, u32)> = FIXTURE_FILES
        .iter()
        .map(|&(n, _, len)| (n.to_owned(), STORE_FILE_SONG as u8, len as u32))
        .collect();
    assert_eq!(r.dir(), want);
    for (name, seed, len) in FIXTURE_FILES {
        assert_eq!(r.file(name), Some(fixture_file(seed, len)), "{name}");
    }
    let s = r.stats();
    assert_eq!(s.corrupt, 0);
    assert!(s.keys >= FIXTURE_KEYS + 3);
    r.maintain();
    assert_eq!(r.put_now(3, b"back"), STORE_OK);
    r.reset();
    r.mount();
    assert_eq!(r.get(3).as_deref(), Some(&b"back"[..]));
    r.assert_clean();
}

#[test]
fn random_bytes_never_crash_the_mount() {
    let mut rng = Rng::new(15);
    for i in 0..40 {
        let region = rng.bytes(HI - LO);
        let mut r = Rig::new(Sim::with_region(i, &region, [false; SECTORS]), 256);
        assert_eq!(r.mount(), STORE_STATE_FOREIGN);
        assert_eq!(r.sim.ops, 0);
    }
}

#[test]
fn damaged_regions_mount_and_stay_safe() {
    let mut base = fresh(16, 256);
    let mut rng = Rng::new(16);
    let mut written: BTreeMap<u32, Vec<Vec<u8>>> = BTreeMap::new();
    for _ in 0..400 {
        let k = rng.below(12) as u32;
        let n = rng.below(300) as usize;
        let v = rng.bytes(n);
        assert_eq!(base.put_now(k, &v), STORE_OK);
        written.entry(k).or_default().push(v);
        if rng.chance(5) {
            base.maintain();
        }
    }
    let region = base.sim.region().to_vec();
    let claimed: Vec<usize> = (0..SECTORS)
        .filter(|&s| rd32(&region, s * SECTOR + 28) == 0 && region[s * SECTOR] == b'S')
        .collect();
    for i in 0..300u64 {
        let mut damaged = region.clone();
        for _ in 0..1 + rng.below(40) {
            let s = claimed[rng.below(claimed.len() as u64) as usize];
            let at = match rng.below(4) {
                0 => rng.below(32) as usize,
                _ => 32 + rng.below((USE - 32) as u64) as usize,
            };
            damaged[s * SECTOR + at] ^= 1 << rng.below(8);
        }
        let mut r = Rig::new(Sim::with_region(i, &damaged, [false; SECTORS]), 256);
        let state = r.mount();
        if state != STORE_STATE_SIDOU {
            continue;
        }
        for k in 0..12 {
            if let Some(v) = r.get(k) {
                assert!(written[&k].contains(&v), "key {k}: a value never written");
            }
        }
        r.maintain();
        let _ = r.put_now(99, b"still");
        r.maintain();
        assert!(
            r.sim.violations.is_empty(),
            "damage {i}: {:#?}",
            r.sim.violations
        );
    }
}

const FILE_MAX: usize = STORE_FILE_MAX as usize;

fn content(seed: u64, len: usize) -> Vec<u8> {
    Rng::new(seed).bytes(len)
}

fn sectors_for(len: usize) -> u32 {
    len.div_ceil(STORE_FILE_SECTOR_DATA as usize) as u32
}

#[test]
fn a_file_round_trips_and_survives_a_power_cycle() {
    let mut r = fresh(20, 256);
    let data = content(20, 5000);
    assert_eq!(r.file_now("SONG 1", &data), STORE_OK);
    assert_eq!(r.file("SONG 1"), Some(data.clone()));
    assert_eq!(r.file("SONG 2"), None);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.file("SONG 1"), Some(data));
    assert_eq!(
        r.dir(),
        vec![("SONG 1".to_owned(), STORE_FILE_SONG as u8, 5000)]
    );
    assert_eq!(r.stats().corrupt, 0);
    r.assert_clean();
}

#[test]
fn a_file_claims_its_sectors_before_it_writes_a_byte() {
    let mut r = fresh(21, 256);
    for (i, len) in [0usize, 1, 1248, 1249, 3744, 3745, 7488, 20_000, FILE_MAX]
        .into_iter()
        .enumerate()
    {
        r.maintain();
        let (pool, ops) = (r.stats().pool, r.sim.ops);
        let head_room = {
            let h = usize::from(r.st.head);
            h != 0xFF && usize::from(r.st.sector[h].free) + 16 + 32 + 20 + 4 <= USE
        };
        assert_eq!(r.file_begin(&format!("F{i}"), len), STORE_OK, "{len} bytes");
        assert_eq!(r.sim.ops, ops, "begin writes nothing");
        assert_eq!(u32::from(r.st.f.nsec), sectors_for(len));
        let taken = pool - r.stats().pool;
        assert_eq!(
            taken,
            sectors_for(len) + u32::from(!head_room),
            "{len} bytes: the sectors and maybe a head for its FILE record"
        );
        let erases = r.sim.erases;
        assert_eq!(r.file_stream(&content(i as u64, len), 700, None), STORE_OK);
        assert_eq!(r.sim.erases, erases, "no erase between begin and end");
        assert_eq!(r.file(&format!("F{i}")), Some(content(i as u64, len)));
    }
    r.reset();
    r.mount();
    for (i, len) in [0usize, 1, 1248, 1249, 3744, 3745, 7488, 20_000, FILE_MAX]
        .into_iter()
        .enumerate()
    {
        assert_eq!(r.file(&format!("F{i}")), Some(content(i as u64, len)));
    }
    r.assert_clean();
}

#[test]
fn the_largest_file_takes_18_sectors_and_reads_back_at_any_offset() {
    let mut r = fresh(22, 64);
    let data = content(22, FILE_MAX);
    assert_eq!(r.file_now("BIG", &data), STORE_OK);
    let f = r.file_open("BIG").unwrap();
    assert_eq!((f.nsec, f.chunks, f.length as usize), (18, 53, FILE_MAX));
    assert_eq!(f.crc, crc32(&data));
    let mut rng = Rng::new(22);
    for _ in 0..3000 {
        let off = rng.below(FILE_MAX as u64 + 1) as usize;
        let n = rng.below(4000) as usize;
        let want = &data[off.min(FILE_MAX)..(off + n).min(FILE_MAX)];
        assert_eq!(r.file_read(&f, off, n).as_deref(), Some(want), "{off}+{n}");
    }
    assert_eq!(r.file_read(&f, FILE_MAX + 10, 5), Some(Vec::new()));
    r.reset();
    r.mount();
    assert_eq!(r.file("BIG"), Some(data));
}

#[test]
fn reads_at_every_offset_and_length() {
    let mut r = fresh(23, 256);
    let len = 5000;
    let data = content(23, len);
    assert_eq!(r.file_now("EVERY", &data), STORE_OK);
    let f = r.file_open("EVERY").unwrap();
    for off in 0..=len {
        for n in [0, 1, 2, 3, 4, 1247, 1248, 1249, 2500, len] {
            let want = &data[off..(off + n).min(len)];
            assert_eq!(r.file_read(&f, off, n).as_deref(), Some(want), "{off}+{n}");
        }
    }
}

#[test]
fn file_writes_take_offers_of_every_size_from_1_to_1000() {
    let mut r = fresh(24, 64);
    for offer in 1..=1000usize {
        let data = content(offer as u64, 1500 + offer * 3);
        let mut rc = r.file_begin("OFFERS", data.len());
        if rc == STORE_BUSY {
            r.maintain();
            rc = r.file_begin("OFFERS", data.len());
        }
        assert_eq!(rc, STORE_OK, "offer {offer}");
        assert_eq!(r.file_stream(&data, offer, None), STORE_OK, "offer {offer}");
        assert_eq!(r.file("OFFERS"), Some(data), "offer {offer}");
    }
    r.reset();
    r.mount();
    assert_eq!(r.dir().len(), 1);
    r.assert_clean();
}

#[test]
fn the_directory_is_in_name_order() {
    let mut r = fresh(25, 256);
    let names = ["ZED", "ALPHA", "M 2", "M 10", "!BANG", "SONG_9", "A"];
    for (i, n) in names.iter().enumerate() {
        assert_eq!(r.file_now(n, &content(i as u64, 100 * i)), STORE_OK);
    }
    let mut want: Vec<(String, u8, u32)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), STORE_FILE_SONG as u8, 100 * i as u32))
        .collect();
    want.sort();
    assert_eq!(r.dir(), want);
    assert_eq!(r.put_now(5, b"setting"), STORE_OK);
    assert_eq!(r.dir().len(), names.len());
    r.reset();
    r.mount();
    assert_eq!(r.dir(), want);
}

#[test]
fn names_are_the_fonts_upper_case() {
    let mut r = fresh(26, 256);
    for bad in [
        "",
        "lower",
        " LEAD",
        "SEVENTEEN CHARS!!",
        "TAB\tX",
        "TILDE~",
        "\u{7f}",
    ] {
        assert_eq!(r.file_begin(bad, 10), STORE_INVALID, "{bad:?}");
    }
    for good in ["A", "SIXTEEN CHARS 16", "@#$%^&*()_+-=[]?", "TRAILING "] {
        assert_eq!(r.file_now(good, b"x"), STORE_OK, "{good:?}");
        assert_eq!(r.file(good).as_deref(), Some(&b"x"[..]));
    }
    assert_eq!(
        unsafe { store_file_begin(&mut *r.st, c"X".as_ptr(), 0, 1) },
        STORE_INVALID,
        "type 0"
    );
    assert_eq!(r.file_begin("X", FILE_MAX + 1), STORE_INVALID);
}

#[test]
fn replacing_a_file_keeps_the_old_until_the_new_commits() {
    let mut r = fresh(27, 64);
    let (old, new) = (content(1, 9000), content(2, 4000));
    assert_eq!(r.file_now("SONG", &old), STORE_OK);
    let handle = r.file_open("SONG").unwrap();
    assert_eq!(r.file_begin("SONG", new.len()), STORE_OK);
    assert_eq!(r.file_write(&new[..200]), 200);
    r.settle();
    assert_eq!(
        r.file("SONG"),
        Some(old.clone()),
        "while the new one is written"
    );
    let _ = r.file_write(&new[200..256]);
    let mut at = 256;
    while at < new.len() {
        at += r.file_write(&new[at..]);
        r.poll();
    }
    assert_eq!(r.file("SONG"), Some(old.clone()), "before the end");
    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_OK);
    while r.file_result() == STORE_BUSY {
        r.poll();
    }
    assert_eq!(r.file_result(), STORE_OK);
    assert_eq!(r.file("SONG"), Some(new.clone()));
    assert_eq!(
        r.file_read(&handle, 0, 10),
        None,
        "an old handle reads nothing"
    );
    assert_eq!(r.dir().len(), 1);
    let live = r.stats().live;
    r.maintain();
    assert_eq!(r.stats().live, live);
    r.reset();
    r.mount();
    assert_eq!(r.file("SONG"), Some(new));
    r.assert_clean();
}

#[test]
fn deleting_renaming_aborting_and_ending_short() {
    let mut r = fresh(28, 256);
    assert_eq!(r.file_now("ONE", &content(1, 3000)), STORE_OK);
    assert_eq!(r.file_now("TWO", &content(2, 300)), STORE_OK);
    assert_eq!(r.file_rename("ONE", "TWO"), STORE_EXISTS);
    assert_eq!(r.file_rename("ONE", "ONE"), STORE_OK);
    assert_eq!(r.file_rename("NONE", "X"), STORE_NOT_FOUND);
    assert_eq!(r.file_rename("ONE", "THREE"), STORE_OK);
    r.settle();
    assert_eq!(r.file("ONE"), None);
    assert_eq!(r.file("THREE"), Some(content(1, 3000)));
    assert_eq!(r.file_delete("TWO"), STORE_OK);
    r.settle();
    assert_eq!(r.file("TWO"), None);
    assert_eq!(r.file_delete("TWO"), STORE_NOT_FOUND);
    r.maintain();
    let pool = r.stats().pool;
    assert_eq!(r.file_begin("HALF", 20_000), STORE_OK);
    assert_eq!(r.file_write(&content(3, 256)), 256);
    r.settle();
    unsafe { store_file_abort(&mut *r.st) };
    assert_eq!(r.file_result(), STORE_NOT_FOUND);
    assert_eq!(r.file("HALF"), None);
    assert_eq!(
        r.stats().pool + 1,
        pool,
        "the claimed sector became garbage, the rest came back"
    );
    assert_eq!(r.file_begin("SHORT", 100), STORE_OK);
    assert_eq!(r.file_write(&[1; 50]), 50);
    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_INVALID);
    assert_eq!(r.file_result(), STORE_INVALID);
    assert_eq!(r.file("SHORT"), None);
    r.reset();
    r.mount();
    assert_eq!(
        r.dir(),
        vec![("THREE".to_owned(), STORE_FILE_SONG as u8, 3000)]
    );
    r.assert_clean();
}

#[test]
fn small_writes_go_on_while_a_file_is_open_and_maintenance_waits() {
    let mut r = fresh(29, 64);
    let data = content(29, 12_000);
    assert_eq!(r.put_now(9, b"head"), STORE_OK);
    assert_eq!(r.file_begin("OPEN", data.len()), STORE_OK);
    assert!(!r.erase_wanted(), "maintenance waits for the file");
    assert_eq!(r.collect(), STORE_BUSY);
    assert_eq!(r.file_begin("SECOND", 10), STORE_BUSY, "one file at a time");
    let mut at = 0;
    let mut k = 0;
    while at < data.len() {
        at += r.file_write(&data[at..(at + 300).min(data.len())]);
        if !r.busy() && k < 30 {
            assert_eq!(r.put(k % 4, &[k as u8; 40]), STORE_OK);
            k += 1;
        }
        r.poll();
    }
    r.settle();
    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_OK);
    while r.file_result() == STORE_BUSY {
        r.poll();
    }
    assert!(k > 10);
    assert_eq!(r.file("OPEN"), Some(data.clone()));
    r.reset();
    r.mount();
    assert_eq!(r.file("OPEN"), Some(data));
    for i in 0..4 {
        assert!(r.get(i).is_some());
    }
    r.assert_clean();
}

const JOB_COLLECT: u8 = 2;

#[test]
fn a_file_begins_while_a_collection_waits_and_both_finish() {
    let mut phases = std::collections::BTreeSet::new();
    for stop in 0..16u64 {
        let mut r = fresh(31, 256);
        for k in 100..106u32 {
            assert_eq!(r.put_now(k, &[k as u8; 40]), STORE_OK);
        }
        for round in 0..3u8 {
            for k in 0..20u32 {
                assert_eq!(r.put_now(k, &[k as u8 ^ round; 150]), STORE_OK);
            }
        }
        assert_eq!(r.collect(), STORE_OK);
        r.sim.later = 50;
        let (mut idle, mut polls) = (0, 0);
        while r.st.job == JOB_COLLECT && idle < stop {
            r.poll();
            idle += u64::from(!r.busy());
            polls += 1;
            assert!(polls < 100_000, "the collection never ended");
        }
        assert!(!r.busy());
        r.sim.later = 0;
        let job = (r.st.job, r.st.job_sec, r.st.job_phase, r.st.job_off);
        if job.0 == JOB_COLLECT {
            phases.insert(job.2);
        }
        let data = content(31 + stop, 9_000);
        assert_eq!(r.file_begin("WAITED", data.len()), STORE_OK, "stop {stop}");
        assert!(!r.erase_wanted(), "maintenance waits for the file");
        assert_eq!(r.file_stream(&data, 256, None), STORE_OK, "stop {stop}");
        assert_eq!(
            (r.st.job, r.st.job_sec, r.st.job_phase, r.st.job_off),
            job,
            "stop {stop}: the collection waited for the file"
        );
        r.settle();
        r.maintain();
        assert_eq!(r.st.job, 0, "stop {stop}: the collection finished");
        for pass in 0..2 {
            for k in 0..20u32 {
                assert_eq!(
                    r.get(k),
                    Some(vec![k as u8 ^ 2; 150]),
                    "stop {stop}, pass {pass}: key {k}"
                );
            }
            for k in 100..106u32 {
                assert_eq!(r.get(k), Some(vec![k as u8; 40]), "stop {stop}: key {k}");
            }
            assert_eq!(r.file("WAITED"), Some(data.clone()), "stop {stop}");
            r.reset();
            r.mount();
        }
        r.assert_clean();
    }
    assert_eq!(
        phases.into_iter().collect::<Vec<_>>(),
        [0, 1, 2],
        "where the file began"
    );
}

#[test]
fn a_waiting_file_holds_maintenance_back_until_it_begins() {
    let mut r = fresh(32, 256);
    for k in 100..106u32 {
        assert_eq!(r.put_now(k, &[k as u8; 40]), STORE_OK);
    }
    for round in 0..3u8 {
        for k in 0..20u32 {
            assert_eq!(r.put_now(k, &[k as u8 ^ round; 150]), STORE_OK);
        }
    }
    assert_eq!(r.collect(), STORE_OK);
    for _ in 0..1_000 {
        if r.busy() {
            break;
        }
        r.poll();
    }
    assert!(r.busy(), "the collection copies a live record");
    assert_eq!(
        r.file_begin("WAITED", 5_000),
        STORE_BUSY,
        "the writer is busy"
    );
    unsafe { store_file_wait(&mut *r.st, 1) };
    let (erases, sec, phase) = (r.sim.erases, r.st.job_sec, r.st.job_phase);
    unsafe { store_quiet(&mut *r.st, 1) };
    let mut polls = 0;
    while r.poll() {
        polls += 1;
        assert!(polls < 10_000, "the record never committed");
    }
    assert!(!r.busy());
    assert!(!r.erase_wanted(), "no maintenance while a file waits");
    for _ in 0..100 {
        assert!(!r.poll(), "a step while a file waits");
    }
    assert_eq!(
        (r.st.job, r.st.job_sec, r.st.job_phase, r.sim.erases),
        (JOB_COLLECT, sec, phase, erases)
    );
    unsafe { store_quiet(&mut *r.st, 0) };
    let data = content(32, 5_000);
    assert_eq!(r.file_begin("WAITED", data.len()), STORE_OK);
    unsafe { store_file_wait(&mut *r.st, 0) };
    assert_eq!(r.file_stream(&data, 256, None), STORE_OK);
    r.maintain();
    assert_eq!(r.st.job, 0);
    assert_eq!(r.file("WAITED"), Some(data));
    for k in 0..20u32 {
        assert_eq!(r.get(k), Some(vec![k as u8 ^ 2; 150]));
    }
    r.assert_clean();
}

#[test]
fn the_pool_short_is_busy_and_writes_nothing_until_a_refill() {
    let mut r = fresh(30, 256);
    assert_eq!(r.file_now("FIRST", &content(1, FILE_MAX)), STORE_OK);
    let ops = r.sim.ops;
    assert_eq!(r.file_begin("SECOND", FILE_MAX), STORE_BUSY);
    assert_eq!(r.sim.ops, ops, "nothing written");
    r.maintain();
    assert_eq!(r.file_begin("SECOND", FILE_MAX), STORE_OK);
    assert_eq!(r.file_stream(&content(2, FILE_MAX), 256, None), STORE_OK);
    assert_eq!(r.file("SECOND"), Some(content(2, FILE_MAX)));
}

#[test]
fn a_file_larger_than_the_free_space_is_refused_before_anything_is_written() {
    let mut r = fresh(31, 256);
    let mut n = 0;
    loop {
        let data = content(n, 30_000);
        let ops = r.sim.ops;
        let rc = r.file_now(&format!("FILL {n}"), &data);
        if rc == STORE_FULL {
            assert_eq!(r.sim.ops, ops, "a refused file wrote something");
            break;
        }
        assert_eq!(rc, STORE_OK);
        n += 1;
        assert!(n < 10);
    }
    println!(
        "{n} files of 30,000 bytes fit; {} bytes free",
        r.stats().free
    );
    assert!(n >= 6, "only {n} files fit");
    assert!(r.stats().free < 30_000 + 1_000);
    assert_eq!(r.file_delete("FILL 0"), STORE_OK);
    r.settle();
    r.maintain();
    assert_eq!(r.file_now("AGAIN", &content(99, 30_000)), STORE_OK);
    r.reset();
    r.mount();
    for i in 1..n {
        assert_eq!(r.file(&format!("FILL {i}")), Some(content(i, 30_000)));
    }
    assert_eq!(r.file("AGAIN"), Some(content(99, 30_000)));
    r.assert_clean();
}

struct FileRun {
    rig: Rig,
    rng: Rng,
    files: BTreeMap<String, Vec<u8>>,
    values: BTreeMap<u32, Vec<u8>>,
    inflight: FileOp,
    forks: bool,
    forks_checked: u64,
    cuts: u64,
    drops: u64,
    seed: u64,
    max_len: usize,
}

#[derive(Clone, Debug)]
enum FileOp {
    Idle,
    Write(String, Vec<u8>),
    Delete(String),
    Rename(String, String),
    Put(u32, Vec<u8>),
}

const NAMES: [&str; 5] = ["A", "SONG 1", "SONG 2", "DUMP", "SIXTEEN CHARS 16"];

impl FileRun {
    fn new(seed: u64, forks: bool, max_len: usize, window: u32, faint: bool) -> Self {
        let mut rng = Rng::new(seed ^ 0xF11E);
        let mut rig = fresh(seed, window);
        rig.sim.log_forks = forks;
        set_tear(&mut rig, seed, faint);
        rig.sim.later = if rng.chance(30) { 20 } else { 0 };
        FileRun {
            rig,
            rng,
            files: BTreeMap::new(),
            values: BTreeMap::new(),
            inflight: FileOp::Idle,
            forks,
            forks_checked: 0,
            cuts: 0,
            drops: 0,
            seed,
            max_len,
        }
    }

    fn verify(&self, r: &Rig, what: &str) {
        let seed = self.seed;
        assert_eq!(r.stats().corrupt, 0, "seed {seed}: {what}: corruption seen");
        for name in NAMES {
            let got = r.file(name);
            let old = self.files.get(name).cloned();
            let ok = match &self.inflight {
                FileOp::Write(n, v) if n == name => got == old || got.as_ref() == Some(v),
                FileOp::Delete(n) if n == name => got == old || got.is_none(),
                FileOp::Rename(a, b) if a == name || b == name => true,
                _ => got == old,
            };
            assert!(
                ok,
                "seed {seed}: {what}: {name} reads {:?} bytes, had {:?} ({:?})",
                got.as_ref().map(|v| v.len()),
                old.as_ref().map(|v| v.len()),
                self.inflight_short()
            );
        }
        if let FileOp::Rename(a, b) = &self.inflight {
            let (ga, gb) = (r.file(a), r.file(b));
            let v = self.files.get(a).cloned();
            assert!(
                (ga == v && gb.is_none()) || (ga.is_none() && gb == v),
                "seed {seed}: {what}: a rename from {a} to {b} half done"
            );
        }
        for k in 0..4 {
            let got = r.get(k);
            let old = self.values.get(&k).cloned();
            let ok = match &self.inflight {
                FileOp::Put(key, v) if *key == k => got == old || got.as_ref() == Some(v),
                _ => got == old,
            };
            assert!(ok, "seed {seed}: {what}: key {k}");
        }
    }

    fn inflight_short(&self) -> String {
        match &self.inflight {
            FileOp::Write(n, v) => format!("writing {n}, {} bytes", v.len()),
            other => format!("{other:?}"),
        }
    }

    fn check_forks(&mut self) {
        let Some(replay) = self.rig.sim.replay(self.seed ^ self.forks_checked) else {
            return;
        };
        let tear = self.rig.sim.tear();
        for (cells, _) in replay {
            self.forks_checked += 1;
            let mut r = Rig::new(
                Sim::with_cells(self.forks_checked, cells, tear),
                self.rig.window,
            );
            assert_eq!(r.mount(), STORE_STATE_SIDOU, "seed {}", self.seed);
            self.verify(&r, "a forked cut");
            if self.forks_checked.is_multiple_of(32) {
                let files: Vec<Option<Vec<u8>>> = NAMES.iter().map(|n| r.file(n)).collect();
                let values: Vec<Option<Vec<u8>>> = (0..4).map(|k| r.get(k)).collect();
                r.maintain();
                assert_eq!(
                    r.file_now("AFTER", b"after"),
                    STORE_OK,
                    "seed {}",
                    self.seed
                );
                r.assert_clean();
                r.reset();
                assert_eq!(r.mount(), STORE_STATE_SIDOU, "seed {}", self.seed);
                for (n, want) in NAMES.iter().zip(&files) {
                    assert_eq!(
                        &r.file(n),
                        want,
                        "seed {}: {n} at the next mount",
                        self.seed
                    );
                }
                for (k, want) in values.iter().enumerate() {
                    assert_eq!(&r.get(k as u32), want, "seed {}: key {k}", self.seed);
                }
                assert_eq!(r.file("AFTER").as_deref(), Some(&b"after"[..]));
            }
        }
    }

    fn after(&mut self) {
        if self.forks {
            self.check_forks();
        }
        let op = std::mem::replace(&mut self.inflight, FileOp::Idle);
        if self.rig.sim.dead {
            self.inflight = op;
            self.cuts += 1;
            self.rig.sim.dead = false;
            self.rig.sim.cut_at = None;
            self.rig.reset();
            assert_eq!(self.rig.mount(), STORE_STATE_SIDOU, "seed {}", self.seed);
            self.verify(&self.rig, "a real cut");
            for name in NAMES {
                match self.rig.file(name) {
                    Some(v) => self.files.insert(name.to_owned(), v),
                    None => self.files.remove(name),
                };
            }
            for k in 0..4 {
                match self.rig.get(k) {
                    Some(v) => self.values.insert(k, v),
                    None => self.values.remove(&k),
                };
            }
            self.inflight = FileOp::Idle;
            return;
        }
        match op {
            FileOp::Write(n, v) => {
                self.files.insert(n, v);
            }
            FileOp::Delete(n) => {
                self.files.remove(&n);
            }
            FileOp::Rename(a, b) => {
                let v = self.files.remove(&a).unwrap();
                self.files.insert(b, v);
            }
            FileOp::Put(k, v) => {
                self.values.insert(k, v);
            }
            FileOp::Idle => {}
        }
    }

    fn maintain(&mut self) {
        self.rig.maintain();
        self.after();
    }

    fn drop_file(&mut self, data: &[u8]) {
        let upto = self.rng.below(data.len() as u64 + 1) as usize;
        let polls = self.rng.below(48);
        let mut at = 0;
        let mut n = 0;
        while (at < upto || n < polls) && !self.rig.sim.dead {
            if at < upto {
                at += self.rig.file_write(&data[at..upto]);
            }
            self.rig.poll();
            n += 1;
            assert!(
                n < 100_000,
                "seed {}: the file stopped taking bytes",
                self.seed
            );
        }
        if !self.rig.sim.dead {
            let short = at < data.len() && self.rng.chance(50);
            if short {
                assert_eq!(unsafe { store_file_end(&mut *self.rig.st) }, STORE_INVALID);
            } else {
                unsafe { store_file_abort(&mut *self.rig.st) };
            }
            let want = if short {
                STORE_INVALID
            } else {
                STORE_NOT_FOUND
            };
            assert_eq!(self.rig.file_result(), want, "seed {}", self.seed);
            self.drops += 1;
        }
        self.after();
    }

    fn step(&mut self) {
        if self.rig.sim.cut_at.is_none() && self.rng.chance(4) {
            self.rig.sim.cut_at = Some(self.rig.sim.ops + self.rng.below(300));
        }
        let name = NAMES[self.rng.below(NAMES.len() as u64) as usize].to_owned();
        match self.rng.below(100) {
            0..=34 => {
                let len = match self.rng.below(10) {
                    0 => 0,
                    1 => self.max_len,
                    2..=5 => self.rng.below(3000) as usize,
                    _ => self.rng.below(self.max_len as u64 + 1) as usize,
                };
                let data = self.rng.bytes(len);
                let mut rc = self.rig.file_begin(&name, len);
                for _ in 0..3 {
                    if rc != STORE_BUSY {
                        break;
                    }
                    self.maintain();
                    rc = self.rig.file_begin(&name, len);
                }
                if rc == STORE_FULL {
                    return;
                }
                assert_eq!(rc, STORE_OK, "seed {}", self.seed);
                if self.rng.chance(15) {
                    self.drop_file(&data);
                    return;
                }
                self.inflight = FileOp::Write(name, data.clone());
                let erases = self.rig.sim.erases;
                let mut rng = self.rng.clone();
                let rc = self.rig.file_stream(&data, 1000, Some(&mut rng));
                self.rng = rng;
                assert_eq!(
                    self.rig.sim.erases, erases,
                    "seed {}: an erase in a file",
                    self.seed
                );
                assert!(
                    self.rig.sim.dead || rc == STORE_OK,
                    "seed {}: {rc}",
                    self.seed
                );
                self.after();
            }
            35..=44 => {
                if self.rig.file_delete(&name) == STORE_OK {
                    self.inflight = FileOp::Delete(name);
                    self.rig.settle();
                }
                self.after();
            }
            45..=54 => {
                let to = NAMES[self.rng.below(NAMES.len() as u64) as usize].to_owned();
                let rc = self.rig.file_rename(&name, &to);
                if rc == STORE_OK && name != to {
                    self.inflight = FileOp::Rename(name, to);
                    self.rig.settle();
                } else {
                    self.rig.settle();
                }
                self.after();
            }
            55..=79 => {
                let k = self.rng.below(4) as u32;
                let n = self.rng.below(200) as usize;
                let v = self.rng.bytes(n);
                if self.rig.put(k, &v) == STORE_OK {
                    self.inflight = FileOp::Put(k, v);
                    self.rig.settle();
                }
                self.after();
            }
            80..=87 => {
                if self.rig.collect() == STORE_OK {
                    self.rig.settle();
                }
                self.after();
            }
            _ => self.maintain(),
        }
        assert!(
            self.rig.sim.violations.is_empty(),
            "seed {}: {:#?}",
            self.seed,
            self.rig.sim.violations
        );
    }

    fn end(&mut self) {
        self.rig.sim.cut_at = None;
        self.rig.sim.log_forks = false;
        self.maintain();
        self.rig.reset();
        assert_eq!(self.rig.mount(), STORE_STATE_SIDOU);
        self.verify(&self.rig, "the end");
        self.rig.assert_clean();
        if self.rig.window >= 16 {
            assert!(u32::from(self.rig.sim.page_progs_max) <= STORE_PAGE_PROGS);
        }
    }
}

fn file_power_loss(
    seed: u64,
    steps: u32,
    forks: bool,
    max_len: usize,
    window: u32,
    faint: bool,
) -> FileRun {
    let mut run = FileRun::new(seed, forks, max_len, window, faint);
    for _ in 0..steps {
        run.step();
    }
    run.end();
    run
}

#[test]
fn files_survive_a_cut_at_every_operation_old_or_new_never_neither_never_a_mix() {
    let (mut forks, mut cuts, mut drops) = (0, 0, 0);
    for (seed, window) in WINDOWS.into_iter().enumerate() {
        let max_len = (window as usize * 40).clamp(300, 6_000);
        let run = file_power_loss(seed as u64, 60, true, max_len, window, seed % 2 == 0);
        forks += run.forks_checked;
        cuts += run.cuts;
        drops += run.drops;
    }
    println!("{forks} forked cuts and {cuts} real cuts checked; {drops} files dropped");
    assert!(forks > 5_000 && cuts > 3 && drops > 5);
}

#[test]
fn files_under_real_cuts_over_long_runs() {
    let (mut cuts, mut drops) = (0, 0);
    for seed in 100..112 {
        let window = WINDOWS[seed as usize % WINDOWS.len()];
        let max_len = if window < 8 { 8_000 } else { FILE_MAX };
        let run = file_power_loss(seed, 250, false, max_len, window, seed % 2 == 1);
        cuts += run.cuts;
        drops += run.drops;
    }
    println!("{cuts} real cuts checked; {drops} files dropped");
    assert!(cuts > 50 && drops > 20);
}

#[test]
#[ignore]
fn files_power_loss_long() {
    let (mut forks, mut cuts) = (0, 0);
    for seed in LONG_FILES_FORKED.chain(LONG_FILES_REAL) {
        let (f, c) = long_files_seed(seed, false);
        forks += f;
        cuts += c;
    }
    println!("{forks} forked cuts and {cuts} real cuts checked");
}

#[test]
fn an_open_files_fresh_key_is_kept_for_it() {
    let mut r = fresh(40, 256);
    for k in 0..STORE_KEYS_MAX - 1 {
        assert_eq!(r.put_now(k, &k.to_le_bytes()), STORE_OK);
    }
    assert_eq!(r.file_begin("SONG", 10), STORE_OK);
    let ops = r.sim.ops;
    assert_eq!(
        r.put(1000, b"new"),
        STORE_FULL,
        "the file holds the last slot"
    );
    assert_eq!(r.sim.ops, ops);
    assert_eq!(
        r.put(3, b"an old key"),
        STORE_OK,
        "an indexed key still writes"
    );
    r.settle();
    assert_eq!(r.file_stream(&[5; 10], 10, None), STORE_OK);
    assert_eq!(r.stats().keys, STORE_KEYS_MAX);
    r.reset();
    r.mount();
    let s = r.stats();
    assert_eq!((s.keys, s.corrupt), (STORE_KEYS_MAX, 0));
    assert_eq!(r.file("SONG").as_deref(), Some(&[5u8; 10][..]));
    assert_eq!(r.get(3).as_deref(), Some(&b"an old key"[..]));
    r.assert_clean();
}

#[test]
fn a_file_dropped_mid_claim_leaves_its_sector_to_be_erased() {
    for window in [1u32, 4, 8, 16] {
        let pieces = (STORE_HEADER as usize).div_ceil(window as usize);
        for polls in 1..pieces {
            for short in [false, true] {
                let what = format!("window {window}, {polls} pieces, short {short}");
                let mut r = fresh(50 + u64::from(window), window);
                assert_eq!(r.file_begin("A", 100), STORE_OK);
                assert_eq!(r.file_write(&[7; 60]), 60);
                for _ in 0..polls {
                    r.poll();
                }
                if r.st.f.pdone == 0 {
                    continue;
                }
                if short {
                    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_INVALID);
                } else {
                    unsafe { store_file_abort(&mut *r.st) };
                }
                r.maintain();
                for i in 0..12u64 {
                    assert_eq!(
                        r.file_now(&format!("B{i}"), &content(i, 2000)),
                        STORE_OK,
                        "{what}"
                    );
                    assert_eq!(r.put_now(i as u32, &[i as u8; 300]), STORE_OK, "{what}");
                }
                r.assert_clean();
                r.reset();
                assert_eq!(r.mount(), STORE_STATE_SIDOU);
                for i in 0..12u64 {
                    assert_eq!(r.file(&format!("B{i}")), Some(content(i, 2000)), "{what}");
                }
                assert_eq!(r.file("A"), None);
                assert_eq!(r.stats().corrupt, 0, "{what}");
            }
        }
    }
}

fn file_idle(r: &mut Rig, ms: u32) {
    unsafe { store_file_idle(&mut *r.st, ms) };
}

#[test]
fn an_open_file_whose_sender_goes_quiet_times_out() {
    let mut r = fresh(41, 64);
    assert_eq!(r.put_now(1, b"x"), STORE_OK);
    assert_eq!(r.file_begin("SYSEX", 5000), STORE_OK);
    let data = content(41, 5000);
    assert_eq!(r.file_write(&data[..300]), 256);
    file_idle(&mut r, STORE_FILE_IDLE_MS * 2);
    assert_eq!(r.file_result(), STORE_BUSY);
    r.settle();
    assert!(!r.erase_wanted(), "maintenance waits for the file");
    file_idle(&mut r, STORE_FILE_IDLE_MS - 1);
    assert_eq!(r.file_result(), STORE_BUSY);
    assert_eq!(
        r.file_write(&data[256..300]),
        44,
        "a byte starts the time again"
    );
    file_idle(&mut r, STORE_FILE_IDLE_MS - 1);
    assert_eq!(r.file_result(), STORE_BUSY);
    file_idle(&mut r, 1);
    assert_eq!(r.file_result(), STORE_TIMEOUT);
    assert_eq!(r.file_write(&data[300..400]), 0, "the file is gone");
    assert!(r.erase_wanted(), "maintenance goes on");
    r.maintain();
    assert_eq!(r.file("SYSEX"), None);
    assert_eq!(r.file_begin("SYSEX", 300), STORE_OK);
    assert_eq!(r.file_write(&data[..300]), 256);
    r.settle();
    assert_eq!(r.file_write(&data[256..300]), 44);
    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_OK);
    file_idle(&mut r, STORE_FILE_IDLE_MS * 10);
    r.settle();
    assert_eq!(r.file_result(), STORE_OK);
    assert_eq!(r.file("SYSEX"), Some(data[..300].to_vec()));
    r.assert_clean();
}

const SECTORS_OF_A_NEARLY_LARGEST_SONG: usize = 17;

#[test]
fn replacing_a_file_counts_the_space_the_old_one_frees() {
    let mut r = fresh(42, 256);
    let near = SECTORS_OF_A_NEARLY_LARGEST_SONG * STORE_FILE_SECTOR_DATA as usize;
    for (i, n) in ["SONG 1", "SONG 2", "SONG 3"].iter().enumerate() {
        r.maintain();
        assert_eq!(r.file_now(n, &content(i as u64, near)), STORE_OK, "{n}");
    }
    r.maintain();
    let again = content(9, FILE_MAX);
    assert_eq!(r.file_now("SONG 2", &again), STORE_OK);
    assert_eq!(r.file("SONG 2"), Some(again.clone()));
    let free = r.stats().free;
    assert_eq!(
        r.file_now("SONG 4", &content(4, near)),
        STORE_FULL,
        "a fourth does not fit ({free} bytes free)"
    );
    r.reset();
    r.mount();
    assert_eq!(r.file("SONG 1"), Some(content(0, near)));
    assert_eq!(r.file("SONG 2"), Some(again));
    assert_eq!(r.file("SONG 3"), Some(content(2, near)));
    r.assert_clean();
}

#[test]
fn a_format_with_a_file_open_ends_it_as_not_found() {
    let mut r = fresh(43, 256);
    assert_eq!(r.file_begin("OPEN", 100), STORE_OK);
    unsafe { store_format(&mut *r.st) };
    assert_eq!(r.file_result(), STORE_NOT_FOUND);
}

fn newest_commit_word_of(region: &[u8], key: u32, typ: u8) -> usize {
    let mut best = (0u32, 0usize);
    for s in sectors_holding(region, key, typ) {
        let h = &region[s * SECTOR..(s + 1) * SECTOR];
        for off in record_offsets(h) {
            let size = record_len(h, off);
            if h[off] == typ && rd32(h, off + 4) == key && rd32(h, off + 8) >= best.0 {
                best = (rd32(h, off + 8), s * SECTOR + off + size - COMMIT_WORD_LEN);
            }
        }
    }
    assert_ne!(best.1, 0, "no record of key {key}");
    best.1
}

#[test]
fn a_faint_commit_word_loses_nothing_written_after_it() {
    for reads in [0xFFu8, 0x5A] {
        let mut r = fresh(60, 256);
        assert_eq!(r.put_now(1, b"old"), STORE_OK);
        assert_eq!(r.put_now(1, b"new"), STORE_OK);
        let c = newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD);
        r.sim.set_faint(c, COMMIT_WORD_LEN, 0x00);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"new"[..]));
        for k in 2..6 {
            assert_eq!(r.put_now(k, b"after"), STORE_OK);
        }
        r.sim.faint_reads(reads);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        for k in 2..6 {
            assert_eq!(
                r.get(k).as_deref(),
                Some(&b"after"[..]),
                "reads {reads:#x}: key {k}"
            );
        }
        let one = r.get(1);
        if reads == 0xFF {
            assert!(one.as_deref() == Some(b"old") || one.as_deref() == Some(b"new"));
        } else {
            assert_eq!(
                one.as_deref(),
                Some(&b"new"[..]),
                "a half-read commit word still commits"
            );
        }
        assert_eq!(r.stats().corrupt, 0);
        r.assert_clean();
    }
}

#[test]
fn a_faint_erase_header_is_programmed_again_before_its_sector_takes_records() {
    let mut r = fresh(61, 256);
    let region = r.sim.region().to_vec();
    for s in 0..SECTORS {
        if region[s * SECTOR..s * SECTOR + 8] == *b"SIDOUSTR" {
            r.sim.set_faint(s * SECTOR, 20, 0x00);
        }
    }
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    let mut rng = Rng::new(61);
    let values: Vec<Vec<u8>> = (0..24).map(|_| rng.bytes(700)).collect();
    for (k, v) in values.iter().enumerate() {
        assert_eq!(r.put_now(k as u32 % 12, v), STORE_OK);
    }
    assert_eq!(r.file_now("SONG", &content(61, 9000)), STORE_OK);
    r.sim.faint_reads(0xFF);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    for k in 0..12 {
        assert_eq!(r.get(k).as_ref(), Some(&values[k as usize + 12]), "key {k}");
    }
    assert_eq!(r.file("SONG"), Some(content(61, 9000)));
    r.assert_clean();
}

#[test]
fn a_faint_claim_loses_nothing_written_after_it() {
    for reads in [0xFFu8, 0x3C] {
        let mut r = fresh(62, 256);
        assert_eq!(r.put_now(1, b"one"), STORE_OK);
        let region = r.sim.region().to_vec();
        let x = (0..SECTORS)
            .rev()
            .find(|&s| {
                region[s * SECTOR..s * SECTOR + 8] == *b"SIDOUSTR"
                    && region[s * SECTOR + 20..s * SECTOR + 32]
                        .iter()
                        .all(|&b| b == 0xFF)
            })
            .unwrap();
        let mut claim = region[x * SECTOR..x * SECTOR + 24].to_vec();
        claim[20..24].copy_from_slice(&0x0100_0000u32.to_le_bytes());
        let crc = crc32(&claim);
        let mut p = claim[20..24].to_vec();
        p.extend_from_slice(&crc.to_le_bytes());
        p.extend_from_slice(&[0; 4]);
        r.sim.cells.prog(x * SECTOR + 20, &p);
        r.sim.set_faint(x * SECTOR + 28, 4, 0x00);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        for k in 2..6 {
            assert_eq!(r.put_now(k, b"after"), STORE_OK);
        }
        r.sim.faint_reads(reads);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        for k in 1..6 {
            assert!(r.get(k).is_some(), "reads {reads:#x}: key {k}");
        }
        r.maintain();
        r.assert_clean();
    }
}

#[test]
fn a_mounts_polls_stay_within_a_slice() {
    let slice =
        STORE_MOUNT_SLICE.max(STORE_REC_HEADER + 64 + 4 * (12 + STORE_CHUNK) + STORE_KEYS_MAX + 24);
    let mut regions: Vec<(String, Vec<u8>)> = Vec::new();
    regions.push(("blank".into(), vec![0xFF; HI - LO]));
    regions.push((
        "the v1 fixture".into(),
        decode_runs(&std::fs::read(fixture_path()).unwrap()),
    ));
    let mut rng = Rng::new(70);
    regions.push(("random".into(), rng.bytes(HI - LO)));
    regions.push(("foreign".into(), foreign_region(70)));
    let mut big = fresh(71, 256);
    let mut k = 0;
    while big.put_now(k % 40, &rng.bytes(VALUE_MAX)) == STORE_OK && k < 400 {
        k += 1;
    }
    regions.push(("the largest values".into(), big.sim.region().to_vec()));
    let mut tiny = fresh(72, 256);
    for i in 0..3000 {
        assert_eq!(tiny.put_now(i % 40, b""), STORE_OK);
        if i % 500 == 0 {
            tiny.maintain();
        }
    }
    regions.push(("empty values".into(), tiny.sim.region().to_vec()));
    let mut files = fresh(73, 256);
    for i in 0..2 {
        assert_eq!(
            files.file_now(&format!("F{i}"), &content(i, FILE_MAX)),
            STORE_OK
        );
        files.maintain();
    }
    regions.push(("the largest files".into(), files.sim.region().to_vec()));
    let mut most = (0, 0);
    for (what, region) in &regions {
        let mut r = Rig::new(Sim::with_region(74, region, [false; SECTORS]), 256);
        r.mount();
        let (slice_work, finish) = r.mount_work;
        println!("{what}: {slice_work} a slice at most, {finish} to finish");
        assert!(slice_work <= slice, "{what}: a slice of {slice_work}");
        assert!(finish <= STORE_POLL_WORK, "{what}: a finish of {finish}");
        most = (most.0.max(slice_work), most.1.max(finish));
    }
    println!("mount polls: {} at most, {} to finish", most.0, most.1);
}

#[test]
fn a_page_takes_at_most_store_page_progs_programs_between_erases() {
    for window in [16u32, 64, 256] {
        let mut r = fresh(80 + u64::from(window), window);
        let mut rng = Rng::new(80);
        for i in 0..6_000u32 {
            let k = rng.below(40) as u32;
            if rng.chance(15) {
                assert_eq!(r.delete_now(k), STORE_OK);
            } else {
                let n = 1 + rng.below(8) as usize;
                assert_eq!(r.put_now(k, &rng.bytes(n)), STORE_OK);
            }
            if i % 700 == 0 {
                assert_eq!(r.file_now("F", &content(u64::from(i), 9_000)), STORE_OK);
                r.maintain();
            }
        }
        println!(
            "window {window}: {} programs in a page at most",
            r.sim.page_progs_max
        );
        assert!(u32::from(r.sim.page_progs_max) <= STORE_PAGE_PROGS);
        assert!(u32::from(r.sim.page_progs_max) >= STORE_PAGE_PROGS - 6);
        r.assert_clean();
    }
}

#[test]
fn a_cut_during_a_blank_regions_first_erase_can_mount_foreign() {
    let mut foreign = 0;
    for seed in 0..16 {
        let mut r = Rig::new(Sim::new(90 + seed), 256);
        r.sim.faint = true;
        assert_eq!(r.mount(), STORE_STATE_BLANK);
        r.sim.cut_at = Some(0);
        r.maintain();
        assert!(r.sim.dead);
        r.sim.dead = false;
        r.sim.cut_at = None;
        r.reset();
        let state = r.mount();
        assert!(
            state == STORE_STATE_BLANK || state == STORE_STATE_FOREIGN,
            "seed {seed}: {state}"
        );
        if state == STORE_STATE_FOREIGN {
            foreign += 1;
            let ops = r.sim.ops;
            r.maintain();
            assert_eq!(r.put(1, b"x"), STORE_FOREIGN);
            assert_eq!(r.sim.ops, ops, "nothing written");
            unsafe { store_format(&mut *r.st) };
        }
        r.maintain();
        assert_eq!(r.put_now(1, b"x"), STORE_OK);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"x"[..]));
        r.assert_clean();
    }
    assert!(foreign > 0, "no cut left the region foreign");
}

#[test]
fn a_commit_word_is_one_program_at_any_window() {
    for window in [1u32, 2, 3] {
        let mut r = fresh(95, window);
        r.sim.log_forks = true;
        assert_eq!(r.put_now(1, b"abcdefg"), STORE_OK);
        let replay = r.sim.replay(0).unwrap();
        let sizes: Vec<usize> = replay
            .ops
            .as_slice()
            .iter()
            .map(|op| match op {
                Op::Prog(_, data) => data.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(sizes.last(), Some(&4), "window {window}: {sizes:?}");
        assert!(sizes[..sizes.len() - 1]
            .iter()
            .all(|&n| n <= window as usize));
    }
}

#[test]
fn many_short_boots_of_writes_never_refuse_as_full_or_lose_a_value() {
    let mut r = fresh(77, 256);
    let mut rng = Rng::new(77);
    let mut model: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let (mut busy, mut full) = (0, 0);
    let song = content(5, 40_000);
    assert_eq!(r.file_now("BIG", &song), STORE_OK);
    for boot in 0..3000 {
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        if boot % 5 != 0 {
            r.maintain();
        }
        let n = 1 + rng.below(2);
        for _ in 0..n {
            let k = rng.below(40) as u32;
            let n = if rng.chance(30) {
                1000 + rng.below(249) as usize
            } else {
                rng.below(64) as usize
            };
            let v = rng.bytes(n);
            match r.put(k, &v) {
                STORE_OK => {
                    r.settle();
                    model.insert(k, v);
                }
                STORE_BUSY => busy += 1,
                STORE_FULL => full += 1,
                e => panic!("{e}"),
            }
        }
    }
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    for (k, v) in &model {
        assert_eq!(r.get(*k).as_ref(), Some(v));
    }
    assert_eq!(r.file("BIG"), Some(song));
    let st = r.stats();
    eprintln!(
        "busy {busy} full {full} live {} pool {} dirty {} claimed {} erases {}..{} total_erases {}",
        st.live, st.pool, st.dirty, st.claimed, st.erases_min, st.erases_max, r.sim.erases
    );
    assert_eq!(full, 0);
}

fn writes_near_capacity_are_never_refused_as_full(boot_between: bool) {
    let mut r = fresh(78, 256);
    let mut files = 0;
    let mut rng = Rng::new(78);
    while files < 7 {
        if r.file_now(&format!("F{files}"), &content(files as u64, 18_000)) != STORE_OK {
            break;
        }
        files += 1;
        r.maintain();
    }
    let (mut busy, mut full, mut ok) = (0, 0, 0);
    let mut minfull = u32::MAX;
    for boot in 0..400u32 {
        if boot_between {
            r.reset();
            assert_eq!(r.mount(), STORE_STATE_SIDOU);
        }
        r.maintain();
        for j in 0..2u32 {
            let k = (boot * 2 + j) % 40;
            let v = rng.bytes(1248);
            match r.put(k, &v) {
                STORE_OK => {
                    r.settle();
                    ok += 1;
                }
                STORE_BUSY => busy += 1,
                STORE_FULL => {
                    full += 1;
                    minfull = minfull.min(r.stats().live);
                }
                e => panic!("{e}"),
            }
        }
    }
    let st = r.stats();
    eprintln!("files {files} ok {ok} busy {busy} full {full} (lowest live when full {minfull}, capacity {}) live {} pool {} claimed {}", STORE_CAPACITY, st.live, st.pool, st.claimed);
    assert_eq!(
        full,
        0,
        "the store refused writes {} under its capacity",
        STORE_CAPACITY - minfull.min(STORE_CAPACITY)
    );
}

#[test]
fn writes_near_capacity_with_a_boot_between_are_never_refused_as_full() {
    writes_near_capacity_are_never_refused_as_full(true);
}

#[test]
fn writes_near_capacity_without_boots_are_never_refused_as_full() {
    writes_near_capacity_are_never_refused_as_full(false);
}

#[test]
fn a_faint_commit_word_then_collections_keep_the_old_value_or_the_new() {
    let mut r = fresh(60, 256);
    assert_eq!(r.put_now(1, b"old"), STORE_OK);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    r.maintain();
    assert_eq!(r.put_now(1, b"new"), STORE_OK);
    let c = newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD);
    r.sim.set_faint(c, COMMIT_WORD_LEN, 0x00);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(1).as_deref(), Some(&b"new"[..]));
    let before = r.sim.reclaims;
    r.maintain();
    let st = r.stats();
    eprintln!("pool {} dirty {} claimed {}", st.pool, st.dirty, st.claimed);
    let v = r.collect();
    eprintln!("collect -> {v}");
    if v == STORE_OK {
        r.maintain();
    }
    eprintln!(
        "sectors with key1: {:?}",
        sectors_holding(r.sim.region(), 1, VALUE_RECORD)
    );
    eprintln!("reclaims {}", r.sim.reclaims - before);
    r.sim.faint_reads(0xFF);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    let one = r.get(1);
    eprintln!(
        "key 1 after: {:?}",
        one.as_deref()
            .map(|v| String::from_utf8_lossy(v).to_string())
    );
    assert!(
        one.as_deref() == Some(b"old") || one.as_deref() == Some(b"new"),
        "lost: {one:?}"
    );
}

#[test]
fn a_faint_file_commit_word_then_collections_keep_the_old_file_or_the_new() {
    let mut r = fresh(61, 256);
    assert_eq!(r.file_now("SONG", &content(1, 9000)), STORE_OK);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    r.maintain();
    assert_eq!(r.file_now("SONG", &content(2, 9000)), STORE_OK);
    let key = r.file_open("SONG").unwrap().key;
    let region = r.sim.region().to_vec();
    let commit = newest_commit_word_of(&region, key, STORE_TYPE_FILE as u8);
    r.sim.set_faint(commit, COMMIT_WORD_LEN, 0x00);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.file("SONG"), Some(content(2, 9000)));
    r.maintain();
    for _ in 0..4 {
        if r.collect() == STORE_OK {
            r.maintain();
        }
    }
    r.sim.faint_reads(0xFF);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    let got = r.file("SONG");
    eprintln!(
        "SONG after: {:?} corrupt {}",
        got.as_ref().map(|v| v == &content(1, 9000)),
        r.stats().corrupt
    );
    assert!(
        got == Some(content(1, 9000)) || got == Some(content(2, 9000)),
        "song lost"
    );
}

#[test]
fn hook_errors_never_cost_a_committed_value() {
    let (mut failed, mut either) = (0, 0);
    for (seed, window) in WINDOWS.into_iter().enumerate() {
        let seed = 100 + seed as u64;
        let mut r = fresh(seed, window);
        r.sim.faint = seed.is_multiple_of(2);
        r.sim.pulse = r.sim.faint;
        r.sim.fail = 3;
        let mut rng = Rng::new(seed);
        let mut values: BTreeMap<u32, Vec<Option<Vec<u8>>>> = BTreeMap::new();
        let mut files: BTreeMap<&str, Vec<Option<Vec<u8>>>> = BTreeMap::new();
        for step in 0..300 {
            let errors = r.stats().errors;
            match rng.below(10) {
                0..=4 => {
                    let k = rng.below(8) as u32;
                    let n = rng.below(if window < 8 { 80 } else { 600 }) as usize;
                    let v = rng.bytes(n);
                    let rc = r.put_now(k, &v);
                    let now = values.entry(k).or_insert_with(|| vec![None]);
                    if rc == STORE_OK {
                        if r.stats().errors > errors {
                            now.push(Some(v));
                            either += 1;
                        } else {
                            *now = vec![Some(v)];
                        }
                    }
                }
                5 => {
                    let k = rng.below(8) as u32;
                    if r.delete_now(k) == STORE_OK {
                        let now = values.entry(k).or_insert_with(|| vec![None]);
                        if r.stats().errors > errors {
                            now.push(None);
                        } else {
                            *now = vec![None];
                        }
                    }
                }
                6 | 7 => {
                    let name = ["A", "B"][rng.below(2) as usize];
                    let len = rng.below(if window < 8 { 600 } else { 9_000 }) as usize;
                    let data = rng.bytes(len);
                    let rc = r.file_now(name, &data);
                    let now = files.entry(name).or_insert_with(|| vec![None]);
                    if r.stats().errors > errors && (rc == STORE_OK || rc == STORE_ERROR) {
                        now.push(Some(data));
                    } else if rc == STORE_OK {
                        *now = vec![Some(data)];
                    }
                }
                8 => {
                    if r.collect() == STORE_OK {
                        r.settle();
                    }
                }
                _ => r.maintain(),
            }
            assert!(
                r.sim.violations.is_empty(),
                "seed {seed}: {:#?}",
                r.sim.violations
            );
            if step % 50 == 49 {
                r.reset();
                assert_eq!(r.mount(), STORE_STATE_SIDOU, "seed {seed}");
                for (k, allowed) in values.iter_mut() {
                    let got = r.get(*k);
                    assert!(allowed.contains(&got), "seed {seed}: key {k}");
                    *allowed = vec![got];
                }
                for (n, allowed) in files.iter_mut() {
                    let got = r.file(n);
                    assert!(allowed.contains(&got), "seed {seed}: file {n}");
                    *allowed = vec![got];
                }
                assert_eq!(r.stats().corrupt, 0, "seed {seed}");
            }
        }
        r.sim.fail = 0;
        r.maintain();
        r.assert_clean();
        failed += r.sim.failed;
    }
    println!("{failed} operations failed, {either} writes left either way");
    assert!(failed > 300 && either > 10);
}

#[test]
fn the_interrupted_write_completes_and_stays() {
    for reads in [0xFFu8, 0x00, 0x5A] {
        let mut r = fresh(63, 256);
        assert_eq!(r.put_now(1, b"old"), STORE_OK);
        assert_eq!(r.put_now(1, b"cut"), STORE_OK);
        let c = newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD);
        r.sim.set_faint(c, COMMIT_WORD_LEN, reads);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"cut"[..]), "reads {reads:#x}");
        assert_eq!(r.put(2, b"next"), STORE_BUSY, "recovery first");
        assert_eq!(r.put_now(2, b"next"), STORE_OK);
        assert_ne!(
            newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD),
            c,
            "copied forward"
        );
        r.maintain();
        while r.collect() == STORE_OK {
            r.maintain();
        }
        for faint in [0xFF, 0x00] {
            r.sim.faint_reads(faint);
            r.reset();
            assert_eq!(r.mount(), STORE_STATE_SIDOU);
            assert_eq!(r.get(1).as_deref(), Some(&b"cut"[..]));
            assert_eq!(r.get(2).as_deref(), Some(&b"next"[..]));
            assert_eq!(r.stats().corrupt, 0);
        }
        r.assert_clean();
    }
    let mut r = fresh(64, 256);
    assert_eq!(r.put_now(1, b"old"), STORE_OK);
    assert_eq!(r.put_now(1, b"never"), STORE_OK);
    assert_eq!(r.put_now(3, b"the newest"), STORE_OK);
    let c = newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD);
    r.reset();
    r.mount();
    assert_eq!(r.put_now(2, b"newer"), STORE_OK);
    r.sim.cells.mem[c..c + 4].fill(0xFF);
    r.sim.cells.meant[c..c + 4].fill(0xFF);
    r.sim.mem[LO + c..LO + c + 4].fill(0xFF);
    r.reset();
    r.mount();
    assert_eq!(r.get(1).as_deref(), Some(&b"old"[..]));
    assert_eq!(r.get(2).as_deref(), Some(&b"newer"[..]));
}

#[test]
fn a_completed_write_that_overflows_the_index_is_the_one_left_out() {
    for first in [false, true] {
        let mut r = fresh(65, 256);
        for k in 0..STORE_KEYS_MAX {
            assert_eq!(r.put_now(k, &k.to_le_bytes()), STORE_OK);
        }
        let region = r.sim.region().to_vec();
        let claimed: Vec<usize> = (0..SECTORS)
            .filter(|&s| rd32(&region, s * SECTOR + 28) == 0 && region[s * SECTOR] == b'S')
            .collect();
        let sec = if first {
            claimed[0]
        } else {
            *claimed.last().unwrap()
        };
        let mut off = SECTOR_HEADER_LEN;
        let h = &region[sec * SECTOR..];
        while h[off] != 0xFF {
            off += record_len(h, off);
        }
        let mut rec = vec![1u8, 0, 4, 0];
        rec.extend_from_slice(&5000u32.to_le_bytes());
        rec.extend_from_slice(&1_000_000u32.to_le_bytes());
        let mut covered = rec.clone();
        covered.extend_from_slice(b"over");
        rec.extend_from_slice(&crc32(&covered).to_le_bytes());
        rec.extend_from_slice(b"over");
        r.sim.cells.prog(sec * SECTOR + off, &rec);
        r.sim.mem[LO..HI].copy_from_slice(&r.sim.cells.mem);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        let s = r.stats();
        assert_eq!((s.keys, s.corrupt), (STORE_KEYS_MAX, 1), "first {first}");
        assert_eq!(r.get(5000), None, "the completed one is left out");
        for k in 0..STORE_KEYS_MAX {
            assert_eq!(r.get(k), Some(k.to_le_bytes().to_vec()));
        }
    }
}

#[test]
fn a_cut_header_never_counts_once_the_next_session_has_written() {
    for now in [0xFFu8, 0x5A] {
        let mut r = fresh(66, 256);
        assert_eq!(r.put_now(1, b"old"), STORE_OK);
        assert_eq!(r.put_now(1, b"torn"), STORE_OK);
        let c = newest_commit_word_of(r.sim.region(), 1, VALUE_RECORD);
        let head = c - 4 - 16;
        for at in [&mut r.sim.cells.mem, &mut r.sim.cells.meant] {
            at[c..c + 4].fill(0xFF);
        }
        r.sim.set_faint(head, 16, now);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"old"[..]));
        unsafe { store_quiet(&mut *r.st, 1) };
        let mut copying = false;
        while !r.sim.dead {
            assert!(r.poll(), "recovery wrote no record");
            copying |= r.st.w.phase == W_COMMIT;
            if copying && r.st.w.phase == 0 && r.sim.cut_at.is_none() {
                r.sim.cut_at = Some(r.sim.ops);
            }
        }
        r.sim.dead = false;
        r.sim.cut_at = None;
        r.sim.faint_reads(0x00);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(
            r.get(1).as_deref(),
            Some(&b"old"[..]),
            "{now:#x}: the cut write came back"
        );
        r.maintain();
        assert_eq!(r.put_now(2, b"after"), STORE_OK);
        r.sim.faint_reads(0x00);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"old"[..]));
        assert_eq!(r.stats().corrupt, 0);
        r.assert_clean();
    }
}

#[test]
fn a_sector_read_as_killed_is_erased_before_anything_relies_on_it() {
    let mut r = fresh(67, 256);
    assert_eq!(r.put_now(7, b"value"), STORE_OK);
    let old = sectors_holding(r.sim.region(), 7, 1)[0];
    r.reset();
    r.mount();
    r.maintain();
    assert_eq!(r.delete_now(7), STORE_OK);
    assert!(!sectors_holding(r.sim.region(), 7, STORE_TYPE_TOMB as u8).contains(&old));
    for b in &mut r.sim.cells.meant[old * SECTOR..old * SECTOR + 8] {
        *b = 0;
    }
    r.sim.set_faint(old * SECTOR, 8, 0x00);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(7), None);
    assert_eq!(r.collect(), STORE_BUSY, "no collection before recovery");
    assert_eq!(r.put(30, b"x"), STORE_BUSY, "no write before recovery");
    r.maintain();
    let h = old * SECTOR;
    assert_eq!(r.sim.cells.faint[h..h + 8], [0; 8], "erased");
    assert_eq!(
        &r.sim.cells.mem[h..h + 8],
        b"SIDOUSTR",
        "a fresh erase header"
    );
    for _ in 0..12 {
        if sectors_holding(r.sim.region(), 7, STORE_TYPE_TOMB as u8).is_empty() {
            break;
        }
        assert_eq!(r.put_now(30, b"a head with room"), STORE_OK);
        while r.collect() == STORE_OK {
            r.settle();
            r.maintain();
        }
        r.reset();
        r.mount();
        r.maintain();
    }
    assert!(sectors_holding(r.sim.region(), 7, STORE_TYPE_TOMB as u8).is_empty());
    r.sim.faint_reads(0xFF);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(7), None, "the deleted value came back");
    r.assert_clean();
}

const W_COMMIT: u8 = 4;

fn copy_commit_of(r: &mut Rig, key: u32) -> usize {
    for _ in 0..100 {
        if r.collect() != STORE_OK {
            r.maintain();
            continue;
        }
        while r.poll() {
            let w = &r.st.w;
            if w.copy == 1 && w.key == key && w.phase == W_COMMIT {
                let at = usize::from(w.sec) * SECTOR
                    + usize::from(w.off)
                    + 16
                    + usize::from(w.len).div_ceil(4) * 4;
                while r.st.w.phase == W_COMMIT {
                    r.poll();
                }
                return at;
            }
        }
    }
    panic!("no collection copied key {key}");
}

#[test]
fn a_copy_read_erased_then_a_delete_never_brings_the_value_back() {
    for later in [0x00u8, 0x5A] {
        let mut r = fresh(68, 256);
        assert_eq!(r.put_now(30, b"first"), STORE_OK);
        r.reset();
        r.mount();
        r.maintain();
        assert_eq!(r.put_now(7, b"value"), STORE_OK);
        r.reset();
        r.mount();
        r.maintain();
        assert_eq!(r.put_now(40, b"newer"), STORE_OK);
        r.reset();
        r.mount();
        r.maintain();
        for k in 50..53 {
            assert_eq!(r.put_now(k, &[k as u8; 1100]), STORE_OK);
        }
        let c = copy_commit_of(&mut r, 7);
        r.sim.set_faint(c, COMMIT_WORD_LEN, 0xFF);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(7).as_deref(), Some(&b"value"[..]));
        assert_eq!(r.delete_now(7), STORE_OK);
        let tomb = |r: &Rig| !sectors_holding(r.sim.region(), 7, STORE_TYPE_TOMB as u8).is_empty();
        for _ in 0..12 {
            if !tomb(&r) {
                break;
            }
            r.reset();
            r.mount();
            r.maintain();
            assert_eq!(r.put_now(42, b"x"), STORE_OK);
            while tomb(&r) && r.collect() == STORE_OK {
                r.settle();
                r.maintain();
            }
        }
        assert!(!tomb(&r), "the tombstone was never dropped");
        assert_eq!(r.get(7), None);
        r.sim.faint_reads(later);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(7), None, "{later:#x}: the deleted value came back");
        assert_eq!(r.get(50), Some(vec![50; 1100]));
        r.assert_clean();
    }
}

#[test]
fn a_write_between_an_erase_and_its_header_lands_safely() {
    let mut r = fresh(69, 256);
    assert_eq!(r.put_now(7, b"value"), STORE_OK);
    r.reset();
    r.mount();
    r.maintain();
    assert_eq!(r.put_now(8, b"other"), STORE_OK);
    assert_eq!(r.collect(), STORE_OK);
    r.settle();
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    let erases = r.sim.erases;
    unsafe { store_quiet(&mut *r.st, 1) };
    while r.sim.erases == erases {
        assert!(r.poll(), "no erase came");
    }
    unsafe { store_quiet(&mut *r.st, 0) };
    r.sim.later = 100;
    let mut rc = STORE_BUSY;
    for _ in 0..1000 {
        rc = r.put(9, b"between");
        if rc != STORE_BUSY {
            break;
        }
        r.poll();
    }
    assert_eq!(rc, STORE_OK);
    r.sim.later = 0;
    r.settle();
    r.maintain();
    for i in 0..60u32 {
        assert_eq!(r.put_now(10 + i % 20, &[i as u8; 900]), STORE_OK);
    }
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    assert_eq!(r.get(7).as_deref(), Some(&b"value"[..]));
    assert_eq!(r.get(9).as_deref(), Some(&b"between"[..]));
    for i in 40..60u32 {
        assert_eq!(
            r.get(10 + i % 20),
            Some(vec![i as u8; 900]),
            "key {}",
            10 + i % 20
        );
    }
    assert_eq!(r.stats().corrupt, 0);
    r.assert_clean();
}

#[test]
fn api_calls_stay_within_a_polls_work() {
    let mut r = fresh(96, 256);
    let mut rng = Rng::new(96);
    for k in 0..STORE_KEYS_MAX {
        if k % 3 == 0 {
            r.reset();
            r.mount();
            r.maintain();
        }
        assert_eq!(
            r.put_now(k, &rng.bytes(200 + (k as usize * 37) % 1000)),
            STORE_OK
        );
    }
    let mut busy = 0;
    for i in 0..2000u32 {
        let k = (i * 7) % STORE_KEYS_MAX;
        let rc = r.put(k, &rng.bytes(VALUE_MAX));
        if rc == STORE_BUSY {
            busy += 1;
            let rc = r.file_begin("BIG", FILE_MAX);
            assert!(rc == STORE_BUSY || rc == STORE_FULL, "{rc}");
            if r.delete(k) == STORE_OK {
                r.settle();
            }
            assert!(r.erase_wanted());
            r.maintain();
            continue;
        }
        assert_eq!(rc, STORE_OK);
        r.settle();
    }
    assert!(busy > 10, "the pool ran short {busy} times");
    println!(
        "API calls: {} work at most; store_poll: {} (STORE_POLL_WORK {STORE_POLL_WORK})",
        r.api_work_max, r.work_max
    );
    r.assert_clean();
}

#[test]
fn an_unmounted_store_refuses_every_write_for_good() {
    let mut r = Rig::new(Sim::new(11), 256);
    assert_eq!(r.state(), STORE_STATE_UNMOUNTED);
    for _ in 0..3 {
        assert_eq!(r.put(1, b"value"), STORE_ERROR);
        assert_eq!(r.delete(1), STORE_ERROR);
        assert_eq!(r.file_begin("SONG", 100), STORE_ERROR);
        assert_eq!(r.file_delete("SONG"), STORE_ERROR);
        assert_eq!(r.file_rename("SONG", "OTHER"), STORE_ERROR);
        assert_eq!(r.collect(), STORE_ERROR);
        assert!(!r.poll(), "no work");
    }
    assert!(!r.busy());
    assert!(!r.erase_wanted());
    assert_eq!(r.get(1), None);
    assert!(r.file_open("SONG").is_none());
    assert!(r.dir().is_empty());
    assert_eq!(r.sim.ops, 0, "the part is never touched");
    assert_eq!(r.mount(), STORE_STATE_BLANK);
    assert_eq!(r.put_now(1, b"value"), STORE_OK);
    assert_eq!(r.get(1).as_deref(), Some(&b"value"[..]));
}

#[test]
fn a_sector_that_never_erases_is_retired_and_the_store_goes_on() {
    let mut sim = Sim::new(21);
    sim.worn = vec![0];
    let mut r = Rig::new(sim, 256);
    assert_eq!(r.mount(), STORE_STATE_BLANK);
    r.maintain();
    let s = r.stats();
    assert_eq!((s.pool, s.retired), (STORE_POOL_TARGET, 1));
    assert_eq!(s.errors, 2, "two failed erases, then no more");
    assert!(!r.erase_wanted(), "maintenance rests");
    assert_eq!(
        s.free,
        STORE_CAPACITY - STORE_FILE_SECTOR_DATA,
        "a sector's worth less"
    );
    let data = content(21, 30_000);
    for round in 0..6u32 {
        for k in 0..6u32 {
            assert_eq!(r.put_now(k, &fixture_value(k, round)), STORE_OK);
        }
        assert_eq!(r.file_now(["A", "B"][round as usize % 2], &data), STORE_OK);
        r.maintain();
        assert!(!r.erase_wanted());
    }
    assert_eq!(r.stats().retired, 1);
    r.reset();
    assert_eq!(r.mount(), STORE_STATE_SIDOU);
    r.maintain();
    assert!(!r.erase_wanted());
    for k in 0..6u32 {
        assert_eq!(r.get(k), Some(fixture_value(k, 5)));
    }
    assert_eq!(r.file("A").as_deref(), Some(&data[..]));
    assert_eq!(r.file("B").as_deref(), Some(&data[..]));
    assert_eq!(r.put_now(7, b"after"), STORE_OK);
    r.assert_clean();
}

#[test]
fn a_worn_head_after_a_cut_is_evacuated_retired_and_recovery_ends() {
    let (mut r, head) = (0..)
        .find_map(|n| {
            let mut r = fresh(22, 64);
            for k in 0..8u32 {
                assert_eq!(r.put_now(k, &fixture_value(k, 0)), STORE_OK);
            }
            let head = usize::from(r.st.head);
            r.sim.cut_at = Some(r.sim.ops + n);
            assert_eq!(r.put(8, &[0x5A; 300]), STORE_OK);
            r.settle();
            assert!(r.sim.dead, "cut {n}: the write ended first");
            r.sim.dead = false;
            r.sim.cut_at = None;
            r.reset();
            assert_eq!(r.mount(), STORE_STATE_SIDOU);
            (r.st.sstate[head] & 0x80 != 0).then_some((r, head))
        })
        .unwrap();
    r.sim.worn = vec![head];
    let cut = r.get(8);
    for boot in 0..3 {
        if boot > 0 {
            r.reset();
            assert_eq!(r.mount(), STORE_STATE_SIDOU);
        }
        assert_eq!(r.put(9, b"x"), STORE_BUSY, "boot {boot}: recovery first");
        r.maintain();
        let s = r.stats();
        assert_eq!(s.retired, 1, "boot {boot}");
        assert!(!r.erase_wanted(), "boot {boot}: maintenance rests");
        for k in 0..8u32 {
            assert_eq!(r.get(k), Some(fixture_value(k, 0)), "boot {boot}: key {k}");
        }
        assert_eq!(
            r.get(8),
            cut,
            "boot {boot}: the cut write as the first mount read it"
        );
        assert_eq!(r.put_now(9, b"after"), STORE_OK, "boot {boot}");
        assert_eq!(r.delete_now(9), STORE_OK, "boot {boot}");
        r.maintain();
        assert_eq!(r.get(9), None, "boot {boot}: the deletion holds");
    }
    r.assert_clean();
}

#[test]
fn power_loss_with_a_worn_sector_leaves_the_old_value_or_the_new() {
    let (mut tried, mut retired) = (0, 0);
    for (seed, window) in [(40u64, 16u32), (42, 64), (44, 256), (46, 4)] {
        let mut run = Run::with(seed, 10, true, window, seed % 4 == 2);
        run.rig.sim.suspend_percent = 0;
        let claimed = |r: &Rig| {
            let head = usize::from(r.st.head);
            (0..SECTORS)
                .filter(|&s| r.st.sstate[s] & 7 == 3 && s != head)
                .max_by_key(|&s| (r.st.sector[s].live > 0, s))
        };
        let mut steps = 0;
        while steps < 40 || claimed(&run.rig).is_none() {
            run.step();
            steps += 1;
            assert!(steps < 2_000, "seed {seed}: no claimed sector but the head");
        }
        let worn = claimed(&run.rig).unwrap();
        run.rig.sim.worn = vec![worn];
        let failed = run.rig.sim.failed;
        for _ in 0..200 {
            run.step();
            retired += u64::from(run.rig.stats().retired > 0);
        }
        tried += run.rig.sim.failed - failed;
        run.end();
    }
    println!("{tried} operations failed in worn sectors; retired after {retired} steps");
    assert!(
        tried > 0 && retired > 0,
        "the worn sectors were tried and retired"
    );
}
