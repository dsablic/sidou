// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

namespace {

void setup(sid_osc* target, sid_osc* src) {
    sid_osc_reset(target);
    sid_osc_reset(src);
    sid_osc_write_control(target, SID_CTRL_SAW | SID_CTRL_SYNC);
    sid_osc_write_control(src, SID_CTRL_SAW);
    target->freq = 0x1000;
    target->acc = 0x123456;
    src->freq = 0x2000;
    src->acc = 0x7ff000;
}

} // namespace

TEST(SidSync, ResetsTargetWhenSourceMsbRisesOnTheLastCycle) {
    sid_osc target, src;
    setup(&target, &src);
    sid_osc_advance(&src, 1); // 0x7ff000 -> 0x801000: rose this cycle
    ASSERT_EQ(src.msb_rose, 1);
    sid_osc_sync(&target, &src);
    EXPECT_EQ(target.acc, 0u);
}

TEST(SidSync, TargetKeepsCyclesThatElapsedAfterTheRise) {
    sid_osc target, src;
    setup(&target, &src);
    sid_osc_advance(&src, 2); // rose one full cycle before the end of the step
    ASSERT_EQ(src.msb_rose, 1);
    sid_osc_sync(&target, &src);
    EXPECT_EQ(target.acc, 0x1000u);
}

TEST(SidSync, NoRiseNoReset) {
    sid_osc target, src;
    setup(&target, &src);
    src.acc = 0;
    sid_osc_advance(&src, 2);
    EXPECT_EQ(src.msb_rose, 0);
}
