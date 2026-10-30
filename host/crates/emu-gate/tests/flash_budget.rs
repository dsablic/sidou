// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The firmware image's size (`sidou_app.bin`) against M2's flash schedule
//! (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2-overview.md, "Budgets"). The app slot is fixed: the app area ends
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
//! | M3 | 534,000 (M3_START + M3's 28,000; `IMAGE_CEILING` from M3's Task 9 to LAB) |
//! | 1.0 | 552,000 (M3's ceiling + 18,000: LAB's estimate and 1.0's small features; `IMAGE_CEILING` until 1.1) |
//! | 1.1 | 556,000 (1.0's ceiling + 4,000: 1.1's features after a win-back; `IMAGE_CEILING` since 1.1) |
//!
//! M3's start target was 506,000 bytes, the image won back at M2's close
//! (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md, "Before M3"), which M3's own 28,000 then takes to
//! at most 534,000 and leaves at least 47,564 bytes of the slot for M4 and M5. M3's stage is
//! `("M3", M3_START + 28,000)` after the M2 schedule: the plan writes it as `start + 28,000`
//! with `start` the image measured at M3's start (at most 506,000), but the image moves under
//! the other 0.4.0 branches and the space reclaim, so the stage's start is the documented
//! `M3_START`, a fixed number. Since M3's Task 9 (the image holds the 6502, the environment,
//! the voice shadow and the player) `IMAGE_CEILING` was M3's stage.
//! 1.0's stage follows M3's: first LAB's 541,000 (docs/plans/2026-10-25-lab-plan.md, Task 1's
//! decisions), raised by the user on 2026-10-28 to 552,000 (LAB's estimate plus 1.0's small
//! features; another agent wins flash back on main, and what it saves stays as room for 1.x).
//! The slot keeps 29,564 bytes (`X_RESERVE`), the room 1.x starts from; this replaces the
//! 40,000-byte M4/M5 floor. `IMAGE_CEILING` was this stage from LAB's Task 1b to 1.1.
//! 1.1's stage follows 1.0's (docs/plans/2026-10-29-1.1-design.md, "Flash"): the recommended scope
//! needs 7.3-11.2 KB against the 7,088 bytes under 1.0's ceiling, and the user decided on
//! 2026-10-29 to win 3-5 KB back first (the font to the glyphs 0..0xDC, song_ext.c, psid_file.c
//! and scene.c built for size: 544,912 down to 540,800 bytes) and set the ceiling 4,000 higher.
//! The slot keeps 25,564 bytes (`X_RESERVE`), the room 1.x starts from after 1.1.
//! The M2 schedule above is history
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

/// M3's start target: the image M3's budget counts from.
const M3_START: usize = 506_000;
/// M3's own flash budget (its plan's ceiling), and what M4 and M5 keep of the slot after it.
const M3_BUDGET: usize = 28_000;
const M4_M5_RESERVE: usize = 47_564;
const PLAN: &str = "1.1";
const IMAGE_CEILING: usize = V11_STAGE.1;
/// M3's stage of the schedule: the image's ceiling once M3 is in.
const M3_STAGE: (&str, usize) = ("M3", M3_START + M3_BUDGET);
/// 1.0's flash budget on top of M3's ceiling, and its stage: the image's ceiling once 1.0 is in
/// (LAB's own 7,000 raised to 18,000 by the user, 2026-10-28).
const V1_BUDGET: usize = 18_000;
const V1_STAGE: (&str, usize) = ("1.0", M3_STAGE.1 + V1_BUDGET);
/// 1.1's flash budget on top of 1.0's ceiling, and its stage: the image's ceiling once 1.1 is in
/// (the 3-5 KB win-back first, then the user's decision, 2026-10-29).
const V11_BUDGET: usize = 4_000;
const V11_STAGE: (&str, usize) = ("1.1", V1_STAGE.1 + V11_BUDGET);
/// What 1.0's ceiling left of the slot, before 1.1.
const V1_RESERVE: usize = 29_564;
/// What 1.1's ceiling leaves of the slot: the room 1.x starts from.
const X_RESERVE: usize = 25_564;
/// What M3's ceiling must leave for M4 and M5 (the plans' floor then, before 1.0's raise).
const M4_M5_FLOOR: usize = 40_000;
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
fn the_image_fits_the_ceiling_of_its_plan() {
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

#[test]
fn m3s_stage_follows_m2s_and_leaves_m4_and_m5_their_share() {
    let (name, ceiling) = M3_STAGE;
    let last = SCHEDULE[SCHEDULE.len() - 1];
    assert_eq!(name, "M3");
    assert!(
        ceiling > last.1,
        "M3's ceiling {ceiling} does not rise from {} ({})",
        last.0,
        last.1
    );
    assert_eq!(ceiling, 534_000);
    assert_eq!(ceiling - M3_START, M3_BUDGET);
    assert!(
        ceiling <= APP_SLOT && APP_SLOT - ceiling >= M4_M5_FLOOR,
        "{} bytes of the slot left after M3's ceiling",
        APP_SLOT.saturating_sub(ceiling)
    );
}

#[test]
fn v1s_stage_follows_m3s_and_leaves_1x_its_room() {
    let (name, ceiling) = V1_STAGE;
    assert_eq!(name, "1.0");
    assert!(
        ceiling > M3_STAGE.1,
        "1.0's ceiling {ceiling} does not rise from M3's ({})",
        M3_STAGE.1
    );
    assert_eq!(ceiling, 552_000);
    assert!(
        ceiling <= APP_SLOT && APP_SLOT - ceiling >= V1_RESERVE,
        "{} bytes of the slot left after 1.0's ceiling",
        APP_SLOT.saturating_sub(ceiling)
    );
    assert_eq!(APP_SLOT - ceiling, V1_RESERVE);
}

#[test]
fn v11s_stage_follows_v1s_and_leaves_1x_its_room() {
    let (name, ceiling) = V11_STAGE;
    assert_eq!(name, "1.1");
    assert!(
        ceiling > V1_STAGE.1,
        "1.1's ceiling {ceiling} does not rise from 1.0's ({})",
        V1_STAGE.1
    );
    assert_eq!(ceiling, 556_000);
    assert_eq!(ceiling - V1_STAGE.1, V11_BUDGET);
    assert!(
        ceiling <= APP_SLOT && APP_SLOT - ceiling >= X_RESERVE,
        "{} bytes of the slot left after 1.1's ceiling",
        APP_SLOT.saturating_sub(ceiling)
    );
    assert_eq!(APP_SLOT - ceiling, X_RESERVE);
    assert_eq!(IMAGE_CEILING, ceiling);
}
