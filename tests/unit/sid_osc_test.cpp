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
    sid_osc o = make_osc(SID_CTRL_SAW, 0x100000);
    sid_osc_advance(&o, 16);
    EXPECT_EQ(o.acc, 0u);
    EXPECT_EQ(o.msb_rose, 1);
    EXPECT_EQ(o.overshoot, 0x800000u); // crossed 0x800000 eight cycles ago
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
