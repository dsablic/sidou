// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CRUSH_H
#define SIDOU_FX_CRUSH_H

/* Bit-crush: sample-and-hold decimation plus a coarser quantiser, both continuous in the
 * amount. At amount a the hold rate is fs (1 - (1 - MIN_RATE / fs) a) and the step is
 * 2^(a (16 - MIN_BITS)) LSB of int16, so amount 1 leaves MIN_BITS bits at MIN_RATE Hz.
 * There is no anti-alias filter: the aliasing is the sound. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CRUSH_MIN_BITS 4
#define SID_FX_CRUSH_MIN_RATE 4410.0f

typedef struct {
    float rate_depth; /* 1 - MIN_RATE / fs */
    float phase;      /* a capture happens when it reaches 1 */
    float hold;
} sid_fx_crush;

static inline void sid_fx_crush_reset(sid_fx_crush* c) {
    c->phase = 1.0f; /* capture on the first sample */
    c->hold = 0.0f;
}

static inline void sid_fx_crush_init(sid_fx_crush* c, uint32_t sample_rate) {
    c->rate_depth = 1.0f - SID_FX_CRUSH_MIN_RATE / (float)sample_rate;
    if (c->rate_depth < 0.0f) {
        c->rate_depth = 0.0f;
    }
    sid_fx_crush_reset(c);
}

/* x is normalised to [-1, 1); amount is in (0, 1]. */
static inline float sid_fx_crush_tick(sid_fx_crush* c, float x, float amount) {
    c->phase += 1.0f - c->rate_depth * amount;
    if (c->phase >= 1.0f) {
        c->phase -= 1.0f;
        /* Step 2^e LSB = 2^(e - 15) full scale; its inverse from the exact power of two
         * of the integer part and a second fit for 2^-f, so no division. */
        float e = amount * (float)(16 - SID_FX_CRUSH_MIN_BITS);
        int32_t i = (int32_t)e;
        float f = e - (float)i;
        float step = sid_fx_exp2(e) * (1.0f / 32768.0f);
        float inv = (float)(32768u >> i) * (1.0f - f * (0.6564f - 0.1564f * f));
        float v = x * inv;
        v += v >= 0.0f ? 0.5f : -0.5f;
        c->hold = (float)(int32_t)v * step;
    }
    return c->hold;
}

#ifdef __cplusplus
}
#endif

#endif
