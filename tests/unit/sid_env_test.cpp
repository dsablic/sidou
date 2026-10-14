// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

extern "C" {
#include "sid/sid_env.h"
}

namespace {

sid_env gated(uint8_t ad, uint8_t sr) {
    sid_env e;
    sid_env_reset(&e);
    sid_env_write_ad(&e, ad);
    sid_env_write_sr(&e, sr);
    sid_env_set_gate(&e, 1);
    return e;
}

} // namespace

TEST(SidEnv, StartsSilentAndFrozen) {
    sid_env e;
    sid_env_reset(&e);
    sid_env_advance(&e, 100000);
    EXPECT_EQ(e.counter, 0);
}

TEST(SidEnv, FastestAttackStepsEveryNineCycles) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 8);
    EXPECT_EQ(e.counter, 0);
    sid_env_advance(&e, 1);
    EXPECT_EQ(e.counter, 1);
}

TEST(SidEnv, AttackReachesFullScaleThenDecays) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 9 * 255);
    EXPECT_EQ(e.counter, 0xff);
    EXPECT_EQ(e.state, SID_ENV_DECAY_SUSTAIN);
}

TEST(SidEnv, DecayStopsAtSustainLevel) {
    sid_env e = gated(0x00, 0x80);
    sid_env_advance(&e, 9 * 255 + 9 * (0xff - 0x88));
    EXPECT_EQ(e.counter, 0x88);
    sid_env_advance(&e, 100000);
    EXPECT_EQ(e.counter, 0x88);
}

TEST(SidEnv, ExponentialDividerDoublesStepTimeBelow0x5d) {
    sid_env e = gated(0x00, 0x00);
    sid_env_advance(&e, 9 * 255); // finish the attack, which also passes 0x5d
    ASSERT_EQ(e.state, SID_ENV_DECAY_SUSTAIN);
    while (e.counter != 0x5d) {
        sid_env_advance(&e, 1);
    }
    uint32_t cycles = 0;
    while (e.counter == 0x5d) {
        sid_env_advance(&e, 1);
        ++cycles;
    }
    EXPECT_EQ(cycles, 18u);
}

TEST(SidEnv, ReleaseFreezesAtZero) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 9 * 255);
    sid_env_set_gate(&e, 0);
    sid_env_advance(&e, 2000000);
    EXPECT_EQ(e.counter, 0);
}

TEST(SidEnv, LoweringThePeriodBelowTheCounterWrapsThrough0x7fff) {
    sid_env e = gated(0xf0, 0xf0);   // attack 15: period 31251
    sid_env_advance(&e, 1000);
    ASSERT_EQ(e.counter, 0);
    sid_env_write_ad(&e, 0x00);      // period 9, but the counter is already at 1000
    sid_env_advance(&e, 0x7fff - 1000 + 9 - 1);
    EXPECT_EQ(e.counter, 0);
    sid_env_advance(&e, 1);
    EXPECT_EQ(e.counter, 1);
}
