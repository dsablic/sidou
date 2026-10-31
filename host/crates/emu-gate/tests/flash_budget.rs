// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;
mod flash_plan;

use common::build_dir;
use flash_plan::{IMAGE_CEILING, PLAN};

const APP_SLOT: usize = 0x8DFBC;
const SLOT_RESERVE: usize = 13_564;
const _: () = assert!(IMAGE_CEILING + SLOT_RESERVE <= APP_SLOT);

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
}
