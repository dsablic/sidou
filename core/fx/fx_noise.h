// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_NOISE_H
#define SIDOU_FX_NOISE_H

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_NOISE_RMS 0.0158f
#define SID_FX_NOISE_TILT 0.6f
#define SID_FX_NOISE_GAIN (SID_FX_NOISE_RMS * 1.4852213f)

typedef struct {
    uint32_t rng;
    float prev;
} sid_fx_noise;

static inline void sid_fx_noise_reset(sid_fx_noise* n) {
    n->prev = 0.0f;
}

static inline void sid_fx_noise_init(sid_fx_noise* n, uint32_t seed) {
    n->rng = sid_fx_rng_seed(seed);
    sid_fx_noise_reset(n);
}

static inline float sid_fx_noise_tick(sid_fx_noise* n, float x, float amount) {
    float w = sid_fx_rng_bipolar(&n->rng);
    float h = w - SID_FX_NOISE_TILT * n->prev;
    n->prev = w;
    return x + amount * SID_FX_NOISE_GAIN * h;
}

#ifdef __cplusplus
}
#endif

#endif
