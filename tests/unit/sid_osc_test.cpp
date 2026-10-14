// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_osc.h"
}

namespace {

sid_osc make_osc(uint8_t control, uint32_t freq) {
    sid_osc o;
    sid_osc_reset(&o);
    sid_osc_write_control(&o, control);
    o.freq = freq;
    return o;
}

} // namespace

TEST(SidOsc, AccumulatorAddsFrequencyEveryCycle) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 3);
    EXPECT_EQ(o.acc, 0x3000u);
}

TEST(SidOsc, AccumulatorWrapsAt24BitsAndReportsMsbRise) {
    const uint32_t freq = 0xffff;
    const uint32_t cycles = 23;
    sid_osc o = make_osc(SID_CTRL_SAW, freq);
    o.acc = 0xf00000; // the first step crosses the 24-bit wrap
    int wraps = 0;
    int rises = 0;
    for (int step = 0; step < 200; ++step) {
        uint32_t acc = o.acc;
        uint8_t rose = 0;
        uint32_t overshoot = 0;
        for (uint32_t i = 0; i < cycles; ++i) {
            uint32_t prev = acc;
            acc = (prev + freq) & 0xffffffu;
            overshoot += freq;
            if (!(prev & 0x800000u) && (acc & 0x800000u)) {
                rose = 1;
                overshoot = acc - 0x800000u;
            }
            if (acc < prev) {
                ++wraps;
            }
        }
        sid_osc_advance(&o, cycles);
        ASSERT_EQ(o.acc, acc) << "step " << step;
        ASSERT_EQ(o.msb_rose, rose) << "step " << step;
        if (rose) {
            ASSERT_EQ(o.overshoot, overshoot) << "step " << step;
            ++rises;
        }
    }
    EXPECT_GT(wraps, 0);
    EXPECT_GT(rises, 0);
}

TEST(SidOsc, SawIsTheTop12Bits) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 3);
    EXPECT_EQ(sid_osc_output(&o, &o), 3u);
}

TEST(SidOsc, TriangleFoldsOnTheMsb) {
    sid_osc o = make_osc(SID_CTRL_TRI, 0);
    o.acc = 0x400000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x800u);
    o.acc = 0x7fffff;
    EXPECT_EQ(sid_osc_output(&o, &o), 0xfffu);
    o.acc = 0xc00000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x7ffu);
}

TEST(SidOsc, RingModXorsTheSourceMsbIntoTheTriangle) {
    sid_osc o = make_osc(SID_CTRL_TRI | SID_CTRL_RING, 0);
    sid_osc src = make_osc(SID_CTRL_SAW, 0);
    o.acc = 0x400000;
    src.acc = 0x800000;
    EXPECT_EQ(sid_osc_output(&o, &src), 0x7ffu);
}

TEST(SidOsc, PulseComparesTop12BitsWithPulseWidth) {
    sid_osc o = make_osc(SID_CTRL_PULSE, 0);
    o.pw = 0x800;
    o.acc = 0x7ff000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x000u);
    o.acc = 0x800000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0xfffu);
}

TEST(SidOsc, NoWaveformSelectedOutputsZero) {
    sid_osc o = make_osc(0, 0x1000);
    sid_osc_advance(&o, 100);
    EXPECT_EQ(sid_osc_output(&o, &o), 0u);
}

TEST(SidOsc, TestBitResetsAndHoldsTheAccumulator) {
    sid_osc o = make_osc(SID_CTRL_SAW, 0x1000);
    sid_osc_advance(&o, 10);
    sid_osc_write_control(&o, SID_CTRL_SAW | SID_CTRL_TEST);
    sid_osc_advance(&o, 10);
    EXPECT_EQ(o.acc, 0u);
}

TEST(SidOsc, CombinedWaveformsAreAndedForNow) {
    sid_osc o = make_osc(SID_CTRL_SAW | SID_CTRL_PULSE, 0);
    o.pw = 0x100;
    o.acc = 0x5a5000;
    EXPECT_EQ(sid_osc_output(&o, &o), 0x5a5u & 0xfffu);
}
