// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
pub const PLAN: &str = "1.8";
pub const IMAGE_CEILING: usize = 579_516;

pub fn assert_fits(what: &str, image: &[u8]) {
    let size = image.len();
    assert!(
        size <= IMAGE_CEILING,
        "{what} is {size} bytes, over {PLAN}'s ceiling of {IMAGE_CEILING}"
    );
}
