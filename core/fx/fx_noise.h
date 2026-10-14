// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_NOISE_H
#define SIDOU_FX_NOISE_H

/* Noise floor: tape-like hiss added to the signal. White noise from a seeded xorshift32
 * (so host and target agree) through 1 - 0.6 z^-1, which tilts it towards the highs
 * (-8 dB at DC, +4 dB at Nyquist relative to white). RMS SID_FX_NOISE_RMS at amount 1,
 * linear in the amount. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_NOISE_RMS 0.0158f /* full scale 1 (-36 dBFS) */
#define SID_FX_NOISE_TILT 0.6f
/* Uniform [-1, 1) has RMS 1/sqrt(3); the tilt filter multiplies it by sqrt(1 + 0.36). */
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

/* x is normalised to [-1, 1); amount is in (0, 1]. */
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
