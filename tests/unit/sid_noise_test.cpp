// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

TEST(SidNoise, ClocksOnEveryRisingEdgeOfBit19) {
    const uint32_t freq = 0xffff;
    const uint32_t cycles = 23;
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, SID_CTRL_NOISE);
    o.freq = freq;
    o.acc = 0xf00000; // the first step crosses the 24-bit wrap

    sid_osc reference = o;
    int clocks = 0;
    for (int step = 0; step < 200; ++step) {
        for (uint32_t i = 0; i < cycles; ++i) {
            uint32_t prev = reference.acc;
            reference.acc = (prev + freq) & 0xffffffu;
            if (!(prev & 0x80000u) && (reference.acc & 0x80000u)) {
                sid_lfsr_clock(&reference);
                ++clocks;
            }
        }
        sid_osc_advance(&o, cycles);
        ASSERT_EQ(o.acc, reference.acc) << "step " << step;
        ASSERT_EQ(o.lfsr, reference.lfsr) << "step " << step;
    }
    EXPECT_GT(clocks, 200);
}

TEST(SidNoise, TestBitClearsLfsrSoNoiseIsSilent) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, SID_CTRL_NOISE);
    o.freq = 0xffff;
    sid_osc_advance(&o, 23);
    sid_osc_write_control(&o, SID_CTRL_NOISE | SID_CTRL_TEST);
    sid_osc_advance(&o, 23);
    EXPECT_EQ(o.lfsr, 0u);
    EXPECT_EQ(sid_osc_output(&o, &o), 0u);
}

TEST(SidNoise, ReleasingTestBitReloadsLfsr) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, SID_CTRL_NOISE | SID_CTRL_TEST);
    sid_osc_write_control(&o, SID_CTRL_NOISE);
    EXPECT_EQ(o.lfsr, SID_LFSR_RESET);
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
