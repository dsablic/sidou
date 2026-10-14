// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_PHASER_H
#define SIDOU_FX_PHASER_H

/* Phaser, mono in, stereo out: per side, six first-order allpasses sharing one
 * coefficient, swept by a sine over a log-frequency table, with feedback from the chain's
 * output to its input; the output is the input plus the chain, which puts notches where
 * the chain turns the phase by odd multiples of 180 degrees. The right side's LFO runs
 * 90 degrees on. Reimplemented from Zolzer, DAFX (phaser) and Zavalishin, The Art of VA
 * Filter Design (first-order allpass).
 *
 * - Amount: depth, the sweep around the middle of MIN_HZ .. MAX_HZ (log);
 * - A: rate, 0.05 .. 5 Hz (log);
 * - B: feedback, 0 .. 0.7.
 * Any amounts in [0, 1] may arrive on any sample. */

#include <stdint.h>

#include "fx_util.h"
#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_PHASER_STAGES 6
#define SID_FX_PHASER_MIN_HZ 150.0f
#define SID_FX_PHASER_LN_RANGE 3.4011974f /* ln(4500 / 150) */
#define SID_FX_PHASER_STEPS 32u
#define SID_FX_PHASER_MIN_RATE_HZ 0.05f
#define SID_FX_PHASER_RATE_OCTAVES 6.643856f /* log2(5 / 0.05) */
#define SID_FX_PHASER_FEEDBACK 0.7f

typedef struct {
    float table[SID_FX_PHASER_STEPS + 1u]; /* allpass coefficient over the sweep */
    float s_l[SID_FX_PHASER_STAGES], s_r[SID_FX_PHASER_STAGES];
    float fb_l, fb_r;
    uint32_t phase, inc;
    float hz_inc, rate;
    float a_l, a_r; /* last coefficients */
} sid_fx_phaser;

static inline void sid_fx_phaser_set_rate(sid_fx_phaser* p, float rate) {
    p->rate = rate;
    float hz = SID_FX_PHASER_MIN_RATE_HZ * sid_fx_exp2(SID_FX_PHASER_RATE_OCTAVES * rate);
    p->inc = (uint32_t)(int32_t)(hz * p->hz_inc) << 1;
}

static inline void sid_fx_phaser_init(sid_fx_phaser* p, uint32_t sample_rate) {
    float fs = (float)sample_rate;
    for (uint32_t i = 0; i <= SID_FX_PHASER_STEPS; ++i) {
        float hz = SID_FX_PHASER_MIN_HZ *
                   sid_fx_exp_init(SID_FX_PHASER_LN_RANGE * (float)i / (float)SID_FX_PHASER_STEPS);
        float t = sidou_tanf(SIDOU_PI * hz / fs);
        p->table[i] = (t - 1.0f) / (t + 1.0f);
    }
    for (int k = 0; k < SID_FX_PHASER_STAGES; ++k) {
        p->s_l[k] = 0.0f;
        p->s_r[k] = 0.0f;
    }
    p->fb_l = 0.0f;
    p->fb_r = 0.0f;
    p->phase = 0;
    p->hz_inc = 2147483648.0f / fs;
    p->a_l = p->table[SID_FX_PHASER_STEPS / 2u];
    p->a_r = p->a_l;
    sid_fx_phaser_set_rate(p, 0.5f);
}

static inline float sid_fx_phaser_coef(const sid_fx_phaser* p, float pos) {
    float f = pos * (float)SID_FX_PHASER_STEPS;
    int32_t i = (int32_t)f;
    if (i >= (int32_t)SID_FX_PHASER_STEPS) {
        i = (int32_t)SID_FX_PHASER_STEPS - 1;
    }
    return sid_fx_lerp(p->table[i], p->table[i + 1], f - (float)i);
}

/* Six allpasses y = a x + s, s = x - a y (transposed direct form II). */
static inline float sid_fx_phaser_chain(float* s, float a, float x) {
    for (int k = 0; k < SID_FX_PHASER_STAGES; ++k) {
        float y = a * x + s[k];
        s[k] = sid_fx_flush(x - a * y);
        x = y;
    }
    return x;
}

/* x is normalised; depth, rate and feedback are in [0, 1]. */
static inline void sid_fx_phaser_tick(sid_fx_phaser* p, float x, float depth, float rate,
                                      float feedback, float* l, float* r) {
    if (rate != p->rate) {
        sid_fx_phaser_set_rate(p, rate);
    }
    float half = 0.5f * depth;
    p->a_l = sid_fx_phaser_coef(p, 0.5f + half * sid_fx_sin(p->phase));
    p->a_r = sid_fx_phaser_coef(p, 0.5f + half * sid_fx_cos(p->phase));
    p->phase += p->inc;
    float fb = SID_FX_PHASER_FEEDBACK * feedback;
    p->fb_l = sid_fx_phaser_chain(p->s_l, p->a_l, x + fb * p->fb_l);
    p->fb_r = sid_fx_phaser_chain(p->s_r, p->a_r, x + fb * p->fb_r);
    *l = 0.5f * (x + p->fb_l);
    *r = 0.5f * (x + p->fb_r);
}

#ifdef __cplusplus
}
#endif

#endif
