// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The firmware's flash hooks for the update protocol (platform/fm1/ota_flash.h), built for
//! the host by harness/flash_hooks.c with stand-ins for the flash driver, the interrupt mask,
//! the audio and the LEDs: erases and writes only inside the staging area, never while the
//! audio runs, the driver always called with interrupts masked, reads in 256-byte windows.

use std::sync::{Mutex, MutexGuard};

use sidou_pack::harness::{fh_counts, fh_erase, fh_flash, fh_fread, fh_prog, fh_reset, FhCounters};

/// ota.c's staging area.
const AREA: u32 = 0xE0000;
const AREA_LEN: u32 = 0x5000;
/// What fh_reset fills the flash with.
const FILL: u8 = 0x5A;

static HARNESS: Mutex<()> = Mutex::new(());

/// The harness, reset, with the audio running or not; one test at a time (global state).
fn harness(audio: bool) -> MutexGuard<'static, ()> {
    let g = HARNESS.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { fh_reset(i32::from(audio)) };
    g
}

fn flash(range: std::ops::Range<u32>) -> Vec<u8> {
    let all = unsafe { std::slice::from_raw_parts(fh_flash(), 1 << 20) };
    all[range.start as usize..range.end as usize].to_vec()
}

fn counts() -> FhCounters {
    unsafe { fh_counts() }
}

fn erase(off: u32) -> i32 {
    unsafe { fh_erase(off) }
}

fn prog(off: u32, data: &[u8]) -> i32 {
    unsafe { fh_prog(off, data.as_ptr(), data.len() as u32) }
}

#[test]
fn the_area_is_ota_cs() {
    let _h = harness(false);
    assert_eq!((counts().area, counts().area_len), (AREA, AREA_LEN));
}

#[test]
fn erases_inside_the_area_with_interrupts_masked_and_leds_dark() {
    let _h = harness(false);
    for off in (AREA..AREA + AREA_LEN).step_by(0x1000) {
        assert_eq!(erase(off), 0, "erase at {off:#x}");
    }
    let c = counts();
    assert_eq!((c.erases, c.leds_dark, c.unmasked_driver_calls), (5, 5, 0));
    assert!(flash(AREA..AREA + AREA_LEN).iter().all(|&b| b == 0xFF));
    assert!(
        flash(AREA - 1..AREA).iter().all(|&b| b == FILL),
        "below the area"
    );
    assert!(
        flash(AREA + AREA_LEN..AREA + AREA_LEN + 1)
            .iter()
            .all(|&b| b == FILL),
        "above the area"
    );
}

#[test]
fn erases_outside_the_area_or_misaligned_are_refused() {
    let _h = harness(false);
    for off in [
        0,
        0x4000,
        AREA - 0x1000,
        AREA + AREA_LEN,
        0xFF000,
        AREA + 0x800,
        0xFFFF_F000,
    ] {
        assert_eq!(erase(off), -8, "erase at {off:#x}");
    }
    assert_eq!(counts().erases, 0);
    assert!(flash(0..1 << 20).iter().all(|&b| b == FILL));
}

#[test]
fn writes_only_inside_the_area() {
    let _h = harness(false);
    assert_eq!(erase(AREA), 0);
    assert_eq!(prog(AREA, &[1, 2, 3]), 0);
    assert_eq!(flash(AREA..AREA + 3), [1, 2, 3]);
    assert_eq!(prog(AREA + AREA_LEN - 4, &[0; 4]), 0, "the last bytes");
    for (off, n) in [
        (AREA - 1, 2usize),
        (AREA + AREA_LEN - 2, 4),
        (AREA + AREA_LEN, 1),
        (0x4000, 16),
        (u32::MAX - 1, 4), // wraps
    ] {
        assert_eq!(prog(off, &vec![0u8; n]), -8, "write of {n} at {off:#x}");
    }
    assert_eq!(counts().writes, 2);
}

#[test]
fn nothing_is_erased_or_written_while_the_audio_runs() {
    let _h = harness(true);
    assert_eq!(erase(AREA), -16);
    assert_eq!(prog(AREA, &[0]), -16);
    let c = counts();
    assert_eq!((c.refused, c.erases, c.writes, c.leds_dark), (2, 0, 0, 0));
    assert!(flash(AREA..AREA + AREA_LEN).iter().all(|&b| b == FILL));
    // Out-of-area requests are refused for that first, and not counted as audio refusals.
    assert_eq!(erase(0x4000), -8);
    assert_eq!(counts().refused, 2);
}

#[test]
fn reads_go_in_short_masked_windows() {
    let _h = harness(true); // reads are allowed while the audio runs
    let mut buf = vec![0u8; 1000];
    assert_eq!(unsafe { fh_fread(0x4000, buf.as_mut_ptr(), 1000) }, 0);
    assert!(buf.iter().all(|&b| b == FILL));
    let c = counts();
    assert_eq!(
        (c.read_windows, c.oversized_windows, c.unmasked_driver_calls),
        (4, 0, 0)
    );
}

/// harness/flash_hooks.c copies FL_IN from the HAL header: keep the two the same.
#[test]
fn the_harness_range_check_is_the_hals() {
    let norm = |s: &str| -> String {
        let a = s.find("#define FL_IN(").expect("FL_IN");
        let mut text = String::new();
        for l in s[a..].lines() {
            text.push_str(l.trim_end_matches('\\').trim());
            text.push(' ');
            if !l.trim_end().ends_with('\\') {
                break;
            }
        }
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../platform/fm1/hal/fm1_flash.h"
    );
    let hal = std::fs::read_to_string(root).unwrap();
    let harness = include_str!("../harness/flash_hooks.c");
    assert_eq!(norm(&hal), norm(harness));
}
