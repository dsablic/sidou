// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The store's core (`app/store.c`) on a simulated NOR flash with the part's semantics: a
//! program only clears bits, within one 256-byte page; an erase sets a sector to 0xFF; reads
//! are plain. The simulator also answers a share of operations with STORE_LATER, cuts power
//! during any operation with a torn outcome, and flags what the store must never do: program
//! the guard band, program bytes not erased (the kill's zeros over a header's magic
//! excepted), program across a page, use a sector whose erase was cut before erasing it again,
//! program from a source in flash, or run more than one operation in a store_poll.
//! docs/reference/store-format.md is the format.

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

/// xorshift64*: deterministic per seed.
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

/// The part's cells: the bytes as they read now, the bits programs meant to clear since the
/// last erase, and the bits a cut left faint (neither programmed nor erased: each mount reads
/// them anew, 0 or 1 at random, until a program clears them for good or an erase sets them).
#[derive(Clone)]
pub struct Cells {
    pub mem: Vec<u8>,
    pub meant: Vec<u8>,
    pub faint: Vec<u8>,
    /// Sectors whose erase was cut: not to be used before an erase completes.
    pub weak: [bool; SECTORS],
}

impl Cells {
    fn blank() -> Self {
        Cells {
            mem: vec![0xFF; HI - LO],
            meant: vec![0xFF; HI - LO],
            faint: vec![0; HI - LO],
            weak: [false; SECTORS],
        }
    }

