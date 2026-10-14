// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>

#include "render.h"
#include "residfp_engine.h"
#include "signal_metrics.h"

namespace {

/* M0 tolerances. Tighten them in M1; never loosen them to make a failure pass
 * without writing down why. */
/* Measured at M0: sidou 0.0014 cents from theory and from reSIDfp; reSIDfp
 * < 0.0001 cents from theory. */
constexpr double kMaxPitchCents = 0.1;
/* Coupled to kEnvelopeFloorDb: the 8580's worst window sits at the floor, where
 * the release-slope difference below has grown largest. Changing the floor
 * means re-checking this tolerance. */
constexpr double kMaxEnvelopeDb8580 = 0.75; /* measured at M0: 0.44 (in the release, at -20 dB) */
/* reSIDfp models the 6581's non-linear envelope DAC, which M0 does not; the
 * release tail drifts ~2 dB apart by -20 dB. M1 adds the DAC and tightens this. */
constexpr double kMaxEnvelopeDb6581 = 2.5; /* measured at M0: 2.04 */
/* M1 investigation item, envelope timing rather than the DAC: on the 8580 our
 * release falls ~5.5% steeper than reSIDfp's (55.9 vs 53.0 dB/s), 1.8 dB apart
 * by -30 dB; below kEnvelopeFloorDb, so this test does not see it yet. */
constexpr double kEnvelopeFloorDb = -20.0;

/* A4 from the register value 0x1d45, not 440 Hz. */
constexpr double kSawA4Hz = 0x1d45 * (double)SID_CLOCK_PAL / 16777216.0;

/* Envelope windows are 4 periods of kSawA4Hz (400.9 samples at 44.1 kHz), so
 * the RMS does not ripple with the phase each window starts at. */
constexpr size_t kEnvelopeWindow = 401;
/* First 9.1 ms: start-up transient. The attack is compared from window 1 on;
 * in window 0 reSIDfp's 6581 is 6.3 dB quieter than ours, the 8580 0.48 dB apart. */
constexpr size_t kSkipWindows = 1;
constexpr size_t kSustainFirst = 17;  /* sustain plateau from 154.6 ms ... */
constexpr size_t kSustainLast = 43;   /* ... to 391.0 ms (exclusive); gate off at 400 ms */

class Reference : public ::testing::TestWithParam<sid_model> {};

std::vector<RegWrite> saw_a4(uint8_t ad, uint8_t sr, uint32_t gate_off_frame) {
    return {{0, 0x18, 0x0f}, {0, 0x00, 0x45}, {0, 0x01, 0x1d},
            {0, 0x05, ad},   {0, 0x06, sr},   {0, 0x04, 0x21},
            {gate_off_frame, 0x04, 0x20}};
}

double cents(double a, double b) { return std::fabs(1200.0 * std::log2(a / b)); }

} // namespace

TEST_P(Reference, UnfilteredSawPitchMatches) {
    auto writes = saw_a4(0x00, 0xf0, 1000);
    auto ours = render_sidou(GetParam(), writes, 60);
    auto ref = render_residfp(GetParam(), writes, 60, /*decimate=*/true);
    constexpr size_t kStart = 4410, kLength = 44100;
    ASSERT_GE(ours.size(), kStart + kLength);
    ASSERT_GE(ref.size(), kStart + kLength);
    std::vector<float> a(ours.begin() + kStart, ours.begin() + kStart + kLength);
    std::vector<float> b(ref.begin() + kStart, ref.begin() + kStart + kLength);
    const double fa = estimate_f0(a, kRefSampleRate, 400, 480);
    const double fb = estimate_f0(b, residfp_decimate_rate(kRefSampleRate), 400, 480);
    EXPECT_LT(cents(fa, kSawA4Hz), kMaxPitchCents) << "sidou " << fa << " Hz";
    EXPECT_LT(cents(fb, kSawA4Hz), kMaxPitchCents)
        << "reSIDfp " << fb << " Hz; the reference itself is off, check the harness";
    EXPECT_LT(cents(fa, fb), kMaxPitchCents) << "sidou " << fa << " Hz vs reSIDfp " << fb << " Hz";
}

TEST_P(Reference, EnvelopeShapeMatches) {
    ASSERT_EQ(kEnvelopeWindow, (size_t)std::lround(4.0 * kRefSampleRate / kSawA4Hz));
    auto writes = saw_a4(0x29, 0xa9, 20);
    auto ours = rms_envelope_db(render_sidou(GetParam(), writes, 50), kEnvelopeWindow);
    auto ref = rms_envelope_db(render_residfp(GetParam(), writes, 50, /*decimate=*/true),
                               kEnvelopeWindow);
    ASSERT_GE(ours.size(), kSustainLast);
    ASSERT_GE(ref.size(), kSustainLast);
    ours = normalized_to_mean(ours, kSustainFirst, kSustainLast);
    ref = normalized_to_mean(ref, kSustainFirst, kSustainLast);
    const double tolerance = GetParam() == SID_MODEL_6581 ? kMaxEnvelopeDb6581 : kMaxEnvelopeDb8580;
    EXPECT_LT(max_envelope_diff_db(ours, ref, kEnvelopeFloorDb, kSkipWindows), tolerance);
}

INSTANTIATE_TEST_SUITE_P(Models, Reference,
                         ::testing::Values(SID_MODEL_6581, SID_MODEL_8580),
                         [](const auto& info) {
                             return info.param == SID_MODEL_6581 ? "MOS6581" : "CSG8580";
                         });
