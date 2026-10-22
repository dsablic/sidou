// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The firmware image's size (`sidou_app.bin`) against M2's flash schedule
//! (docs/plans/2026-10-15-m2-overview.md, "Budgets"). The app slot is fixed: the app area ends
//! at 0x93000 whatever the store's size (docs/hardware.md, "Flash map"), so the image's
//! ceilings are set here, plan by plan, and a raise reads as a change to the plan:
//!
//! | After | Ceiling (bytes) |
//! |---|---|
//! | M2a | 372,000 |
//! | M2b | 384,000 |
//! | M2c | 414,000 |
//! | M2d | 440,000 |
//! | M2e | 452,000 |
//! | M2f | 512,000 |
//!
//! M2 is closed (the end-of-M2 review is done) and M3 has not started, so `IMAGE_CEILING` is
//! M3's start target: 506,000 bytes, the image won back at M2's close
//! (docs/plans/2026-10-19-m3-sid-files.md, "Before M3"), which M3's own 28,000 then takes to
//! at most 534,000 and leaves at least 47,564 bytes of the slot for M4 and M5. M3's Task 2
//! adds `("M3", start + 28,000)` after the M2 schedule, `start` being the image measured
//! then (at most 506,000), and moves `IMAGE_CEILING` to it. The M2 schedule above is history
//! (the plans landed in parallel, so each ceiling was the latest stage merged, not the next
//! by sub-milestone order): it only rises, ends at or below 512,000 bytes and leaves at least
//! 69,564 bytes of the 581,564-byte slot for M3 (the 6502 core, PSID loading, the browser).

mod common;

use common::build_dir;

/// M2's schedule: the image's ceiling after each plan.
const SCHEDULE: [(&str, usize); 6] = [
    ("M2a", 372_000),
    ("M2b", 384_000),
    ("M2c", 414_000),
    ("M2d", 440_000),
    ("M2e", 452_000),
    ("M2f", 512_000),
];

/// What the image must hold now that M2 is closed and M3 not started: M3's start target.
const M3_START: usize = 506_000;
/// M3's own flash budget (its plan's ceiling), and what M4 and M5 keep of the slot after it.
const M3_BUDGET: usize = 28_000;
const M4_M5_RESERVE: usize = 47_564;
const PLAN: &str = "M3's start";
const IMAGE_CEILING: usize = M3_START;
// M3's start target is within M2's schedule, and M3's budget on top of it leaves M4 and M5
// their share of the slot.
const _: () = assert!(M3_START <= SCHEDULE[SCHEDULE.len() - 1].1);
const _: () = assert!(APP_SLOT - (M3_START + M3_BUDGET) >= M4_M5_RESERVE);

/// The app slot (app.ld's XIP region, sidou-pack's APP_SLOT).
const APP_SLOT: usize = 0x8DFBC;
/// What M2's last ceiling must leave of the slot for M3.
const M3_RESERVE: usize = 69_564;

#[test]
fn the_schedule_only_rises_and_leaves_m3_its_share() {
    for w in SCHEDULE.windows(2) {
        assert!(
            w[0].1 <= w[1].1,
            "the ceiling falls from {} to {}: {:?}",
            w[0].0,
            w[1].0,
            w
        );
    }
    let last = SCHEDULE[SCHEDULE.len() - 1].1;
    assert!(last <= 512_000, "M2's last ceiling is {last}");
    assert_eq!(APP_SLOT, 581_564);
    assert!(
        APP_SLOT - last >= M3_RESERVE,
        "{} bytes left for M3",
        APP_SLOT - last
    );
}

#[test]
fn the_image_fits_the_ceiling_of_m3s_start() {
    let Some(build) = build_dir() else {
        common::skip_message("the flash budget");
        return;
    };
    let image = build.join("platform/fm1/sidou_app.bin");
    let size = std::fs::metadata(&image)
        .unwrap_or_else(|e| panic!("{}: {e}; build the fm1 preset", image.display()))
        .len() as usize;
    println!(
        "image {size} bytes, {PLAN}'s ceiling {IMAGE_CEILING} ({} left), the slot {APP_SLOT}",
        IMAGE_CEILING as i64 - size as i64
    );
    assert!(
        size <= IMAGE_CEILING,
        "sidou_app.bin is {size} bytes, over {PLAN}'s ceiling of {IMAGE_CEILING}"
    );
    assert!(size <= APP_SLOT);
}
