// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The store on the device (M2a Task 4, platform/fm1/store_fm1.c), end to end in fm1-emu:
//! values persist across a power cycle and a cut after any flash operation; programs go in
//! write windows right after an audio block, or held with the audio faded to silence; no erase
//! while the instrument plays; the peak scan costs nothing when off; a file claims its sectors
//! before it writes; the store's share of a main-loop pass stays within STORE_POLL_WORST.
//!
//! fm1-emu's flash is instantaneous (its operations take no time and never tear), so a cut is
//! the flash as it was after the k-th operation. The timings in store_fm1.h are provisional
//! (the P25Q80H datasheet's maxima) until M2a Task 3b's spike: by the decision rule no window
//! fits them, so programs are held operations and the tests that need windows set
//! `store_fm1_window` in RAM, as a spike's numbers would.
//!
//! The firmware's test hook (`store_fm1_hook`, store_fm1.h) does the store operations: the tests
//! write it in emulated RAM, as a debugger would; nothing on the device writes it.

mod common;

use std::collections::BTreeMap;

use common::{build_dir, mount_dump, Firmware, Machine, CPU_HZ, POLL_INSTRUCTIONS};
use fm1_core::Bus;
use sidou_sys::*;

/// The store's share of a main-loop pass on the device (M2a plan, budgets).
const STORE_POLL_WORST: u64 = 30_000;
/// The hook's operations (store_fm1.h).
const PUT: u32 = 1;
const GET: u32 = 2;
const FILE: u32 = 3;
const COLLECT: u32 = 4;
const SONG: u32 = 5;
/// store_fm1_test's layout (all 32-bit, then the data).
const HOOK_KEY: u32 = 4;
const HOOK_LEN: u32 = 8;
const HOOK_RESULT: u32 = 12;
const HOOK_DATA: u32 = 16;
const HOOK_DATA_MAX: usize = 16;
/// store_fm1_stats's layout.
const PROGS: u32 = 0;
const ERASES: u32 = 4;
const WINDOW_LATER: u32 = 8;
const HOLDS: u32 = 16;
/// A chord: A4, C#5, E5 (Felucca's key ids; app/panel.h).
const CHORD: [u8; 3] = [30, 34, 37];
const CYCLES_PER_US: u64 = CPU_HZ / 1_000_000;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

/// A #define of platform/fm1/store_fm1.h (a number, `u` suffix and all).
fn define(name: &str) -> u64 {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../platform/fm1/store_fm1.h");
    let text = std::fs::read_to_string(path).unwrap();
    let line = text
        .lines()
        .find(|l| l.starts_with(&format!("#define {name} ")))
        .unwrap_or_else(|| panic!("no {name} in store_fm1.h"));
    line.split_whitespace()
        .nth(2)
        .unwrap()
        .trim_end_matches('u')
        .parse()
        .unwrap()
}

/// The firmware's store as the tests reach it.
struct Dev<'a> {
    fw: &'a Firmware,
    m: Machine,
    store: u32,
    hook: u32,
    counts: u32,
}

impl<'a> Dev<'a> {
    /// Powered on with `flash` (blank if None), run until the main loop has run a while (the
    /// store's mount, recovery and pool fill ran in the LCD's wake-up wait).
    fn boot(fw: &'a Firmware, flash: Option<&[u8]>) -> Self {
        let mut m = Machine::boot_blank(&fw.image);
        if let Some(f) = flash {
            m.soc.flash.mem.copy_from_slice(f);
        }
        let mut d = Dev {
            fw,
            m,
            store: fw.symbol("store_fm1_store"),
            hook: fw.symbol("store_fm1_hook"),
            counts: fw.symbol("store_fm1_counts"),
        };
        // lcd_init's reset (150 ms) and wake-up wait (120 ms), then the main loop.
        d.m.run_ms(400);
        d
    }

    fn read32(&mut self, a: u32) -> u32 {
        self.m.soc.read32(a)
    }

    fn st8(&mut self, field: usize) -> u8 {
        self.m.soc.read8(self.store + field as u32)
    }

    fn count(&mut self, field: u32) -> u32 {
        self.read32(self.counts + field)
    }

    /// Flash operations so far.
    fn ops(&mut self) -> u32 {
        self.count(PROGS) + self.count(ERASES)
    }

    fn state(&mut self) -> u8 {
        self.st8(ilp32::store::state)
    }

