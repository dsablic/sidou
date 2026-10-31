// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::{Mutex, MutexGuard};

use sidou_pack::harness::{
    fs_classify, fs_counts, fs_driver_ok, fs_erase, fs_flash, fs_mark, fs_page_erase,
    fs_pattern_byte, fs_pattern_state, fs_prog, fs_reset, fs_suspend, FsCounters,
};

const FLASH: u32 = 1 << 20;
const SECTOR: u32 = 0x1000;
const PAGE: u32 = 0x100;
const STORE_LO: u32 = 0x97000;
const STORE_HI: u32 = 0xE0000;
const APP_HI: u32 = 0x93000;
const OTA: std::ops::Range<u32> = 0xE0000..0xE5000;
const SPIKE: std::ops::Range<u32> = 0xDE000..0xE0000;
const USE: u32 = 0xF00;
const FILL: u8 = 0x5A;

const BLANK: i32 = 0;
const OURS: i32 = 1;
const FOREIGN: i32 = 2;
const ABSENT: i32 = 0;
const INTACT: i32 = 1;
const DAMAGED: i32 = 2;

static HARNESS: Mutex<()> = Mutex::new(());

fn harness() -> MutexGuard<'static, ()> {
    let g = HARNESS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { fs_reset(FILL) };
    g
}

fn flash() -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(fs_flash(), FLASH as usize) }
}

fn counts() -> FsCounters {
    unsafe { fs_counts() }
}

fn inside(off: u32, n: u32) -> bool {
    off >= SPIKE.start && u64::from(off) + u64::from(n) <= u64::from(SPIKE.end)
}

fn untouched_outside() {
    let f = flash();
    let moved = (0..FLASH)
        .filter(|&a| !SPIKE.contains(&a))
        .find(|&a| f[a as usize] != FILL);
    assert_eq!(moved, None, "a byte outside the spike's sectors changed");
    let c = counts();
    if c.calls > 0 {
        assert!(
            c.lo >= SPIKE.start && c.hi <= SPIKE.end,
            "the driver reached [{:#x}, {:#x})",
            c.lo,
            c.hi
        );
    }
}

#[test]
fn the_bounds_are_the_stores_last_two_sectors_clear_of_everything_else() {
    let _h = harness();
    let c = counts();
    assert_eq!((c.spike_lo, c.spike_hi), (SPIKE.start, SPIKE.end));
    assert_eq!((c.pattern, c.measure), (0xDE000, 0xDF000));
    assert_eq!(
        (c.store_lo, c.store_hi, c.sector_use),
        (STORE_LO, STORE_HI, USE)
    );
    assert!(c.spike_lo >= c.store_lo && c.spike_hi <= c.store_hi);
    assert!(
        c.spike_lo >= APP_HI,
        "the flash head and the app area are below"
    );
    assert!(c.spike_hi <= OTA.start, "the update staging area is above");
}

#[test]
fn sector_erases_reach_the_driver_only_for_the_spikes_two_sectors() {
    let _h = harness();
    let mut accepted = Vec::new();
    let offs = (0..2 * FLASH)
        .step_by(PAGE as usize)
        .chain([u32::MAX - SECTOR + 1, !(SECTOR - 1)]);
    for off in offs {
        if unsafe { fs_erase(off) } == 0 {
            accepted.push(off);
        } else {
            assert_eq!(unsafe { fs_erase(off) }, -8);
        }
    }
    assert_eq!(accepted, vec![0xDE000, 0xDF000]);
    assert_eq!(counts().calls, 2);
    untouched_outside();
    assert!(flash()[SPIKE.start as usize..SPIKE.end as usize]
        .iter()
        .all(|&b| b == 0xFF));
}

#[test]
fn page_erases_reach_the_driver_only_for_whole_pages_inside() {
    let _h = harness();
    let mut accepted = 0;
    for off in (0..FLASH + SECTOR).step_by(16) {
        let ok = unsafe { fs_page_erase(off) } == 0;
        assert_eq!(
            ok,
            inside(off, PAGE) && off % PAGE == 0,
            "page erase at {off:#x}"
        );
        accepted += u32::from(ok);
    }
    assert_eq!(accepted, (SPIKE.end - SPIKE.start) / PAGE);
    assert_eq!(unsafe { fs_page_erase(!(PAGE - 1)) }, -8);
    untouched_outside();
}

#[test]
fn programs_reach_the_driver_only_inside_one_page_below_the_guard_band() {
    let _h = harness();
    let zeros = [0u8; 257];
    for off in (0..FLASH + SECTOR).step_by(8) {
        for n in [0u32, 1, 4, 16, 64, 255, 256, 257] {
            let ok = unsafe { fs_prog(off, zeros.as_ptr(), n) } == 0;
            let want = inside(off, n)
                && (1..=PAGE).contains(&n)
                && off % PAGE + n <= PAGE
                && off % SECTOR + n <= USE;
            assert_eq!(ok, want, "program of {n} at {off:#x}");
        }
    }
    for (off, n) in [
        (0xDE000, u32::MAX),
        (u32::MAX, 2),
        (u32::MAX - 15, 16),
        (0xDFFFF, 1),
    ] {
        assert_eq!(
            unsafe { fs_prog(off, zeros.as_ptr(), n) },
            -8,
            "{off:#x}+{n}"
        );
    }
    untouched_outside();
    let f = flash();
    for s in [0xDE000usize, 0xDF000] {
        assert!(f[s + USE as usize..s + SECTOR as usize]
            .iter()
            .all(|&b| b == FILL));
    }
}

