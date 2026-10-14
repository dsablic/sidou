// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

TEST(SidNoise, ClocksOnEveryRisingEdgeOfBit19) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, SID_CTRL_NOISE);
    o.freq = 0x80000; // bit 19 toggles every cycle, so it rises every 2 cycles

    sid_osc reference = o;
    for (int i = 0; i < 10; ++i) {
        sid_lfsr_clock(&reference);
    }
    sid_osc_advance(&o, 20);
    EXPECT_EQ(o.lfsr, reference.lfsr);
}

TEST(SidNoise, FirstClockFromResetShiftsInZero) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_lfsr_clock(&o);
    EXPECT_EQ(o.lfsr, 0x7ffff0u);
}

TEST(SidNoise, OutputUsesReSidTapMapping) {
    EXPECT_EQ(sid_noise_output(SID_LFSR_RESET), 0xfe0u);
    EXPECT_EQ(sid_noise_output(0x400000u), 0x800u);
    EXPECT_EQ(sid_noise_output(0x000004u), 0x010u);
}

TEST(SidNoise, LfsrIsMaximalLength) {
    sid_osc o;
    sid_osc_reset(&o);
    for (uint32_t i = 0; i < 0x7fffffu; ++i) {
        sid_lfsr_clock(&o);
    }
    EXPECT_EQ(o.lfsr, SID_LFSR_RESET);
}
