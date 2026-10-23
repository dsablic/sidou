// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_spread.h"

#include "fx/fx_limit.h"

_Static_assert(CHIP_SPREAD_BLOCK == 128u, "the gains' steps are 1/128");

void chip_spread_gains(uint32_t spread, float* mid, float* side) {
    if (spread > CHIP_SPREAD_MAX) {
        spread = CHIP_SPREAD_MAX;
    }
    /* phi in [0, pi / 4]: Taylor to phi^9 and phi^8, the terms left under 3e-8 there. */
    float p = (float)spread * (0.785398163f / (float)CHIP_SPREAD_MAX);
    float q = p * p;
    *side = p * (1.0f - q / 6.0f * (1.0f - q / 20.0f * (1.0f - q / 42.0f * (1.0f - q / 72.0f))));
    *mid = 1.0f - q / 2.0f * (1.0f - q / 12.0f * (1.0f - q / 30.0f * (1.0f - q / 56.0f)));
}

void chip_spread_side(int16_t* side, const int16_t* a, const int16_t* b, chip_mix_fade fade) {
    for (uint32_t i = 0; i < CHIP_SPREAD_BLOCK; i++) {
        int32_t w = fade == CHIP_MIX_FULL ? 128
                    : fade == CHIP_MIX_IN ? (int32_t)i + 1
                                          : (int32_t)CHIP_MIX_FADE - 1 - (int32_t)i;
        side[i] = (int16_t)(((int32_t)a[i] - ((b[i] * w) >> 7)) >> 1);
    }
}

/* A level in LSBs to int16 through the limiter, rounded half away from zero: as
 * sid_fx_to_i16_limited converts the slots' output. */
static int16_t spread_out(float o) {
    if (o > (float)SID_FX_LIMIT_KNEE || o < -(float)SID_FX_LIMIT_KNEE) {
        return sid_fx_limit_round(o);
    }
    o += o >= 0.0f ? 0.5f : -0.5f;
    return (int16_t)o;
}

void chip_spread_apply(int16_t* out, const float* in, const int16_t* side, float m0, float m1,
                       float k0, float k1) {
    /* The mid in LSBs: x 32768, a power of two, so a gain of 1 is exactly the slots' level. */
    const float dm = (m1 - m0) * (32768.0f / (float)CHIP_SPREAD_BLOCK);
    const float dk = (k1 - k0) * (1.0f / (float)CHIP_SPREAD_BLOCK);
    float m = m0 * 32768.0f, k = k0;
    for (uint32_t f = 0; f < CHIP_SPREAD_BLOCK; f++) {
        m += dm;
        k += dk;
        float s = (float)side[f] * k;
        out[2u * f] = spread_out(in[2u * f] * m + s);
        out[2u * f + 1u] = spread_out(in[2u * f + 1u] * m - s);
    }
}

void chip_spread_set(live* l, uint32_t spread) {
    l->spread = (uint8_t)(spread < CHIP_SPREAD_MAX ? spread : CHIP_SPREAD_MAX);
    l->settings_dirty = 1;
}