#[test]
fn the_suspend_check_erases_only_a_spike_sector() {
    let _h = harness();
    let mut buf = [0u8; 256];
    for off in (0..FLASH).step_by(SECTOR as usize) {
        let ok = unsafe { fs_suspend(off, 0xDE100, buf.as_mut_ptr(), 256) } == 0;
        assert_eq!(ok, SPIKE.contains(&off), "suspended erase at {off:#x}");
    }
    assert_eq!(
        unsafe { fs_suspend(0xDF000, 0x4000, buf.as_mut_ptr(), 256) },
        -8
    );
    assert_eq!(
        unsafe { fs_suspend(0xDF000, 0xDE000, buf.as_mut_ptr(), 257) },
        -8
    );
    untouched_outside();
}

#[test]
fn the_ram_drivers_own_gate_is_the_same_bounds() {
    let _h = harness();
    for off in (0..FLASH + SECTOR).step_by(8) {
        let erase = unsafe { fs_driver_ok(off, SECTOR) } != 0;
        assert_eq!(
            erase,
            inside(off, SECTOR) && off % SECTOR == 0,
            "erase {off:#x}"
        );
        for n in [1u32, 16, 256] {
            let prog = unsafe { fs_driver_ok(off, n) } != 0;
            let want = inside(off, n) && off % PAGE + n <= PAGE && off % SECTOR + n <= USE;
            assert_eq!(prog, want, "program of {n} at {off:#x}");
        }
    }
    assert_eq!(unsafe { fs_driver_ok(0xE0000, SECTOR) }, 0, "the OTA area");
}

fn sector(f: impl Fn(usize) -> u8) -> Vec<u8> {
    (0..SECTOR as usize).map(f).collect()
}

fn mark() -> Vec<u8> {
    let m = unsafe { std::ffi::CStr::from_ptr(fs_mark()) }
        .to_bytes()
        .to_vec();
    assert_eq!(m.len(), 16);
    assert_ne!(&m[..8], b"SIDOUSTR", "not the store's sector magic");
    m
}

fn classify(s: &[u8]) -> i32 {
    unsafe { fs_classify(s.as_ptr()) }
}

fn pattern_state(s: &[u8]) -> i32 {
    unsafe { fs_pattern_state(s.as_ptr()) }
}

fn pattern() -> Vec<u8> {
    let m = mark();
    sector(|i| match i {
        _ if i < 16 => m[i],
        _ if i < USE as usize => unsafe { fs_pattern_byte(i as u32) },
        _ => 0xFF,
    })
}

#[test]
fn only_blank_sectors_and_the_spikes_own_are_taken() {
    let _h = harness();
    let m = mark();
    assert_eq!(classify(&sector(|_| 0xFF)), BLANK);
    let ours = sector(|i| if i < 16 { m[i] } else { 0xFF });
    assert_eq!(classify(&ours), OURS);
    assert_eq!(classify(&pattern()), OURS);
    let mut busy = ours.clone();
    busy[0x100..0x800].fill(0);
    assert_eq!(classify(&busy), OURS, "the spike's own, mid-run");
    for i in [0usize, 15, 16, 0xEFF, 0xF00, 0xF06, 0xFFF] {
        let mut s = sector(|_| 0xFF);
        s[i] = 0xFE;
        assert_eq!(
            classify(&s),
            FOREIGN,
            "a blank sector with byte {i:#x} programmed"
        );
    }
    let mut band = ours.clone();
    band[0xF06] = 0x41;
    assert_eq!(
        classify(&band),
        FOREIGN,
        "the spike's mark, the band programmed"
    );
    let mut store = sector(|_| 0xFF);
    store[..8].copy_from_slice(b"SIDOUSTR");
    assert_eq!(
        classify(&store),
        FOREIGN,
        "a store sector holds the user's data"
    );
    let mut off = ours.clone();
    off[7] ^= 1;
    assert_eq!(classify(&off), FOREIGN);
    assert_eq!(classify(&sector(|i| (i * 7) as u8)), FOREIGN);
}

#[test]
fn the_round_trip_pattern_is_absent_intact_or_damaged() {
    let _h = harness();
    assert_eq!(pattern_state(&sector(|_| 0xFF)), ABSENT);
    let p = pattern();
    assert_eq!(pattern_state(&p), INTACT);
    assert!(p[16..USE as usize].contains(&0), "zeros among the pattern");
    assert_ne!(p[0x100..0x200], p[0x200..0x300], "no two pages alike");
    for i in [0usize, 16, 0x800, 0xEFF, 0xF00] {
        let mut d = p.clone();
        d[i] ^= 0x10;
        assert_eq!(pattern_state(&d), DAMAGED, "byte {i:#x} changed");
    }
}
