// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>

extern "C" {
#include "sid/sid_filter.h"
#include "sid/sid_math.h"
}

namespace {

constexpr float kFs = 44100.0f;

sid_svf linear_svf(float hz, float q) {
    sid_svf f;
    sid_svf_reset(&f, 0.0f);
    sid_svf_set(&f, sidou_tanf(SIDOU_PI * hz / kFs), 1.0f / q);
    return f;
}

} // namespace

TEST(SidMath, TanMatchesLibmOnTheCutoffRange) {
    for (float x = 0.0f; x < 1.35f; x += 0.01f) {
        EXPECT_NEAR(sidou_tanf(x), std::tan(x), 2e-5f * std::tan(x) + 1e-7f) << "x=" << x;
    }
}

TEST(SidFilter, LowPassPassesDcAndHighPassBlocksIt) {
    sid_svf f = linear_svf(1000.0f, 0.707f);
    for (int i = 0; i < 44100; ++i) {
        sid_svf_tick(&f, 0.5f);
    }
    EXPECT_NEAR(f.lp, 0.5f, 1e-4f);
    EXPECT_NEAR(f.hp, 0.0f, 1e-4f);
}

TEST(SidFilter, LowPassGainAtCutoffEqualsQ) {
    const float hz = 1000.0f, q = 0.707f;
    sid_svf f = linear_svf(hz, q);
    float peak = 0.0f;
    for (int i = 0; i < 44100; ++i) {
        float x = std::sin(2.0f * SIDOU_PI * hz * (float)i / kFs);
        sid_svf_tick(&f, x);
        if (i > 22050) {
            peak = std::fmax(peak, std::fabs(f.lp));
        }
    }
    EXPECT_NEAR(peak, q, 0.01f);
}

TEST(SidFilter, SoftClipBoundsTheBandPassState) {
    sid_svf f;
    sid_svf_reset(&f, 2.0f);
    sid_svf_set(&f, sidou_tanf(SIDOU_PI * 2000.0f / kFs), 1.0f / 3.0f);
    for (int i = 0; i < 4410; ++i) {
        sid_svf_tick(&f, (i / 20) % 2 ? 10.0f : -10.0f);
        ASSERT_LE(std::fabs(f.ic1), 1.0f / 2.0f + 1e-6f);
    }
}

TEST(SidCutoff, CurvesHitTheMeasuredPoints) {
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 0), 220.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 1023), 6000.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 1024), 4600.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_6581, 2047), 18000.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_8580, 2047), 12500.0f);
    EXPECT_FLOAT_EQ(sid_cutoff_hz(SID_MODEL_8580, 64), 400.0f);
}