    /// Sets the program window (store_fm1_window): bytes, 0 for held programs.
    fn window(&mut self, bytes: u32) {
        let a = self.fw.symbol("store_fm1_window");
        self.m.soc.write32(a, bytes);
    }

    /// Starts a hook operation (it runs once the store rests).
    fn start(&mut self, op: u32, key: u32, data: &[u8], len: u32) {
        let h = self.hook;
        self.m.soc.write32(h + HOOK_KEY, key);
        self.m.soc.write32(h + HOOK_LEN, len);
        for (i, &b) in data.iter().enumerate() {
            self.m.soc.write8(h + HOOK_DATA + i as u32, b);
        }
        self.m.soc.write32(h, op);
    }

    /// Runs until the hook's operation is done; its result.
    fn finish(&mut self, timeout_ms: u64) -> i32 {
        let end = self.m.ms() + timeout_ms;
        while self.read32(self.hook) != 0 {
            assert!(self.m.ms() < end, "the hook's operation never ran");
            self.m.run_ms(1);
        }
        self.read32(self.hook + HOOK_RESULT) as i32
    }

    fn op(&mut self, op: u32, key: u32, data: &[u8]) -> i32 {
        self.start(op, key, data, data.len() as u32);
        self.finish(20_000)
    }

    /// A put, and the run until it has committed: its data is the hook's until then (the
    /// store programs it from there, store.h).
    fn put(&mut self, key: u32, data: &[u8]) -> i32 {
        let rc = self.op(PUT, key, data);
        let end = self.m.ms() + 30_000;
        while self.st8(ilp32::store::w + ilp32::store_writer::phase) != 0 {
            assert!(self.m.ms() < end, "the put never committed");
            self.m.run_ms(1);
        }
        rc
    }

    fn get(&mut self, key: u32) -> Option<Vec<u8>> {
        let n = self.op(GET, key, &[]);
        if n == STORE_NOT_FOUND {
            return None;
        }
        assert!((0..=HOOK_DATA_MAX as i32).contains(&n), "get gave {n}");
        Some(
            (0..n as u32)
                .map(|i| self.m.soc.read8(self.hook + HOOK_DATA + i))
                .collect(),
        )
    }

    /// Runs until the store has rested 200 ms: no write, no job, no operation.
    fn settle(&mut self) {
        let end = self.m.ms() + 30_000;
        let mut still = 0;
        while still < 20 {
            if self.m.ms() >= end {
                let fm = self.fw.symbol("store_fm1_state");
                let b: Vec<u8> = (0..34).map(|i| self.m.soc.read8(fm + i)).collect();
                panic!(
                    "the store never rested: job {} phase {} writer {} file {} ops {}; \
                     wanted {} quiet {} holding {} polled {}",
                    self.st8(ilp32::store::job),
                    self.st8(ilp32::store::job_phase),
                    self.st8(ilp32::store::w + ilp32::store_writer::phase),
                    self.st8(ilp32::store::f + ilp32::store_file_writer::phase),
                    self.ops(),
                    b[28],
                    b[29],
                    b[31],
                    b[32]
                );
            }
            let ops = self.ops();
            self.m.run_ms(10);
            let idle = self.st8(ilp32::store::w + ilp32::store_writer::phase) == 0
                && self.st8(ilp32::store::f + ilp32::store_file_writer::phase) == 0
                && self.st8(ilp32::store::job) == 0;
            still = if idle && self.ops() == ops {
                still + 1
            } else {
                0
            };
        }
    }

    fn flash(&self) -> Vec<u8> {
        self.m.soc.flash.mem.clone()
    }

    fn chord(&mut self, down: bool) {
        for k in CHORD {
            self.m.key(k, down);
        }
    }

    /// Runs until the flash operations reach `ops` (10 us steps: they are milliseconds
    /// apart), never past.
    fn run_to_ops(&mut self, ops: u32) {
        let end = self.m.ms() + 60_000;
        while self.ops() < ops {
            assert!(self.m.ms() < end, "never reached {ops} operations");
            self.m.run_cycles(10 * CYCLES_PER_US);
        }
        assert_eq!(self.ops(), ops, "two operations in one step");
    }
}

/// The test file's bytes (store_fm1.c's hook: byte i is i * 7 + seed).
fn pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len).map(|i| (i as u32 * 7 + seed) as u8).collect()
}

