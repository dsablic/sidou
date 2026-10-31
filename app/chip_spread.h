// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CHIP_SPREAD_H
#define SIDOU_CHIP_SPREAD_H

#include <stdint.h>

#include "chip_mix.h"
#include "live.h"
#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define CHIP_SPREAD_MAX 100u
#define CHIP_SPREAD_BLOCK CHIP_MIX_FADE

void chip_spread_gains(uint32_t spread, float* mid, float* side);

void chip_spread_side(int16_t* side, const int16_t* a, const int16_t* b, chip_mix_fade fade);

void chip_spread_apply(int16_t* out, const float* in, const int16_t* side, float m0, float m1,
                       float k0, float k1);

#define CHIP_SPREAD_GLIDE 32u
#define CHIP_SPREAD_ON 16u
#define CHIP_SPREAD_OFF 4u
#define CHIP_SPREAD_HOLD 16u
_Static_assert((CHIP_SPREAD_ON & (CHIP_SPREAD_ON - 1u)) == 0u &&
                   (CHIP_SPREAD_OFF & (CHIP_SPREAD_OFF - 1u)) == 0u,
               "the thresholds are powers of two");

typedef struct {
    uint8_t held;
    uint8_t share;
} chip_spread_glide;

uint32_t chip_spread_glide_step(chip_spread_glide* g, uint32_t a, uint32_t b);

static inline uint32_t chip_spread_level(const sid* c) {
    return (uint32_t)c->env[0].counter | c->env[1].counter | c->env[2].counter;
}

void chip_spread_set(live* l, uint32_t spread);

#ifdef __cplusplus
}
#endif

#endif
