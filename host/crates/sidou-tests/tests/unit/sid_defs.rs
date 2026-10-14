// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::SID_CLOCK_PAL;

#[test]
fn pal_clock_is_the_c64_value() {
    assert_eq!(SID_CLOCK_PAL, 985248);
}