/// The file "TEST" of a dump, read through the host store: None if it does not exist.
fn dump_file(flash: &mut [u8]) -> Option<Vec<u8>> {
    let st = mount_dump(flash);
    let mut f = store_file::default();
    if unsafe { store_file_open(&*st, c"TEST".as_ptr(), &mut f) } != STORE_OK {
        return None;
    }
    let mut buf = vec![0u8; f.length as usize];
    let n = unsafe { store_file_read(&*st, &f, 0, buf.as_mut_ptr().cast(), f.length) };
    assert_eq!(n as u32, f.length);
    Some(buf)
}

/// A flash with a few values written over three boots, so collections have sectors to gain.
fn worn_flash(fw: &Firmware) -> (Vec<u8>, BTreeMap<u32, Vec<u8>>) {
    let mut values = BTreeMap::new();
    let mut flash = None;
    for boot in 0..3u32 {
        let mut d = Dev::boot(fw, flash.as_deref());
        for k in 0..3u32 {
            let v: Vec<u8> = (0..12).map(|i| (i + 10 * k + 40 * boot) as u8).collect();
            assert_eq!(d.put(k, &v), STORE_OK);
            values.insert(k, v);
        }
        d.settle();
        flash = Some(d.flash());
    }
    (flash.unwrap(), values)
}

#[test]
fn a_value_persists_across_a_power_cycle() {
    let Some(fw) = fixture("store_persists") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    assert_eq!(
        d.state(),
        STORE_STATE_SIDOU as u8,
        "a blank region formats at boot"
    );
    assert_eq!(d.put(5, b"hello"), STORE_OK);
    d.settle();
    assert!(
        d.count(HOLDS) > 0,
        "held programs (no window fits the provisional timings)"
    );
    let flash = d.flash();
    let mut d2 = Dev::boot(&fw, Some(&flash));
    assert_eq!(d2.get(5).as_deref(), Some(&b"hello"[..]));
    assert_eq!(d2.get(6), None);
}

/// Cuts after every flash operation of `work` (started on a machine booted from `base`), each
/// checked by `check` on the flash as it was.
fn cut_everywhere(
    fw: &Firmware,
    base: &[u8],
    every: u32,
    work: impl Fn(&mut Dev),
    mut check: impl FnMut(&Firmware, Vec<u8>, u32),
) -> u32 {
    let mut d = Dev::boot(fw, Some(base));
    let start = d.ops();
    work(&mut d);
    d.settle();
    let total = d.ops() - start;
    assert!(total > 0);
    let mut k = 1;
    while k <= total {
        let mut d = Dev::boot(fw, Some(base));
        let start = d.ops();
        work(&mut d);
        d.run_to_ops(start + k);
        check(fw, d.flash(), k);
        k += if k + every > total && k != total {
            total - k
        } else {
            every
        };
    }
    total
}

#[test]
fn a_cut_after_any_operation_leaves_the_old_value_or_the_new() {
    let Some(fw) = fixture("store_cuts") else {
        return;
    };
    let (base, values) = worn_flash(&fw);
    // A settings save: every operation.
    let save = cut_everywhere(
        &fw,
        &base,
        1,
        |d| d.start(PUT, 1, b"the new value", 13),
        |fw, flash, k| {
            let mut d = Dev::boot(fw, Some(&flash));
            let got = d.get(1);
            assert!(
                got.as_deref() == Some(&b"the new value"[..]) || got == values.get(&1).cloned(),
                "cut after operation {k}: {got:?}"
            );
            for key in [0, 2] {
                assert_eq!(d.get(key), values.get(&key).cloned(), "cut {k}: key {key}");
            }
        },
    );
    // A file replaced, and the collection and erases its old sectors take after: every
    // third operation and the last.
    let mut with_file = Dev::boot(&fw, Some(&base));
    with_file.start(FILE, 1, &[], 20_000);
    assert_eq!(with_file.finish(60_000), STORE_OK);
    with_file.settle();
    let with_file = with_file.flash();
    let gc = cut_everywhere(
        &fw,
        &with_file,
        3,
        |d| d.start(FILE, 2, &[], 20_000),
        |_, mut flash, k| {
            let f = dump_file(&mut flash).expect("the old file or the new");
            assert!(
                f == pattern(20_000, 1) || f == pattern(20_000, 2),
                "replacement cut {k}: a mix"
            );
        },
    );
    // The largest file: every 24th operation and the last (the host store checks the dumps).
    let file = cut_everywhere(
        &fw,
        &base,
        24,
        |d| d.start(FILE, 9, &[], 65_536),
        |_, mut flash, k| {
            if let Some(f) = dump_file(&mut flash) {
                assert_eq!(f, pattern(65_536, 9), "file cut {k}: a mix");
            }
        },
    );
    println!(
        "cuts checked: a save of {save} operations, a collection of {gc}, a 64 KiB file of {file}"
    );
}

