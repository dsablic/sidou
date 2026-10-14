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

TEST(SidEnv, RaisingSustainAboveCounterDuringDecayFallsToZeroAndFreezes) {
    sid_env e = gated(0x00, 0x00);
    sid_env_advance(&e, 9 * 255);
    ASSERT_EQ(e.state, SID_ENV_DECAY_SUSTAIN);
    while (e.counter > 0x80) {
        sid_env_advance(&e, 1);
    }
    sid_env_write_sr(&e, 0xf0); // sustain level 0xff, above the counter
    sid_env_advance(&e, 1000000);
    EXPECT_EQ(e.counter, 0);
    EXPECT_EQ(e.hold_zero, 1);
    sid_env_advance(&e, 1000000);
    EXPECT_EQ(e.counter, 0);
}

TEST(SidEnv, GateOffMidAttackKeepsAttackExponentialPeriod) {
    sid_env e = gated(0x00, 0xf0);
    // Gate off before attack reaches 0x06, the first threshold that changes the period.
    sid_env_advance(&e, 9 * 5);
    ASSERT_EQ(e.counter, 5);
    ASSERT_EQ(e.exp_period, 1u);
    sid_env_set_gate(&e, 0);
    // A release from sustain would step every 30 rate periods below 0x06; this one keeps 1.
    sid_env_advance(&e, 9);
    EXPECT_EQ(e.counter, 4);
    sid_env_advance(&e, 9);
    EXPECT_EQ(e.counter, 3);
    EXPECT_EQ(e.exp_period, 1u);
}

TEST(SidEnv, GateOnAfterFreezeClearsHoldZeroAndRestartsAttack) {
    sid_env e = gated(0x00, 0xf0);
    sid_env_advance(&e, 9 * 255);
    sid_env_set_gate(&e, 0);
    sid_env_advance(&e, 2000000);
    ASSERT_EQ(e.counter, 0);
    ASSERT_EQ(e.hold_zero, 1);
    ASSERT_LT(e.rate_counter, 9u);
    sid_env_set_gate(&e, 1);
    EXPECT_EQ(e.hold_zero, 0);
    EXPECT_EQ(e.state, SID_ENV_ATTACK);
    sid_env_advance(&e, 9);
    EXPECT_EQ(e.counter, 1);
}

TEST(SidEnv, AdWriteEqualToRateCounterWaitsFullWrap) {
    sid_env e = gated(0x10, 0xf0); // attack 1: period 32
    sid_env_advance(&e, 9);
    ASSERT_EQ(e.rate_counter, 9u);
    sid_env_write_ad(&e, 0x00); // period 9 == rate_counter: the match was already missed
    sid_env_advance(&e, 0x7fff - 1);
    EXPECT_EQ(e.counter, 0);
    sid_env_advance(&e, 1);
    EXPECT_EQ(e.counter, 1);
}

namespace {

// Independent per-cycle model of reSID 0.16 EnvelopeGenerator::clock().
struct ref_env {
    static constexpr uint32_t period_table[16] = {9,   32,  63,   95,   149,  220,   267,   313,
                                                  392, 977, 1954, 3126, 3907, 11720, 19532, 31251};
    uint32_t rate_counter = 0;
    uint32_t rate_period = period_table[0];
    uint32_t exp_counter = 0;
    uint32_t exp_period = 1;
    sid_env_state state = SID_ENV_RELEASE;
    uint32_t counter = 0;
    uint32_t attack = 0, decay = 0, sustain = 0, release = 0;
    bool gate = false;
    bool hold_zero = true;

    void write_ad(uint8_t v) {
        attack = v >> 4;
        decay = v & 0x0f;
        if (state == SID_ENV_ATTACK) {
            rate_period = period_table[attack];
        } else if (state == SID_ENV_DECAY_SUSTAIN) {
            rate_period = period_table[decay];
        }
    }

    void write_sr(uint8_t v) {
        sustain = v >> 4;
        release = v & 0x0f;
        if (state == SID_ENV_RELEASE) {
            rate_period = period_table[release];
        }
    }

    void set_gate(bool next) {
        if (!gate && next) {
            state = SID_ENV_ATTACK;
            rate_period = period_table[attack];
            hold_zero = false;
        } else if (gate && !next) {
            state = SID_ENV_RELEASE;
            rate_period = period_table[release];
        }
        gate = next;
    }

    void clock() {
        if (++rate_counter & 0x8000) {
            rate_counter = (rate_counter + 1) & 0x7fff;
        }
        if (rate_counter != rate_period) {
            return;
        }
        rate_counter = 0;
        if (state != SID_ENV_ATTACK && ++exp_counter != exp_period) {
            return;
        }
        exp_counter = 0;
        if (hold_zero) {
            return;
        }
        switch (state) {
        case SID_ENV_ATTACK:
            counter = (counter + 1) & 0xff;
            if (counter == 0xff) {
                state = SID_ENV_DECAY_SUSTAIN;
                rate_period = period_table[decay];
            }
            break;
        case SID_ENV_DECAY_SUSTAIN:
            if (counter != sustain * 0x11) {
                --counter;
            }
            break;
        case SID_ENV_RELEASE:
            counter = (counter - 1) & 0xff;
            break;
        }
        switch (counter) {
        case 0xff: exp_period = 1; break;
        case 0x5d: exp_period = 2; break;
        case 0x36: exp_period = 4; break;
        case 0x1a: exp_period = 8; break;
        case 0x0e: exp_period = 16; break;
        case 0x06: exp_period = 30; break;
        case 0x00:
            exp_period = 1;
            hold_zero = true;
            break;
        default: break;
        }
    }
};

struct lcg {
    uint32_t x;
    uint32_t next() {
        x = x * 1664525u + 1013904223u;
        return x >> 8;
    }
};

} // namespace

TEST(SidEnv, JumpAheadMatchesPerCycleReference) {
    sid_env e;
    sid_env_reset(&e);
    ref_env r;
    lcg rng{0x51d0u};
    for (int step = 0; step < 4000; ++step) {
        uint32_t op = rng.next() % 16;
        if (op == 0) {
            uint8_t v = (uint8_t)rng.next();
            sid_env_write_ad(&e, v);
            r.write_ad(v);
        } else if (op == 1) {
            uint8_t v = (uint8_t)rng.next();
            sid_env_write_sr(&e, v);
            r.write_sr(v);
        } else if (op <= 3) {
            bool g = !r.gate;
            sid_env_set_gate(&e, g ? 1 : 0);
            r.set_gate(g);
        } else {
            uint32_t cycles = op <= 5 ? rng.next() % 70001 : 22 + rng.next() % 2;
            sid_env_advance(&e, cycles);
            for (uint32_t i = 0; i < cycles; ++i) {
                r.clock();
            }
        }
        ASSERT_EQ(e.counter, r.counter) << "step " << step;
        ASSERT_EQ(e.rate_counter, r.rate_counter) << "step " << step;
        ASSERT_EQ(e.exp_counter, r.exp_counter) << "step " << step;
        ASSERT_EQ(e.exp_period, r.exp_period) << "step " << step;
        ASSERT_EQ(e.rate_period, r.rate_period) << "step " << step;
        ASSERT_EQ(e.state, r.state) << "step " << step;
        ASSERT_EQ(e.hold_zero != 0, r.hold_zero) << "step " << step;
    }
}
