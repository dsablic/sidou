// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::collections::BTreeMap;

use common::panel::{note_key, A4_KEY};
use common::{
    assert_within_budget, mount_dump, Firmware, Machine, CPU_HZ, HALF_FRAMES, POLL_INSTRUCTIONS,
    STORE_POLL_WORST,
};
use fm1_core::Bus;
use sidou_sys::*;

const OP_PUT: u32 = 1;
const OP_GET: u32 = 2;
const OP_FILE: u32 = 3;
const OP_COLLECT: u32 = 4;
const OP_SONG: u32 = 5;
const HOOK_KEY: u32 = 4;
const HOOK_LEN: u32 = 8;
const HOOK_RESULT: u32 = 12;
const HOOK_DATA: u32 = 16;
const HOOK_DATA_MAX: usize = 16;
const STAT_PROGS: u32 = 0;
const STAT_ERASES: u32 = 4;
const STAT_WINDOW_LATER: u32 = 8;
const STAT_HOLDS: u32 = 16;
const SERVICE_STATE_BYTES: u32 = 34;
const SERVICE_WANTED: usize = 28;
const SERVICE_QUIET: usize = 29;
const SERVICE_HOLDING: usize = 31;
const SERVICE_POLLED: usize = 32;
const JOB_ERASE: u8 = 1;
const JOB_COLLECT: u8 = 2;
const JOB_FORWARD: u8 = 3;
const COLLECT_SCAN: u8 = 0;
const COLLECT_KILL: u8 = 1;
const MOUNT_RECORDS: u8 = 4;
const A_MAJOR_CHORD: [u8; 3] = [A4_KEY, note_key(73), note_key(76)];
const CYCLES_PER_US: u64 = CPU_HZ / 1_000_000;
const PAST_LCD_WAKE_UP_MS: u64 = 400;
const OP_WITHIN_MS: u64 = 20_000;
const FILE_WITHIN_MS: u64 = 60_000;
const COMMIT_WITHIN_MS: u64 = 30_000;
const REST_WITHIN_MS: u64 = 30_000;
const REST_MS: u64 = 200;
const REST_STEP_MS: u64 = 10;
const OPS_WITHIN_MS: u64 = 60_000;
const OP_STEP_US: u64 = 10;
const SMALL_FILE: u32 = 20_000;
const SINGLE_WINDOW: u32 = 16;
const HELD_PROGRAMS: u32 = 0;
const ERASE_STEP_MS: u64 = 10;
const ERASE_ASK_AND_MARGIN_MS: u64 = 1_300;
const PEAK_SCAN_PER_SAMPLE: f64 = 2.0;
const PER_UNIT_MIN_WORK: u32 = 256;
const EVERY_OPERATION: u32 = 1;
const EVERY_THIRD: u32 = 3;
const EVERY_24TH: u32 = 24;
const MIDI_CLOCK: u8 = 0xF8;
const ACTIVE_SENSING: u8 = 0xFE;
const VOLUME_CC: [u8; 3] = [0xB0, 0x07, 100];
const HOLD_SILENCE_SAMPLES: usize = 2 * 2 * HALF_FRAMES;

fn store_fm1_define(name: &str) -> u64 {
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

struct Dev<'a> {
    fw: &'a Firmware,
    m: Machine,
    store: u32,
    hook: u32,
    counts: u32,
}

impl<'a> Dev<'a> {
    fn boot(fw: &'a Firmware, flash: Option<&[u8]>) -> Self {
        let mut m = Machine::boot_blank(&fw.image);
        if let Some(f) = flash {
            m.soc.flash.mem.copy_from_slice(f);
        }
        let mut d = Dev::over(fw, m);
        d.m.run_ms(PAST_LCD_WAKE_UP_MS);
        d
    }

