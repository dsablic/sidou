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

/// A program cut short: some of the bits it would clear are cleared.
fn torn_prog(rng: &mut Rng, region: &mut [u8], off: usize, data: &[u8]) {
    for (i, &d) in data.iter().enumerate() {
        let clear = region[off + i] & !d;
        region[off + i] &= !(clear & rng.next() as u8);
    }
}

/// An erase cut short: all 0xFF (cut before its header), any mix of old bytes and 0xFF, or
/// erased up to some point.
fn torn_erase(rng: &mut Rng, region: &mut [u8], sec: usize) {
    let s = &mut region[sec * SECTOR..(sec + 1) * SECTOR];
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

/// A region as an operation, cut, would have left it, with the sectors left weak.
pub struct Fork {
    pub region: Vec<u8>,
    pub weak: [bool; SECTORS],
}

/// The simulated part.
pub struct Sim {
    pub mem: Vec<u8>,
    /// Sectors whose erase was cut: not to be used before an erase completes.
    pub weak: [bool; SECTORS],
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
    /// Keep the region as each operation, cut, would leave it.
    pub fork: bool,
    pub forks: Vec<Fork>,
}

impl Sim {
    pub fn new(seed: u64) -> Box<Self> {
        Box::new(Sim {
            mem: vec![0xFF; FLASH],
            weak: [false; SECTORS],
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
            forks: Vec::new(),
        })
    }

    pub fn with_region(seed: u64, region: &[u8], weak: [bool; SECTORS]) -> Box<Self> {
        let mut sim = Sim::new(seed);
        sim.mem[LO..HI].copy_from_slice(region);
        sim.weak = weak;
        sim
    }

    pub fn region(&self) -> &[u8] {
        &self.mem[LO..HI]
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
        let kill = rel == 0 && n == 8 && data.iter().all(|&b| b == 0);
        if !kill {
            if self.weak[sec] {
                return self.violation(format!(
                    "a program at {off:#x} in sector {sec}, cut mid-erase"
                ));
            }
            if self.mem[off..off + n].iter().any(|&b| b != 0xFF) {
                return self.violation(format!("a program over programmed bytes at {off:#x}+{n}"));
            }
        }
        let i = self.ops;
        self.ops += 1;
        self.progs += 1;
        self.kills += u64::from(kill);
        if self.fork {
            let mut region = self.mem[LO..HI].to_vec();
            torn_prog(&mut self.rng, &mut region, off - LO, &data);
            self.forks.push(Fork {
                region,
                weak: self.weak,
            });
        }
        if self.cut_at == Some(i) {
            torn_prog(&mut self.rng, &mut self.mem[LO..HI], off - LO, &data);
            self.dead = true;
            return STORE_ERROR;
        }
        for (b, d) in self.mem[off..off + n].iter_mut().zip(&data) {
            *b &= d;
        }
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
        if self.fork {
            let mut region = self.mem[LO..HI].to_vec();
            torn_erase(&mut self.rng, &mut region, sec);
            let mut weak = self.weak;
            weak[sec] = true;
            self.forks.push(Fork { region, weak });
        }
        if self.cut_at == Some(i) {
            torn_erase(&mut self.rng, &mut self.mem[LO..HI], sec);
            self.weak[sec] = true;
            self.dead = true;
            return STORE_ERROR;
        }
        self.mem[off..off + SECTOR].fill(0xFF);
        self.weak[sec] = false;
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
        assert_eq!(guard_band_written(self.sim.region()), None);
    }
}

/// The first store sector whose guard band holds a programmed byte.
pub fn guard_band_written(region: &[u8]) -> Option<usize> {
    (0..SECTORS).find(|&s| {
        region[s * SECTOR + USE..(s + 1) * SECTOR]
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
    // The store's RAM on the device: under 2 KB with Task 3's file handle to come.
    let size = ilp32::store::SIZE;
    println!("store: {size} bytes on the device");
    assert!(size <= 1_900, "the store struct is {size} bytes");
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
        let mut rng = Rng::new(seed ^ 0xC0FFEE);
        let window = [16u32, 64, 256][rng.below(3) as usize];
        let mut rig = fresh(seed, window);
        rig.sim.fork = forks;
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
        let forks = std::mem::take(&mut self.rig.sim.forks);
        for f in forks {
            self.forks_checked += 1;
            assert_eq!(guard_band_written(&f.region), None);
            let mut r = Rig::new(
                Sim::with_region(self.forks_checked, &f.region, f.weak),
                self.rig.window,
            );
            let state = r.mount();
            assert!(
                state == STORE_STATE_SIDOU || state == STORE_STATE_BLANK,
                "seed {}: a cut left the region {state}",
                self.seed
            );
            self.verify(&r, "a forked cut");
            let erased = f.weak.iter().zip(&self.rig.sim.weak).any(|(a, b)| a != b);
            if erased || self.forks_checked.is_multiple_of(self.deep) {
                // Carry on from there: a refill (a sector cut mid-erase is erased again
                // before any use) and a write.
                r.maintain();
                let k = self.keys;
                assert_eq!(r.put_now(k, b"after"), STORE_OK, "seed {}", self.seed);
                assert_eq!(r.get(k).as_deref(), Some(&b"after"[..]));
                self.verify(&r, "a forked cut, then a write");
                r.assert_clean();
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

fn power_loss(seed: u64, steps: u32, keys: u32, forks: bool) -> Run {
    let mut run = Run::new(seed, keys, forks);
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
    for seed in 0..3 {
        let run = power_loss(seed, 700, 10, true);
        forks += run.forks_checked;
        cuts += run.cuts;
        kills += run.rig.sim.kills;
        reclaims += run.rig.sim.reclaims;
    }
    println!(
        "{forks} forked cuts and {cuts} real cuts checked; {kills} kills, {reclaims} sectors \
         reclaimed"
    );
    assert!(forks > 3_000 && cuts > 10);
}

#[test]
fn power_loss_with_real_cuts_over_long_runs() {
    let mut cuts = 0;
    for seed in 100..110 {
        let run = power_loss(seed, 4_000, 14, false);
        cuts += run.cuts;
    }
    println!("{cuts} real cuts checked");
    assert!(cuts > 500);
}

/// The long version: thousands of seeds (minutes). `cargo test --release -p sidou-tests --test
/// app store::power_loss_long -- --ignored`.
#[test]
#[ignore]
fn power_loss_long() {
    let (mut forks, mut cuts) = (0, 0);
    for seed in 1_000..1_400 {
        let run = power_loss(seed, 1_500, 12, true);
        forks += run.forks_checked;
        cuts += run.cuts;
    }
    for seed in 10_000..13_000 {
        cuts += power_loss(seed, 3_000, 16, false).cuts;
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

const FIXTURE_KEYS: u32 = 16;
const FIXTURE_ROUNDS: u32 = 12;
/// Deleted in the fixture's last round.
const FIXTURE_DELETED: [u32; 3] = [3, 8, 13];

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/store-v1.bin")
}

/// A STORE_FORMAT 1 region: rounds of writes to 16 keys with collections between, three keys
/// deleted at the end, then a write cut halfway.
fn fixture_region() -> Vec<u8> {
    let mut r = fresh(13, 64);
    for round in 0..FIXTURE_ROUNDS {
        for k in 0..FIXTURE_KEYS {
            assert_eq!(r.put_now(k, &fixture_value(k, round)), STORE_OK);
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
    let s = r.stats();
    assert_eq!(s.corrupt, 0);
    assert_eq!(s.keys, FIXTURE_KEYS);
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