#[test]
fn programs_in_windows_start_right_after_a_block() {
    let Some(fw) = fixture("store_windows") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.window(16);
    d.m.watch_calls(fw.symbol("fl_prog_ram"));
    d.m.snapshot_buffer(Some((fw.symbol("store_fm1_hook"), 0)));
    let blocks0 = d.m.alnk_blocks.len();
    let halves0 = d.m.soc.periph.audio_halves;
    let value: Vec<u8> = (0..16u8).collect();
    assert_eq!(d.put(7, &value), STORE_OK);
    d.settle();
    let lead = define("STORE_WINDOW_LEAD_US") * CYCLES_PER_US;
    let exits: Vec<u64> = d.m.buffer_snapshots.iter().map(|s| s.exit).collect();
    assert!(!d.m.watched_starts.is_empty());
    for &s in &d.m.watched_starts {
        let last = exits
            .iter()
            .rev()
            .find(|&&e| e <= s)
            .copied()
            .expect("a block before");
        assert!(
            s - last <= lead,
            "a program {} us after a block's end",
            (s - last) / CYCLES_PER_US
        );
    }
    assert!(
        d.count(WINDOW_LATER) > 0,
        "some programs waited for a window"
    );
    assert_eq!(d.count(HOLDS), 0, "no hold for a program in a window");
    let blocks = (d.m.alnk_blocks.len() - blocks0) as i64;
    let halves = (d.m.soc.periph.audio_halves - halves0) as i64;
    assert!(
        (blocks - halves).abs() <= 1,
        "{blocks} blocks for {halves} halves played: one missed"
    );
    // The driver's instructions with interrupts masked (fm1-emu: one a cycle), against the
    // estimate the decision rule adds to the part's program time.
    let worst = *d.m.watched_calls.iter().max().unwrap();
    let driver_us = define("STORE_DRIVER_US");
    println!(
        "{} programs; the driver {worst} instructions, {} us at 240 MHz (estimate {driver_us} us); \
         with the provisional 16-byte program time {} us",
        d.m.watched_calls.len(),
        worst / CYCLES_PER_US,
        worst / CYCLES_PER_US + define("STORE_PROG16_MAX_US")
    );
    assert!(worst / CYCLES_PER_US <= driver_us);
    let mut d2 = Dev::boot(&fw, Some(&d.flash()));
    assert_eq!(d2.get(7), Some(value));
}

/// Maintenance with work: a file written, then replaced, leaves its old sectors to collect and
/// erase (the boot's own maintenance clears whatever a mount finds, so this is made in the
/// session). The files go in 256-byte windows; `then` runs as the second one commits, before
/// maintenance has had a pass, and programs are held operations again from there.
fn queued_maintenance<'a>(fw: &'a Firmware, mut then: impl FnMut(&mut Dev)) -> Dev<'a> {
    let mut d = Dev::boot(fw, None);
    d.window(256);
    for seed in 1..=2 {
        d.start(FILE, seed, &[], 20_000);
        assert_eq!(d.finish(60_000), STORE_OK);
    }
    then(&mut d);
    d.window(0);
    d
}

/// Runs `ms` while `each_10ms` runs every 10 ms; 1 if an erase happened.
fn erased_within(d: &mut Dev, ms: u64, mut each_10ms: impl FnMut(&mut Dev, u64)) -> Option<u64> {
    let erases = d.count(ERASES);
    let t0 = d.m.ms();
    for i in 0..ms / 10 {
        each_10ms(d, i);
        d.m.run_ms(10);
        if d.count(ERASES) != erases {
            return Some(d.m.ms() - t0);
        }
    }
    None
}

