// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_defs.h"
}

TEST(SidDefs, PalClockIsTheC64Value) {
    EXPECT_EQ(SID_CLOCK_PAL, 985248u);
}