    fn over(fw: &'a Firmware, m: Machine) -> Self {
        Dev {
            fw,
            m,
            store: fw.symbol("store_fm1_store"),
            hook: fw.symbol("store_fm1_hook"),
            counts: fw.symbol("store_fm1_counts"),
        }
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

    fn ops(&mut self) -> u32 {
        self.count(STAT_PROGS) + self.count(STAT_ERASES)
    }

    fn state(&mut self) -> u8 {
        self.st8(ilp32::store::state)
    }

    fn window(&mut self, bytes: u32) {
        let a = self.fw.symbol("store_fm1_window");
        self.m.soc.write32(a, bytes);
    }

    fn start(&mut self, op: u32, key: u32, data: &[u8], len: u32) {
        let h = self.hook;
        self.m.soc.write32(h + HOOK_KEY, key);
        self.m.soc.write32(h + HOOK_LEN, len);
        for (i, &b) in data.iter().enumerate() {
            self.m.soc.write8(h + HOOK_DATA + i as u32, b);
        }
        self.m.soc.write32(h, op);
    }

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
        self.finish(OP_WITHIN_MS)
    }

    fn put(&mut self, key: u32, data: &[u8]) -> i32 {
        let rc = self.op(OP_PUT, key, data);
        let end = self.m.ms() + COMMIT_WITHIN_MS;
        while self.writer_busy() {
            assert!(self.m.ms() < end, "the put never committed");
            self.m.run_ms(1);
        }
        rc
    }

    fn get(&mut self, key: u32) -> Option<Vec<u8>> {
        let n = self.op(OP_GET, key, &[]);
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

    fn writer_busy(&mut self) -> bool {
        self.st8(ilp32::store::w + ilp32::store_writer::phase) != 0
    }

    fn file_writer_busy(&mut self) -> bool {
        self.st8(ilp32::store::f + ilp32::store_file_writer::phase) != 0
    }

    fn settle(&mut self) {
        let end = self.m.ms() + REST_WITHIN_MS;
        let mut quiet_ms = 0;
        while quiet_ms < REST_MS {
            if self.m.ms() >= end {
                let fm = self.fw.symbol("store_fm1_state");
                let b: Vec<u8> = (0..SERVICE_STATE_BYTES)
                    .map(|i| self.m.soc.read8(fm + i))
                    .collect();
                panic!(
                    "the store never rested: job {} phase {} writer {} file {} ops {}; \
                     wanted {} quiet {} holding {} polled {}",
                    self.st8(ilp32::store::job),
                    self.st8(ilp32::store::job_phase),
                    self.st8(ilp32::store::w + ilp32::store_writer::phase),
                    self.st8(ilp32::store::f + ilp32::store_file_writer::phase),
                    self.ops(),
                    b[SERVICE_WANTED],
                    b[SERVICE_QUIET],
                    b[SERVICE_HOLDING],
                    b[SERVICE_POLLED]
                );
            }
            let ops = self.ops();
            self.m.run_ms(REST_STEP_MS);
            let idle =
                !self.writer_busy() && !self.file_writer_busy() && self.st8(ilp32::store::job) == 0;
            quiet_ms = if idle && self.ops() == ops {
                quiet_ms + REST_STEP_MS
            } else {
                0
            };
        }
    }

    fn flash(&self) -> Vec<u8> {
        self.m.soc.flash.mem.clone()
    }

    fn hold_chord(&mut self, down: bool) {
        for k in A_MAJOR_CHORD {
            self.m.key(k, down);
        }
    }

    fn run_to_ops(&mut self, ops: u32) {
        let end = self.m.ms() + OPS_WITHIN_MS;
        while self.ops() < ops {
            assert!(self.m.ms() < end, "never reached {ops} operations");
            self.m.run_cycles(OP_STEP_US * CYCLES_PER_US);
        }
        assert_eq!(self.ops(), ops, "two operations in one step");
    }
}

fn hook_file_pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len).map(|i| (i as u32 * 7 + seed) as u8).collect()
}

