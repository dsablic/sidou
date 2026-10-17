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
//! `IMAGE_CEILING` is the entry of the plan in progress. Each plan's last task moves it to its
//! own entry, or lowers it to the measured image plus a margin. The plans land in parallel,
//! so the stage in progress is the latest whose code is merged, not the next one by
//! sub-milestone order: M2a's store came after M2b's chip B, M2c's songs and M2d's song views had
//! merged. The schedule only rises and
//! ends at or below 512,000 bytes, which leaves at least 69,564 bytes of the 581,564-byte slot
//! for M3 (the 6502 core, PSID loading, the browser).

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

/// The plan in progress: its index in the schedule (the latest stage whose code is merged:
/// M2f's RED MOON RITE, with M2a's store, M2b's chip B, M2c's songs and M2d's song views in;
/// M2e's entry is passed over, its code not merged yet).
const IN_PROGRESS: usize = 5;
const PLAN: &str = SCHEDULE[IN_PROGRESS].0;
const IMAGE_CEILING: usize = SCHEDULE[IN_PROGRESS].1;

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
fn the_image_fits_the_ceiling_of_the_plan_in_progress() {
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