#[test]
fn no_erase_while_it_plays() {
    let Some(fw) = fixture("store_quiet") else {
        return;
    };
    // A held chord: nothing; released, the erase follows once the output and input are quiet.
    let mut d = queued_maintenance(&fw, |d| d.chord(true));
    let ops = d.ops();
    assert_eq!(
        erased_within(&mut d, 2_000, |_, _| {}),
        None,
        "an erase while a chord sounded"
    );
    assert_eq!(d.ops(), ops, "a held operation while a chord sounded");
    d.chord(false);
    let audio0 = d.m.soc.periph.audio.len();
    // A DAW's clock (F8 at 120 BPM: 48 a second) and active sensing do not prevent it.
    let at = erased_within(&mut d, 5_000, |d, i| {
        d.m.midi(&[0xF8]);
        if i.is_multiple_of(2) {
            d.m.midi(&[0xF8]);
        }
        if i.is_multiple_of(30) {
            d.m.midi(&[0xFE]);
        }
    })
    .expect("no erase once quiet");
    assert!(
        at >= define("STORE_QUIET_MS"),
        "an erase {at} ms after the keys let go"
    );
    // The hold: the output faded to silence around the operation (two halves, 256 frames;
    // they play after the operation, which runs as soon as both are written).
    d.m.run_ms(50);
    let played = &d.m.soc.periph.audio[audio0..];
    let mut run = 0usize;
    let longest = played.iter().fold(0usize, |best, &s| {
        run = if s == 0 { run + 1 } else { 0 };
        best.max(run)
    });
    assert!(
        longest >= 2 * 2 * 128,
        "the hold's silence: {longest} samples"
    );
    // A CC every 100 ms prevents it; after the last, it goes.
    let mut d = queued_maintenance(&fw, |d| d.m.midi(&[0xB0, 0x07, 100]));
    let cc = |d: &mut Dev, i: u64| {
        if i.is_multiple_of(10) {
            d.m.midi(&[0xB0, 0x07, 100]);
        }
    };
    assert_eq!(
        erased_within(&mut d, 2_000, cc),
        None,
        "an erase while CCs came in"
    );
    assert!(erased_within(&mut d, 5_000, |_, _| {}).is_some());
    // A song playing prevents it.
    let mut d = queued_maintenance(&fw, |d| {
        d.start(SONG, 0, &[], 1);
        d.finish(1_000);
    });
    assert_eq!(
        erased_within(&mut d, 2_000, |_, _| {}),
        None,
        "an erase while a song played"
    );
    d.start(SONG, 0, &[], 0);
    d.finish(1_000);
    assert!(erased_within(&mut d, 5_000, |_, _| {}).is_some());
}

/// Mean audio-interrupt instructions a block over `ms`.
fn block_mean(d: &mut Dev, ms: u64) -> f64 {
    let n0 = d.m.alnk_blocks.len();
    d.m.run_ms(ms);
    let b = &d.m.alnk_blocks[n0..];
    b.iter().sum::<u64>() as f64 / b.len() as f64
}

#[test]
fn the_peak_scan_costs_at_most_2_instructions_a_sample() {
    let Some(fw) = fixture("store_peak_scan") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.chord(true); // never quiet: no hold, the scan's cost alone changes
    d.m.run_ms(300);
    let off = block_mean(&mut d, 600);
    // A file replaced in windows while the chord plays: maintenance wants erases.
    d.window(256);
    for seed in 1..=2 {
        d.start(FILE, seed, &[], 20_000);
        assert_eq!(d.finish(60_000), STORE_OK);
    }
    // The next ask (store_erase_wanted) turns the scan on: asked once a second (WANT_MS), so
    // a wait of a full period and a little over, whatever phase the last write left it in.
    d.m.run_ms(1_300);
    let on = block_mean(&mut d, 600);
    let per_sample = (on - off) / 128.0;
    println!("audio interrupt: {off:.0} instructions a block, {on:.0} with the scan on ({per_sample:.2} a sample)");
    assert!(
        per_sample <= 2.0,
        "the peak scan costs {per_sample:.2} a sample"
    );
    assert!(per_sample > 0.0, "the scan never ran");
}

#[test]
fn a_file_claims_before_it_writes_and_lands_while_a_chord_plays() {
    let Some(fw) = fixture("store_file") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.window(256);
    d.chord(true);
    d.m.run_ms(100);
    let (erases, holds) = (d.count(ERASES), d.count(HOLDS));
    d.start(FILE, 4, &[], 65_536);
    assert_eq!(d.finish(60_000), STORE_OK);
    assert_eq!(
        d.count(ERASES),
        erases,
        "an erase while the file was written"
    );
    assert_eq!(d.count(HOLDS), holds, "a hold while the file was written");
    d.chord(false);
    assert_eq!(dump_file(&mut d.flash()), Some(pattern(65_536, 4)));
}