fn dumped_test_file(flash: &mut [u8]) -> Option<Vec<u8>> {
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
    let Some(fw) = common::firmware("store_persists") else {
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
        d.count(STAT_HOLDS) > 0,
        "held programs (no window fits the provisional timings)"
    );
    let flash = d.flash();
    let mut d2 = Dev::boot(&fw, Some(&flash));
    assert_eq!(d2.get(5).as_deref(), Some(&b"hello"[..]));
    assert_eq!(d2.get(6), None);
}

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
    let Some(fw) = common::firmware("store_cuts") else {
        return;
    };
    let (base, values) = worn_flash(&fw);
    let save = cut_everywhere(
        &fw,
        &base,
        EVERY_OPERATION,
        |d| d.start(OP_PUT, 1, b"the new value", 13),
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
    let mut with_file = Dev::boot(&fw, Some(&base));
    with_file.start(OP_FILE, 1, &[], SMALL_FILE);
    assert_eq!(with_file.finish(FILE_WITHIN_MS), STORE_OK);
    with_file.settle();
    let with_file = with_file.flash();
    let gc = cut_everywhere(
        &fw,
        &with_file,
        EVERY_THIRD,
        |d| d.start(OP_FILE, 2, &[], SMALL_FILE),
        |_, mut flash, k| {
            let f = dumped_test_file(&mut flash).expect("the old file or the new");
            assert!(
                f == hook_file_pattern(SMALL_FILE as usize, 1)
                    || f == hook_file_pattern(SMALL_FILE as usize, 2),
                "replacement cut {k}: a mix"
            );
        },
    );
    let file = cut_everywhere(
        &fw,
        &base,
        EVERY_24TH,
        |d| d.start(OP_FILE, 9, &[], STORE_FILE_MAX),
        |_, mut flash, k| {
            if let Some(f) = dumped_test_file(&mut flash) {
                assert_eq!(
                    f,
                    hook_file_pattern(STORE_FILE_MAX as usize, 9),
                    "file cut {k}: a mix"
                );
            }
        },
    );
    println!(
        "cuts checked: a save of {save} operations, a collection of {gc}, a 64 KiB file of {file}"
    );
}

#[test]
fn programs_in_windows_start_right_after_a_block() {
    let Some(fw) = common::firmware("store_windows") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.window(SINGLE_WINDOW);
    d.m.watch_calls(fw.symbol("fl_prog_ram"));
    d.m.snapshot_buffer(Some((fw.symbol("store_fm1_hook"), 0)));
    let blocks0 = d.m.alnk_blocks.len();
    let halves0 = d.m.soc.periph.audio_halves;
    let value: Vec<u8> = (0..SINGLE_WINDOW as u8).collect();
    assert_eq!(d.put(7, &value), STORE_OK);
    d.settle();
    let lead = store_fm1_define("STORE_WINDOW_LEAD_US") * CYCLES_PER_US;
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
        d.count(STAT_WINDOW_LATER) > 0,
        "some programs waited for a window"
    );
    assert_eq!(d.count(STAT_HOLDS), 0, "no hold for a program in a window");
    let blocks = (d.m.alnk_blocks.len() - blocks0) as i64;
    let halves = (d.m.soc.periph.audio_halves - halves0) as i64;
    assert!(
        (blocks - halves).abs() <= 1,
        "{blocks} blocks for {halves} halves played: one missed"
    );
    let worst = *d.m.watched_calls.iter().max().unwrap();
    let driver_us = store_fm1_define("STORE_DRIVER_US");
    println!(
        "{} programs; the driver {worst} instructions, {} us at 240 MHz (estimate {driver_us} us); \
         with the provisional 16-byte program time {} us",
        d.m.watched_calls.len(),
        worst / CYCLES_PER_US,
        worst / CYCLES_PER_US + store_fm1_define("STORE_PROG16_MAX_US")
    );
    assert_within_budget(
        "the flash driver's masked time (us)",
        worst / CYCLES_PER_US,
        driver_us,
    );
    let mut d2 = Dev::boot(&fw, Some(&d.flash()));
    assert_eq!(d2.get(7), Some(value));
}

fn queued_maintenance<'a>(fw: &'a Firmware, mut then: impl FnMut(&mut Dev)) -> Dev<'a> {
    let mut d = Dev::boot(fw, None);
    d.window(STORE_WINDOW_MAX);
    for seed in 1..=2 {
        d.start(OP_FILE, seed, &[], SMALL_FILE);
        assert_eq!(d.finish(FILE_WITHIN_MS), STORE_OK);
    }
    then(&mut d);
    d.window(HELD_PROGRAMS);
    d
}

