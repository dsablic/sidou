// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
pub const PLAN: &str = "1.8";
pub const IMAGE_CEILING: usize = 579_516;

pub const SIZE_DEFERRED_TO: &str = "2.0";
const SIZE_DEFERRED_OPTION: &[u8] = b" SIDOU_CPU2=ON";

pub fn size_deferred(image: &[u8]) -> bool {
    image
        .windows(SIZE_DEFERRED_OPTION.len())
        .any(|w| w == SIZE_DEFERRED_OPTION)
}

pub fn assert_fits(what: &str, image: &[u8]) {
    let size = image.len();
    if size_deferred(image) {
        println!(
            "{what}: the second core's image (SIDOU_CPU2=ON), {size} bytes, {} over {PLAN}'s \
             ceiling {IMAGE_CEILING}: its size gate is {SIZE_DEFERRED_TO}'s work (the owner, \
             2026-11-15)",
            size as i64 - IMAGE_CEILING as i64
        );
        return;
    }
    assert!(
        size <= IMAGE_CEILING,
        "{what} is {size} bytes, over {PLAN}'s ceiling of {IMAGE_CEILING}"
    );
}
