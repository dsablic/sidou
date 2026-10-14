// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>

#include "render.h"
#include "signal_metrics.h"

namespace {

/* M0 tolerances. Tighten them in M1; never loosen them to make a failure pass
 * without writing down why. */
constexpr double kMaxPitchCents = 0.1; /* measured at M0: 0.03 */
constexpr double kMaxEnvelopeDb8580 = 1.0; /* measured at M0: 0.64 */
/* reSIDfp models the 6581's non-linear envelope DAC, which M0 does not; the
 * release tail drifts ~2 dB apart by -20 dB. M1 adds the DAC and tightens this. */
constexpr double kMaxEnvelopeDb6581 = 2.5; /* measured at M0: 2.26 */
constexpr double kEnvelopeFloorDb = -20.0;
constexpr size_t kSkipWindows = 5;      /* first 50 ms: start-up transients */
constexpr size_t kSustainWindow = 30;   /* 300 ms, in the sustain phase */

class Reference : public ::testing::TestWithParam<sid_model> {};

std::vector<RegWrite> saw_a4(uint8_t ad, uint8_t sr, uint32_t gate_off_frame) {
    return {{0, 0x18, 0x0f}, {0, 0x00, 0x45}, {0, 0x01, 0x1d},
            {0, 0x05, ad},   {0, 0x06, sr},   {0, 0x04, 0x21},
            {gate_off_frame, 0x04, 0x20}};
}

} // namespace

TEST_P(Reference, UnfilteredSawPitchMatches) {
    auto writes = saw_a4(0x00, 0xf0, 1000);
    auto ours = render_sidou(GetParam(), writes, 60);
    auto ref = render_residfp(GetParam(), writes, 60, /*exact_rate=*/true);
    std::vector<float> a(ours.begin() + 4410, ours.begin() + 4410 + 44100);
    std::vector<float> b(ref.begin() + 4410, ref.begin() + 4410 + 44100);
    const double expected = 0x1d45 * (double)SID_CLOCK_PAL / 16777216.0;
    const double fa = estimate_f0(a, kRefSampleRate, 400, 480);
    const double fb = estimate_f0(b, kRefSampleRate, 400, 480);
    EXPECT_NEAR(fb, expected, 0.05) << "the reference itself is off; check the harness";
    EXPECT_LT(std::fabs(1200.0 * std::log2(fa / fb)), kMaxPitchCents)
        << "sidou " << fa << " Hz vs reSIDfp " << fb << " Hz";
}

TEST_P(Reference, EnvelopeShapeMatches) {
    auto writes = saw_a4(0x29, 0xa9, 20);
    auto ours = normalized(rms_envelope_db(render_sidou(GetParam(), writes, 50), kRefSampleRate, 10),
                           kSustainWindow);
    auto ref = normalized(rms_envelope_db(render_residfp(GetParam(), writes, 50), kRefSampleRate, 10),
                          kSustainWindow);
    const double tolerance = GetParam() == SID_MODEL_6581 ? kMaxEnvelopeDb6581 : kMaxEnvelopeDb8580;
    EXPECT_LT(max_envelope_diff_db(ours, ref, kEnvelopeFloorDb, kSkipWindows), tolerance);
}

INSTANTIATE_TEST_SUITE_P(Models, Reference,
                         ::testing::Values(SID_MODEL_6581, SID_MODEL_8580),
                         [](const auto& info) {
                             return info.param == SID_MODEL_6581 ? "MOS6581" : "CSG8580";
                         });