    /// A complete program of `data` at region offset `off`.
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
        self.weak[sec] = false;
    }

    /// A program cut short. Plain: some of the bits it would clear are cleared. Faint: each
    /// such bit is cleared, left faint, or left as it was.
    fn torn_prog(&mut self, rng: &mut Rng, off: usize, data: &[u8], faint: bool) {
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
                match rng.below(100) {
                    0..=39 => {
                        self.mem[at] &= !m;
                        self.faint[at] &= !m;
                    }
                    40..=74 => self.faint[at] |= m,
                    _ => {}
                }
            }
        }
        if faint {
            self.reread_range(rng, off, data.len());
        }
    }

    /// An erase cut short. Plain: all 0xFF (cut before its header), any mix of old bytes and
    /// 0xFF, or erased up to some point. Faint: per bit, either cut during the part's
    /// pre-program to 0 (bits cleared, faint or as they were) or during the erase proper,
    /// from all 0 (bits set, faint, or still 0).
    fn torn_erase(&mut self, rng: &mut Rng, sec: usize, faint: bool) {
        let r = sec * SECTOR..(sec + 1) * SECTOR;
        self.weak[sec] = true;
        self.meant[r.clone()].fill(0);
        if faint {
            let preprogram = rng.chance(50);
            for at in r.clone() {
                for bit in 0..8 {
                    let m = 1u8 << bit;
                    let roll = rng.below(100);
                    if preprogram {
                        if self.mem[at] & m == 0 && self.faint[at] & m == 0 {
                            continue;
                        }
                        match roll {
                            0..=39 => {
                                self.mem[at] &= !m;
                                self.faint[at] &= !m;
                            }
                            40..=74 => self.faint[at] |= m,
                            _ => {}
                        }
                    } else {
                        self.mem[at] &= !m;
                        self.faint[at] &= !m;
                        match roll {
                            0..=39 => self.mem[at] |= m,
                            40..=74 => self.faint[at] |= m,
                            _ => {}
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
                self.mem[at] = (self.mem[at] & !f) | (rng.next() as u8 & f);
            }
        }
    }

    /// What a mount reads: every faint bit anew.
    pub fn reread(&mut self, rng: &mut Rng) {
        self.reread_range(rng, 0, HI - LO);
    }
}

/// A flash operation, as the fork log keeps it (region offsets).
#[derive(Clone)]
pub enum Op {
    Prog(usize, Vec<u8>),
    Erase(usize),
}

/// Every region a cut during one of the logged operations would have left: from the cells
/// before the first, each operation torn in turn, then applied whole for the next.
pub struct Replay {
    cells: Cells,
    ops: std::vec::IntoIter<Op>,
    rng: Rng,
    faint: bool,
}

impl Iterator for Replay {
    /// The cells as the operation, cut, would have left them, and whether it was an erase.
    type Item = (Cells, bool);

    fn next(&mut self) -> Option<(Cells, bool)> {
        let op = self.ops.next()?;
        let mut fork = self.cells.clone();
        match &op {
            Op::Prog(off, data) => {
                fork.torn_prog(&mut self.rng, *off, data, self.faint);
                self.cells.prog(*off, data);
            }
            Op::Erase(sec) => {
                fork.torn_erase(&mut self.rng, *sec, self.faint);
                self.cells.erase(*sec);
            }
        }
        Some((fork, matches!(op, Op::Erase(_))))
    }
}

/// The simulated part.
pub struct Sim {
    /// The whole flash, as the store's map reads it; the region's cells are mirrored in `cells`.
    pub mem: Vec<u8>,
    pub cells: Cells,
    /// Programs and erases done (each a place a power cut can fall).
    pub ops: u64,
    pub progs: u64,
    pub erases: u64,
    /// Kills (a header's magic programmed to zero) and erases of sectors that held data.
    pub kills: u64,
    pub reclaims: u64,
    /// Cut the power during this operation.
    pub cut_at: Option<u64>,
    pub dead: bool,
    /// Percent of operations answered STORE_LATER.
    pub later: u64,
    pub rng: Rng,
    pub violations: Vec<String>,
    /// Operations in the current store_poll call.
    pub in_poll: u32,
    /// Log the operations, for the regions a cut during each would leave (`replay`).
    pub fork: bool,
    log: Option<(Cells, Vec<Op>)>,
    /// Cuts leave faint bits, and every mount reads them anew.
    pub faint: bool,
    /// Programs into each 256-byte page since its sector's last erase, and the most seen.
    pub page_progs: Vec<u16>,
    pub page_progs_max: u16,
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
            fork: false,
            log: None,
            faint: false,
            page_progs: vec![0; (HI - LO) / STORE_PAGE as usize],
            page_progs_max: 0,
        })
    }

    pub fn with_region(seed: u64, region: &[u8], weak: [bool; SECTORS]) -> Box<Self> {
        let mut sim = Sim::new(seed);
        sim.cells.mem.copy_from_slice(region);
        // What the region holds counts as programmed, as far as it reads programmed.
        sim.cells.meant.copy_from_slice(region);
        sim.cells.weak = weak;
        sim.sync();
        sim
    }

    pub fn with_cells(seed: u64, cells: Cells, faint: bool) -> Box<Self> {
        let mut sim = Sim::new(seed);
        sim.cells = cells;
        sim.faint = faint;
        sim.sync();
        sim
    }

    fn sync(&mut self) {
        self.mem[LO..HI].copy_from_slice(&self.cells.mem);
    }

    pub fn region(&self) -> &[u8] {
        &self.mem[LO..HI]
    }

    /// Makes the bits meant programmed in `n` region bytes from `off` faint, reading `reads`
    /// (masked to them) until the next change.
    pub fn set_faint(&mut self, off: usize, n: usize, reads: u8) {
        for at in off..off + n {
            let f = !self.cells.meant[at];
            self.cells.faint[at] = f;
            self.cells.mem[at] = (self.cells.mem[at] & !f) | (reads & f);
        }
        self.sync();
    }

    /// Every faint bit reads `reads` (masked to it).
    pub fn faint_reads(&mut self, reads: u8) {
        for at in 0..HI - LO {
            let f = self.cells.faint[at];
            self.cells.mem[at] = (self.cells.mem[at] & !f) | (reads & f);
        }
        self.sync();
    }

    /// The bytes as a mount reads them: faint bits anew.
    pub fn reread(&mut self) {
        if self.faint {
            self.cells.reread(&mut self.rng);
            self.sync();
        }
    }

    /// The logged operations since the last call, to replay as forks.
    pub fn replay(&mut self, seed: u64) -> Option<Replay> {
        let (cells, ops) = self.log.take()?;
        Some(Replay {
            cells,
            ops: ops.into_iter(),
            rng: Rng::new(seed),
            faint: self.faint,
        })
    }

    fn logged(&mut self, op: Op) {
        if self.fork {
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
        // The kill: zeros over a header's magic, in one program or in pieces.
        let kill = rel + n <= 8 && data.iter().all(|&b| b == 0);
        let r = off - LO;
        if !kill {
            if self.cells.weak[sec] {
                return self.violation(format!(
                    "a program at {off:#x} in sector {sec}, cut mid-erase"
                ));
            }
            // Over programmed bytes, only the bytes meant already (weak bits programmed again).
            let meant = &self.cells.meant[r..r + n];
            if meant.iter().zip(&data).any(|(&m, &d)| m != 0xFF && m != d) {
                return self.violation(format!("a program over programmed bytes at {off:#x}+{n}"));
            }
        }
        let i = self.ops;
        self.ops += 1;
        self.progs += 1;
        self.kills += u64::from(kill && rel == 0);
        let page = r / STORE_PAGE as usize;
        self.page_progs[page] += 1;
        self.page_progs_max = self.page_progs_max.max(self.page_progs[page]);
        self.logged(Op::Prog(r, data.clone()));
        if self.cut_at == Some(i) {
            self.cells.torn_prog(&mut self.rng, r, &data, self.faint);
            self.sync();
            self.dead = true;
            return STORE_ERROR;
        }
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
        if self.later > 0 && self.rng.chance(self.later) {
            return STORE_LATER;
        }
        let sec = (off - LO) / SECTOR;
        let i = self.ops;
        self.ops += 1;
        self.erases += 1;
        self.reclaims += u64::from(self.mem[off..off + SECTOR].iter().any(|&b| b != 0xFF));
        self.logged(Op::Erase(sec));
        let pages = SECTOR / STORE_PAGE as usize;
        self.page_progs[sec * pages..(sec + 1) * pages].fill(0);
        if self.cut_at == Some(i) {
            self.cells.torn_erase(&mut self.rng, sec, self.faint);
            self.sync();
            self.dead = true;
            return STORE_ERROR;
        }
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

/// A store on a simulated part.
pub struct Rig {
    pub sim: Box<Sim>,
    pub st: Box<store>,
    pub window: u32,
    /// The data of the last put: the store programs it from here until it commits.
    data: Vec<u8>,
}

impl Rig {
    pub fn new(sim: Box<Sim>, window: u32) -> Self {
        let mut r = Rig {
            sim,
            st: Box::default(),
            window,
            data: Vec::new(),
        };
        r.reset();
        r
    }

    /// A fresh store struct on the same part, as after a power cycle.
    pub fn reset(&mut self) {
        let flash = store_flash {
            map: Some(sim_map),
            prog: Some(sim_prog),
            erase: Some(sim_erase),
            ctx: (&mut *self.sim as *mut Sim).cast(),
        };
        *self.st = Default::default();
        unsafe { store_init(&mut *self.st, &flash, self.window) };
    }

    /// One store_poll: never more than one flash operation.
    pub fn poll(&mut self) -> bool {
        self.sim.in_poll = 0;
        let busy = unsafe { store_poll(&mut *self.st) } != 0;
        assert!(
            self.sim.in_poll <= 1,
            "{} flash operations in one store_poll",
            self.sim.in_poll
        );
        busy
    }

    pub fn mount(&mut self) -> store_state_id {
        self.sim.reread();
        unsafe { store_mount(&mut *self.st) };
        let mut polls = 0;
        while self.busy() {
            self.poll();
            polls += 1;
            assert!(polls < 100_000, "the mount never ends");
        }
        assert!(polls >= SECTORS, "the mount ran in {polls} polls");
        self.state()
    }

    pub fn state(&self) -> store_state_id {
        unsafe { store_state(&*self.st) }
    }

    pub fn busy(&self) -> bool {
        unsafe { store_busy(&*self.st) != 0 }
    }

    /// Polls until the store rests (or the power is cut).
    pub fn settle(&mut self) {
        let mut polls = 0u64;
        while self.poll() && !self.sim.dead {
            polls += 1;
            assert!(polls < 2_000_000, "store_poll never rests");
        }
    }

    /// Maintenance while quiet, until it has nothing left to do.
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
        self.data = data.to_vec();
        unsafe {
            store_put(
                &mut *self.st,
                key,
                typ,
                self.data.as_ptr().cast(),
                self.data.len() as u32,
            )
        }
    }

    pub fn delete(&mut self, key: u32) -> i32 {
        unsafe { store_delete(&mut *self.st, key) }
    }

    pub fn collect(&mut self) -> i32 {
        unsafe { store_collect(&mut *self.st) }
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

    pub fn erase_wanted(&self) -> bool {
        unsafe { store_erase_wanted(&*self.st) != 0 }
    }

    /// Writes `data` as `key` and polls until it commits; a STORE_BUSY first lets
    /// maintenance refill the pool once.
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
            guard_band_programmed(self.sim.region(), &self.sim.cells.weak),
            None
        );
    }
}