fn erased_within(d: &mut Dev, ms: u64, mut each_step: impl FnMut(&mut Dev, u64)) -> Option<u64> {
    let erases = d.count(STAT_ERASES);
    let t0 = d.m.ms();
    for i in 0..ms / ERASE_STEP_MS {
        each_step(d, i);
        d.m.run_ms(ERASE_STEP_MS);
        if d.count(STAT_ERASES) != erases {
            return Some(d.m.ms() - t0);
        }
    }
    None
}

#[test]
fn no_erase_while_a_chord_sounds_ccs_arrive_or_a_song_plays_and_one_follows_once_quiet() {
    let Some(fw) = common::firmware("store_quiet") else {
        return;
    };
    let mut d = queued_maintenance(&fw, |d| d.hold_chord(true));
    let ops = d.ops();
    assert_eq!(
        erased_within(&mut d, 2_000, |_, _| {}),
        None,
        "an erase while a chord sounded"
    );
    assert_eq!(d.ops(), ops, "a held operation while a chord sounded");
    d.hold_chord(false);
    let audio0 = d.m.soc.periph.audio.len();
    let at = erased_within(&mut d, 5_000, |d, i| {
        d.m.midi(&[MIDI_CLOCK]);
        if i.is_multiple_of(2) {
            d.m.midi(&[MIDI_CLOCK]);
        }
        if i.is_multiple_of(30) {
            d.m.midi(&[ACTIVE_SENSING]);
        }
    })
    .expect("no erase once quiet");
    assert!(
        at >= store_fm1_define("STORE_QUIET_MS"),
        "an erase {at} ms after the keys let go"
    );
    d.m.run_ms(50);
    let played = &d.m.soc.periph.audio[audio0..];
    let mut run = 0usize;
    let longest = played.iter().fold(0usize, |best, &s| {
        run = if s == 0 { run + 1 } else { 0 };
        best.max(run)
    });
    assert!(
        longest >= HOLD_SILENCE_SAMPLES,
        "the hold's silence: {longest} samples"
    );
    let mut d = queued_maintenance(&fw, |d| d.m.midi(&VOLUME_CC));
    let cc = |d: &mut Dev, i: u64| {
        if i.is_multiple_of(10) {
            d.m.midi(&VOLUME_CC);
        }
    };
    assert_eq!(
        erased_within(&mut d, 2_000, cc),
        None,
        "an erase while CCs came in"
    );
    assert!(erased_within(&mut d, 5_000, |_, _| {}).is_some());
    let mut d = queued_maintenance(&fw, |d| {
        d.start(OP_SONG, 0, &[], 1);
        d.finish(1_000);
    });
    assert_eq!(
        erased_within(&mut d, 2_000, |_, _| {}),
        None,
        "an erase while a song played"
    );
    d.start(OP_SONG, 0, &[], 0);
    d.finish(1_000);
    assert!(erased_within(&mut d, 5_000, |_, _| {}).is_some());
}

fn block_mean(d: &mut Dev, ms: u64) -> f64 {
    let n0 = d.m.alnk_blocks.len();
    d.m.run_ms(ms);
    let b = &d.m.alnk_blocks[n0..];
    b.iter().sum::<u64>() as f64 / b.len() as f64
}

#[test]
fn the_peak_scan_costs_at_most_2_instructions_a_sample() {
    let Some(fw) = common::firmware("store_peak_scan") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.hold_chord(true);
    d.m.run_ms(300);
    let off = block_mean(&mut d, 600);
    d.window(STORE_WINDOW_MAX);
    for seed in 1..=2 {
        d.start(OP_FILE, seed, &[], SMALL_FILE);
        assert_eq!(d.finish(FILE_WITHIN_MS), STORE_OK);
    }
    d.m.run_ms(ERASE_ASK_AND_MARGIN_MS);
    let on = block_mean(&mut d, 600);
    let per_sample = (on - off) / HALF_FRAMES as f64;
    println!("audio interrupt: {off:.0} instructions a block, {on:.0} with the scan on ({per_sample:.2} a sample)");
    assert_within_budget("the peak scan a sample", per_sample, PEAK_SCAN_PER_SAMPLE);
    assert!(per_sample > 0.0, "the scan never ran");
}