/// What a store_poll did, from the store as the call found it (store.c's enums).
fn kind(b: &[u8]) -> &'static str {
    use ilp32::store as s;
    let w = b[s::w + ilp32::store_writer::phase];
    if b[s::state] == STORE_STATE_MOUNTING as u8 {
        return match (b[s::mount_sec], b[s::mount_phase]) {
            (73, _) => "a mount's finish",
            (_, 4) => "a mount slice of records",
            _ => "a mount slice of headers and erased bytes",
        };
    }
    if w != 0 {
        return if b[s::w + ilp32::store_writer::copy] != 0 {
            "a copy"
        } else {
            "a record"
        };
    }
    if b[s::f + ilp32::store_file_writer::phase] != 0 {
        return "a file's chunk";
    }
    match (b[s::job], b[s::job_phase]) {
        (1, _) => "an erase (its kill, erase, header)",
        (2, 0) => "a collection's scan (tombstones' walks, copies' starts)",
        (2, 1) => "a collection's kill",
        (2, _) => "forget",
        (3, _) => "the newest record's copy forward",
        _ => "maintenance's plan (due)",
    }
}

#[test]
fn the_stores_share_of_a_pass_stays_within_store_poll_worst() {
    let Some(fw) = fixture("store_poll_work") else {
        return;
    };
    let (base, _) = worn_flash(&fw);
    let mut by_kind: BTreeMap<&str, (u64, u64, f64)> = BTreeMap::new();
    let mut service_max = 0;
    let mut pass_max = 0;
    for scenario in 0..2 {
        // Booted with the calls watched from power-on: the mount and the boot's maintenance.
        let mut m = Machine::boot(&fw.image);
        m.soc.flash.mem.copy_from_slice(&base);
        let st = fw.symbol("store_fm1_store");
        if scenario == 0 {
            m.watch_calls(fw.symbol("store_poll"));
            m.watch_bytes(st, ilp32::store::SIZE as u32);
        } else {
            m.watch_calls(fw.symbol("store_fm1_service"));
            m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        }
        m.run_ms(200);
        let mut d = Dev {
            fw: &fw,
            m,
            store: st,
            hook: fw.symbol("store_fm1_hook"),
            counts: fw.symbol("store_fm1_counts"),
        };
        // Saves, a collection and maintenance, a file; with windows (the busiest passes).
        d.window(256);
        for k in 0..6u32 {
            assert_eq!(d.put(k, &[k as u8; 16]), STORE_OK);
        }
        let _ = d.op(COLLECT, 0, &[]); // STORE_NOT_FOUND if the boot collected it all
        d.settle();
        d.start(FILE, 1, &[], 9_000);
        assert_eq!(d.finish(30_000), STORE_OK);
        d.settle();
        let m = &d.m;
        if scenario == 0 {
            for ((&instr, entry), exit) in m
                .watched_calls
                .iter()
                .zip(&m.watched_entry_bytes)
                .zip(&m.watched_exit_bytes)
            {
                let work = u32::from_le_bytes(exit[ilp32::store::work..][..4].try_into().unwrap());
                let e = by_kind.entry(kind(entry)).or_insert((0, 0, 0.0));
                e.0 = e.0.max(instr);
                e.1 = e.1.max(u64::from(work));
                if work >= 256 {
                    e.2 = e.2.max(instr as f64 / f64::from(work));
                }
                assert!(
                    instr <= STORE_POLL_WORST,
                    "a store_poll of {instr} ({})",
                    kind(entry)
                );
            }
        } else {
            service_max = *m.watched_calls.iter().max().unwrap();
            pass_max = *m.passes.iter().max().unwrap();
        }
    }
    let poll_work = u64::from(STORE_POLL_WORK);
    for (k, (instr, work, per)) in &by_kind {
        println!("{k}: {instr} instructions at most, {work} work at most, {per:.2} a unit");
        assert!(
            per * poll_work as f64 <= STORE_POLL_WORST as f64,
            "{k}: {per:.2} instructions a unit: raise its weight in store.c"
        );
    }
    println!(
        "store_fm1_service {service_max} at most (STORE_POLL_WORST {STORE_POLL_WORST}); the rest \
         of a pass {pass_max}; together {} of {POLL_INSTRUCTIONS}",
        service_max + pass_max
    );
    assert!(service_max <= STORE_POLL_WORST);
    assert!(service_max + pass_max <= POLL_INSTRUCTIONS);
}