/// Files.
impl Rig {
    pub fn file_begin(&mut self, name: &str, len: usize) -> i32 {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { store_file_begin(&mut *self.st, c.as_ptr(), STORE_FILE_SONG as u8, len as u32) }
    }

    pub fn file_write(&mut self, data: &[u8]) -> usize {
        unsafe { store_file_write(&mut *self.st, data.as_ptr().cast(), data.len() as u32) as usize }
    }

    pub fn file_result(&self) -> i32 {
        unsafe { store_file_result(&*self.st) }
    }

    /// Offers `data` in pieces of up to `offer` bytes (random sizes if `rng`), polling between
    /// offers as the main loop would, then ends the file and polls until it is committed (or
    /// the power is cut). Returns the result. Checks the back-pressure as it goes.
    pub fn file_stream(&mut self, data: &[u8], offer: usize, mut rng: Option<&mut Rng>) -> i32 {
        let mut at = 0;
        let mut idle = 0;
        while at < data.len() {
            let want = match rng.as_deref_mut() {
                Some(r) => 1 + r.below(offer as u64) as usize,
                None => offer,
            }
            .min(data.len() - at);
            let room = STORE_WINDOW_MAX as usize - usize::from(self.st.f.fill);
            let took = self.file_write(&data[at..at + want]);
            assert_eq!(
                took,
                want.min(room),
                "store_file_write took {took} of {want} with {room} free"
            );
            at += took;
            idle = if took == 0 { idle + 1 } else { 0 };
            assert!(idle < 100, "the file stopped taking bytes");
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

    /// A whole file write; no maintenance between its begin and its end.
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

    /// The file's bytes, checked against its CRC.
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
        unsafe { store_file_delete(&mut *self.st, c.as_ptr()) }
    }

    pub fn file_rename(&mut self, from: &str, to: &str) -> i32 {
        let (a, b) = (
            std::ffi::CString::new(from).unwrap(),
            std::ffi::CString::new(to).unwrap(),
        );
        unsafe { store_file_rename(&mut *self.st, a.as_ptr(), b.as_ptr()) }
    }
}

/// The first store sector whose guard band holds a programmed byte.
pub fn guard_band_written(region: &[u8]) -> Option<usize> {
    guard_band_programmed(region, &[false; SECTORS])
}

/// The same, but for sectors whose erase was cut (`weak`): the part, not the store, left
/// whatever they hold, and the store erases them before any use.
pub fn guard_band_programmed(region: &[u8], weak: &[bool; SECTORS]) -> Option<usize> {
    (0..SECTORS).find(|&s| {
        !weak[s]
            && region[s * SECTOR + USE..(s + 1) * SECTOR]
                .iter()
                .any(|&b| b != 0xFF)
    })
}

/// A formatted, empty store with its pool full.
pub fn fresh(seed: u64, window: u32) -> Rig {
    let mut r = Rig::new(Sim::new(seed), window);
    assert_eq!(r.mount(), STORE_STATE_BLANK);
    r.maintain();
    r
}

fn crc32(data: &[u8]) -> u32 {
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

#[test]
fn the_constants_are_the_plans() {
    assert_eq!(STORE_RECORDS, 3808);
    assert_eq!(STORE_POOL_MIN, 20);
    assert_eq!(STORE_POOL_TARGET, 22);
    assert_eq!(STORE_FILE_SECTOR_DATA, 3744);
    assert_eq!(STORE_CAPACITY, 198_432);
    // The store's RAM on the device: at most 2,000 bytes (the plan's budget).
    let size = ilp32::store::SIZE;
    println!("store: {size} bytes on the device");
    assert!(size <= 2_000, "the store struct is {size} bytes");
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
    // A short buffer gets the start, and the whole length.
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
    // Every clean sector: the erase header, then 0xFF.
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
    // The claim: a sequence number, a CRC of bytes 0-23, the claim word.
    let seq = rd32(h, 20);
    assert_eq!(rd32(h, 24), crc32(&h[..24]));
    assert_eq!(rd32(h, 28), 0);
    // The record at 32: type, flags, len, key, seq, CRC; data padded to 4; commit word.
    let rec = &h[32..32 + 16 + 8 + 4];
    assert_eq!(&rec[..4], &[1, 0, 5, 0]);
    assert_eq!(rd32(rec, 4), 0x1234_5678);
    assert!(
        rd32(rec, 8) > seq,
        "the record's number follows its sector's claim"
    );
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
    // Cold data, written once, and hot keys rewritten all the time.
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
    // Collect until the tombstone's sector goes: it has nothing older to hide by then.
    // The other keys move on twice, out of the first sector.
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

/// Bytes another firmware might leave: Felucca's projects look nothing like a SIDOU header.
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
    // One foreign sector in an otherwise blank region is foreign too.
    let mut one = vec![0xFF; HI - LO];
    one[40 * SECTOR + 100] = 0x42;
    let mut r2 = Rig::new(Sim::with_region(10, &one, [false; SECTORS]), 256);
    assert_eq!(r2.mount(), STORE_STATE_FOREIGN);
    // The user's confirm: a format, then the store is SIDOU's.
    unsafe { store_format(&mut *r.st) };
    assert_eq!(r.state(), STORE_STATE_BLANK);
    r.maintain();
    assert_eq!(r.stats().pool, STORE_POOL_TARGET);
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

// ---- power loss ----

/// What a key may read after a cut: its committed value, or the interrupted write's.
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
    /// Check every fork with a mount; every `deep`th also refills and writes on it.
    forks: bool,
    deep: u64,
    forks_checked: u64,
    cuts: u64,
    full: u64,
    seed: u64,
}

impl Run {
    fn new(seed: u64, keys: u32, forks: bool) -> Self {
        Run::with(seed, keys, forks, [16, 64, 256][(seed % 3) as usize], false)
    }

    /// `window`: the most one program writes; `faint`: cuts leave faint bits, read anew at
    /// every mount.
    fn with(seed: u64, keys: u32, forks: bool, window: u32, faint: bool) -> Self {
        let mut rng = Rng::new(seed ^ 0xC0FFEE);
        let mut rig = fresh(seed, window);
        rig.sim.fork = forks;
        rig.sim.faint = faint;
        rig.sim.later = if rng.chance(30) { 25 } else { 0 };
        Run {
            rig,
            rng,
            model: BTreeMap::new(),
            keys,
            inflight: Inflight::Idle,
            forks,
            deep: 8,
            forks_checked: 0,
            cuts: 0,
            full: 0,
            seed,
        }
    }

    fn allowed(&self, key: u32) -> Vec<Option<Vec<u8>>> {
        let mut a = vec![self.model.get(&key).cloned()];
        match &self.inflight {
            Inflight::Put(k, v) if *k == key => a.push(Some(v.clone())),
            Inflight::Delete(k) if *k == key => a.push(None),
            _ => {}
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

    /// Mounts every region a cut during the last operations would have left.
    fn check_forks(&mut self) {
        let Some(replay) = self.rig.sim.replay(self.seed ^ self.forks_checked) else {
            return;
        };
        let faint = self.rig.sim.faint;
        for (cells, erased) in replay {
            self.forks_checked += 1;
            assert_eq!(guard_band_programmed(&cells.mem, &cells.weak), None);
            let mut r = Rig::new(
                Sim::with_cells(self.forks_checked, cells, faint),
                self.rig.window,
            );
            let state = r.mount();
            assert!(
                state == STORE_STATE_SIDOU || state == STORE_STATE_BLANK,
                "seed {}: a cut left the region {state}",
                self.seed
            );
            self.verify(&r, "a forked cut");
            if erased || self.forks_checked.is_multiple_of(self.deep) {
                // Carry on from there: a refill (a sector cut mid-erase is erased again
                // before any use) and a write.
                let first: Vec<Option<Vec<u8>>> = (0..self.keys).map(|k| r.get(k)).collect();
                r.maintain();
                let k = self.keys;
                assert_eq!(r.put_now(k, b"after"), STORE_OK, "seed {}", self.seed);
                assert_eq!(r.get(k).as_deref(), Some(&b"after"[..]));
                self.verify(&r, "a forked cut, then a write");
                r.assert_clean();
                // Mounted again (faint bits read anew): what the first mount read stays.
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

    /// After the power was cut: a fresh store on what the part holds.
    fn recover(&mut self) {
        self.cuts += 1;
        self.rig.sim.dead = false;
        self.rig.sim.cut_at = None;
        self.rig.reset();
        let state = self.rig.mount();
        assert_eq!(state, STORE_STATE_SIDOU, "seed {}", self.seed);
        self.verify(&self.rig, "a real cut");
        if let Inflight::Put(k, _) | Inflight::Delete(k) = self.inflight {
            match self.rig.get(k) {
                Some(v) => self.model.insert(k, v),
                None => self.model.remove(&k),
            };
        }
        self.inflight = Inflight::Idle;
    }

    /// Polls the write in flight until it commits or the power is cut.
    fn finish(&mut self) {
        self.rig.settle();
        if self.forks {
            self.check_forks();
        }
        if self.rig.sim.dead {
            self.recover();
            return;
        }
        match std::mem::replace(&mut self.inflight, Inflight::Idle) {
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
        // Busy: a refill, then again (once more if the power was cut during the refill).
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
        // A write never takes the pool under its minimum (a deletion in a full store, one
        // under). Garbage collection's copies may use its reserve below that, until the
        // victim's erase.
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
        self.rig.sim.fork = false;
        self.maintain();
        self.rig.reset();
        assert_eq!(self.rig.mount(), STORE_STATE_SIDOU);
        self.verify(&self.rig, "the end");
        self.rig.assert_clean();
    }
}

/// Windows the quick runs take in turn: the device's (16, 64, 256), the smallest, and sizes
/// that split the 32-byte header, the 16-byte record header and the 4-byte commit word unevenly.
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

/// The long version: thousands of seeds (minutes), every window size from 1 to 256, faint bits
/// for three seeds in four. `cargo test --release -p sidou-tests --test app
/// store::power_loss_long -- --ignored`.
#[test]
#[ignore]
fn power_loss_long() {
    let (mut forks, mut cuts) = (0, 0);
    for seed in 1_000..1_400u64 {
        let window = 1 + (seed % 256) as u32;
        let run = power_loss(seed, 1_500, 12, true, window, seed % 4 != 0);
        forks += run.forks_checked;
        cuts += run.cuts;
    }
    for seed in 10_000..13_000u64 {
        let window = 1 + (seed % 256) as u32;
        cuts += power_loss(seed, 3_000, 16, false, window, seed % 4 != 0).cuts;
    }
    println!("{forks} forked cuts and {cuts} real cuts checked");
}

/// The tombstone rule: the sector holding a deletion is collected before the sector holding
/// the value it deleted, and the deletion must not come undone, at any cut.
#[test]
fn a_deletion_survives_its_sector_being_collected_first() {
    let mut run = Run::new(12, 24, true);
    run.rig.sim.later = 0;
    // The value, among cold data that keeps its sector over half live.
    run.write(1, Some(vec![0xA1; 40]));
    for k in 10..13 {
        run.write(k, Some(vec![k as u8; 1100]));
    }
    // Into later sectors: the deletion among values about to be superseded.
    for _ in 0..3 {
        run.write(2, Some(vec![2; 1200]));
    }
    run.write(1, None);
    for _ in 0..12 {
        run.write(2, Some(vec![3; 1200]));
    }
    // Collect: the deletion's sector, nearly all garbage, goes first.
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
    // Then the old value's sector, once its cold data moved on.
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

/// The claimed sectors holding a valid-looking record of `key` and `typ`.
fn sectors_holding(region: &[u8], key: u32, typ: u8) -> Vec<usize> {
    let mut out = Vec::new();
    for s in 0..SECTORS {
        let h = &region[s * SECTOR..(s + 1) * SECTOR];
        if h[..8] != *b"SIDOUSTR" || rd32(h, 28) != 0 {
            continue;
        }
        let mut off = 32;
        while off + 20 <= USE && h[off] != 0xFF {
            let len = u16::from_le_bytes([h[off + 2], h[off + 3]]) as usize;
            if h[off] == typ && rd32(h, off + 4) == key {
                out.push(s);
            }
            off += 16 + len.div_ceil(4) * 4 + 4;
        }
    }
    out
}

// ---- format versions ----

/// The fixture's values: key k's final value.
fn fixture_value(k: u32, round: u32) -> Vec<u8> {
    let len = ((k * 37 + round * 11) % 120) as usize;
    (0..len)
        .map(|i| (i as u32 * 7 + k * 13 + round) as u8)
        .collect()
}

/// The fixture's files' bytes.
fn fixture_file(seed: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u32 * 31 + seed * 17 + (i as u32 >> 8)) as u8)
        .collect()
}

/// The files the fixture holds at the end: name, seed, length.
const FIXTURE_FILES: [(&str, u32, usize); 3] =
    [("NEW", 4, 400), ("SONG A", 1, 5000), ("SONG B", 3, 2600)];

const FIXTURE_KEYS: u32 = 16;
const FIXTURE_ROUNDS: u32 = 12;
/// Deleted in the fixture's last round.
const FIXTURE_DELETED: [u32; 3] = [3, 8, 13];

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/store-v1.bin")
}

/// A STORE_FORMAT 1 region: rounds of writes to 16 keys with collections between, files
/// written, replaced, deleted and renamed among them, three keys deleted at the end, then a
/// write cut halfway.
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

/// The fixture on disk: runs of bytes other than 0xFF, each `[u32 flash offset][u16 length]
/// [bytes]`, little-endian; everything else in the region is 0xFF.
fn encode_runs(region: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < region.len() {
        if region[i] == 0xFF {
            i += 1;
            continue;
        }
        // A run ends at 8 bytes of 0xFF in a row, or at 65,535 bytes.
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

fn decode_runs(file: &[u8]) -> Vec<u8> {
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

/// `tests/data/store-v1.bin` is a STORE_FORMAT 1 region, checked in: every later version
/// must mount it to the same values (design doc section 5's migration rule).
/// `SIDOU_STORE_FIXTURE_UPDATE=1` writes it, for format 1 only.
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
    // And it carries on: a refill and writes over it.
    r.maintain();
    assert_eq!(r.put_now(3, b"back"), STORE_OK);
    r.reset();
    r.mount();
    assert_eq!(r.get(3).as_deref(), Some(&b"back"[..]));
    r.assert_clean();
}

// ---- damage ----

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

/// A real region with random bytes changed, in its headers and records: the mount never
/// crashes or reads out of bounds (run under the sanitizers), every value it gives is one the
/// region held at some point, and the store still never programs what it must not.
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

// ---- files ----

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
    // Small values are not files.
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
    // The old file's sectors are garbage now: a refill collects them.
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
    // Aborted: nothing appears, unused sectors go back to the pool.
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
    // Ended short.
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
    // A head with room: the file's sectors leave the pool under its minimum, so small writes
    // that need a new head wait for the file and a refill.
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
    // Deleting makes room after the collections.
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

/// Files under power cuts: random file writes, replacements, deletions and renames among small
/// writes, collections and refills. After every cut every file reads its old bytes or its new
/// ones, whole (its CRC checked), and every small value its old or new value.
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
        rig.sim.fork = forks;
        rig.sim.faint = faint;
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
                FileOp::Rename(a, b) if a == name || b == name => true, // checked below
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
        let faint = self.rig.sim.faint;
        for (cells, _) in replay {
            self.forks_checked += 1;
            let mut r = Rig::new(
                Sim::with_cells(self.forks_checked, cells, faint),
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
                // Mounted again (faint bits read anew): what the first mount read stays.
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

    /// After the operation: the forks, then a real cut's recovery, or the model's update.
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

    /// The open file, given some of its bytes and some polls (its claims may be half done),
    /// then aborted or ended short: the old file stays, whole.
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
        self.rig.sim.fork = false;
        self.maintain();
        self.rig.reset();
        assert_eq!(self.rig.mount(), STORE_STATE_SIDOU);
        self.verify(&self.rig, "the end");
        self.rig.assert_clean();
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
        // Files of up to a few windows' worth at the smallest windows, or the run takes minutes.
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

/// The long version, with the power-loss runs above (minutes).
#[test]
#[ignore]
fn files_power_loss_long() {
    let (mut forks, mut cuts) = (0, 0);
    for seed in 1_000..1_060u64 {
        let window = WINDOWS[seed as usize % WINDOWS.len()];
        let max_len = (window as usize * 100).clamp(1_000, 20_000);
        let run = file_power_loss(seed, 200, true, max_len, window, seed % 4 != 0);
        forks += run.forks_checked;
        cuts += run.cuts;
    }
    for seed in 10_000..10_300u64 {
        let window = 1 + (seed % 256) as u32;
        cuts += file_power_loss(seed, 600, false, FILE_MAX, window, seed % 4 != 0).cuts;
    }
    println!("{forks} forked cuts and {cuts} real cuts checked");
}

// ---- regressions ----

/// An open file's fresh key is taken from its begin: a small write cannot take the key
/// index's last slot from under it (the file's commit would index a 49th key).
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

/// An abort (or a short end) while a file sector's claim is half programmed leaves that sector
/// dirty: it is erased before any use, never claimed again over the bits already cleared.
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
                    continue; // the claim's first pieces may sit in a page already done
                }
                if short {
                    assert_eq!(unsafe { store_file_end(&mut *r.st) }, STORE_INVALID);
                } else {
                    unsafe { store_file_abort(&mut *r.st) };
                }
                // Every sector is used again: files, then values.
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

/// A file whose sender went away does not hold the store: after STORE_FILE_IDLE_MS without a
/// byte it is aborted, and maintenance and other files go on.
#[test]
fn an_open_file_whose_sender_goes_quiet_times_out() {
    let mut r = fresh(41, 64);
    assert_eq!(r.put_now(1, b"x"), STORE_OK);
    assert_eq!(r.file_begin("SYSEX", 5000), STORE_OK);
    let data = content(41, 5000);
    assert_eq!(r.file_write(&data[..300]), 256);
    // The staging buffer full: the store is the slow one, the time does not count.
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
    // Ended, it commits however long the store takes.
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

/// Replacing a file is checked against the space it frees: three songs nearly fill the store
/// (three of 64 KiB never fit: 3 x 18 sectors is more than the 53 outside the pool's minimum),
/// and re-saving one, even at the largest size, succeeds.
#[test]
fn replacing_a_file_counts_the_space_the_old_one_frees() {
    let mut r = fresh(42, 256);
    let near = 17 * STORE_FILE_SECTOR_DATA as usize; // 63,648 bytes: 17 sectors
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

// ---- faint bits ----

/// The region offset of the commit word of `key`'s newest small record (by its sequence number).
fn commit_word_of(region: &[u8], key: u32) -> usize {
    let mut best = (0u32, 0usize);
    for s in sectors_holding(region, key, 1) {
        let h = &region[s * SECTOR..(s + 1) * SECTOR];
        let mut off = 32;
        while off + 20 <= USE && h[off] != 0xFF {
            let len = u16::from_le_bytes([h[off + 2], h[off + 3]]) as usize;
            let size = 16 + len.div_ceil(4) * 4 + 4;
            if h[off] == 1 && rd32(h, off + 4) == key && rd32(h, off + 8) >= best.0 {
                best = (rd32(h, off + 8), s * SECTOR + off + size - 4);
            }
            off += size;
        }
    }
    assert_ne!(best.1, 0, "no record of key {key}");
    best.1
}

/// A cut during a record's commit word can leave its bits faint: read as programmed at one
/// mount, erased or half programmed at the next. Nothing written after it may depend on it,
/// and once a mount has read it committed, it stays committed.
#[test]
fn a_faint_commit_word_loses_nothing_written_after_it() {
    for reads in [0xFFu8, 0x5A] {
        let mut r = fresh(60, 256);
        assert_eq!(r.put_now(1, b"old"), STORE_OK);
        assert_eq!(r.put_now(1, b"new"), STORE_OK);
        let c = commit_word_of(r.sim.region(), 1);
        r.sim.set_faint(c, 4, 0x00);
        r.reset();
        assert_eq!(r.mount(), STORE_STATE_SIDOU);
        assert_eq!(r.get(1).as_deref(), Some(&b"new"[..]));
        for k in 2..6 {
            assert_eq!(r.put_now(k, b"after"), STORE_OK);
        }
        // The next mount reads the faint bits otherwise.
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

/// A cut during an erase header's program can leave it faint: valid at one mount (the sector
/// joins the pool), not at the next. Before a sector takes records, its claim programs the
/// erase header again, so the records never go with it.
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

/// A cut during a claim can leave its claim word faint: the sector reads claimed and empty at
/// one mount, cut short at the next. Nothing is written into it before then.
#[test]
fn a_faint_claim_loses_nothing_written_after_it() {
    for reads in [0xFFu8, 0x3C] {
        let mut r = fresh(62, 256);
        assert_eq!(r.put_now(1, b"one"), STORE_OK);
        // A clean sector claimed with the newest number, its claim word faint.
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