#[test]
fn a_file_claims_before_it_writes_and_lands_while_a_chord_plays() {
    let Some(fw) = common::firmware("store_file") else {
        return;
    };
    let mut d = Dev::boot(&fw, None);
    d.window(STORE_WINDOW_MAX);
    d.hold_chord(true);
    d.m.run_ms(100);
    let (erases, holds) = (d.count(STAT_ERASES), d.count(STAT_HOLDS));
    d.start(OP_FILE, 4, &[], STORE_FILE_MAX);
    assert_eq!(d.finish(FILE_WITHIN_MS), STORE_OK);
    assert_eq!(
        d.count(STAT_ERASES),
        erases,
        "an erase while the file was written"
    );
    assert_eq!(
        d.count(STAT_HOLDS),
        holds,
        "a hold while the file was written"
    );
    d.hold_chord(false);
    assert_eq!(
        dumped_test_file(&mut d.flash()),
        Some(hook_file_pattern(STORE_FILE_MAX as usize, 4))
    );
}

fn poll_kind(b: &[u8]) -> &'static str {
    use ilp32::store as s;
    let w = b[s::w + ilp32::store_writer::phase];
    if b[s::state] == STORE_STATE_MOUNTING as u8 {
        return match (b[s::mount_sec], b[s::mount_phase]) {
            (sec, _) if u32::from(sec) == STORE_SECTORS => "a mount's finish",
            (_, MOUNT_RECORDS) => "a mount slice of records",
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
        (JOB_ERASE, _) => "an erase (its kill, erase, header)",
        (JOB_COLLECT, COLLECT_SCAN) => "a collection's scan (tombstones' walks, copies' starts)",
        (JOB_COLLECT, COLLECT_KILL) => "a collection's kill",
        (JOB_COLLECT, _) => "forget",
        (JOB_FORWARD, _) => "the newest record's copy forward",
        _ => "maintenance's plan (due)",
    }
}

#[test]
fn the_stores_share_of_a_pass_stays_within_store_poll_worst() {
    let Some(fw) = common::firmware("store_poll_work") else {
        return;
    };
    let (base, _) = worn_flash(&fw);
    let mut by_kind: BTreeMap<&str, (u64, u64, f64)> = BTreeMap::new();
    let mut service_max = 0;
    let mut pass_max = 0;
    for watching_polls in [true, false] {
        let mut m = Machine::boot(&fw.image);
        m.soc.flash.mem.copy_from_slice(&base);
        let st = fw.symbol("store_fm1_store");
        if watching_polls {
            m.watch_calls(fw.symbol("store_poll"));
            m.watch_bytes(st, ilp32::store::SIZE as u32);
        } else {
            m.watch_calls(fw.symbol("store_fm1_service"));
            m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        }
        m.run_ms(200);
        let mut d = Dev::over(&fw, m);
        d.window(STORE_WINDOW_MAX);
        for k in 0..6u32 {
            assert_eq!(d.put(k, &[k as u8; 16]), STORE_OK);
        }
        let _ = d.op(OP_COLLECT, 0, &[]);
        d.settle();
        d.start(OP_FILE, 1, &[], 9_000);
        assert_eq!(d.finish(30_000), STORE_OK);
        d.settle();
        let m = &d.m;
        if watching_polls {
            for ((&instr, entry), exit) in m
                .watched_calls
                .iter()
                .zip(&m.watched_entry_bytes)
                .zip(&m.watched_exit_bytes)
            {
                let work = u32::from_le_bytes(exit[ilp32::store::work..][..4].try_into().unwrap());
                let e = by_kind.entry(poll_kind(entry)).or_insert((0, 0, 0.0));
                e.0 = e.0.max(instr);
                e.1 = e.1.max(u64::from(work));
                if work >= PER_UNIT_MIN_WORK {
                    e.2 = e.2.max(instr as f64 / f64::from(work));
                }
                assert!(
                    instr <= STORE_POLL_WORST,
                    "a store_poll of {instr} ({})",
                    poll_kind(entry)
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
    assert_within_budget("store_fm1_service", service_max, STORE_POLL_WORST);
    assert!(service_max + pass_max <= POLL_INSTRUCTIONS);
}
