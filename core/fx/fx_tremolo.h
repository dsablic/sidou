// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_TREMOLO_H
#define SIDOU_FX_TREMOLO_H

/* Tremolo and auto-pan, mono in, stereo out, from one sine LFO shaped from sine towards
 * square by driving the soft clip (sidou_soft_clip) and renormalising its peak to 1.
 *
 * - Tremolo: gain 1 - depth (1 - lfo) / 2 on both sides.
 * - Auto-pan: equal power, pan angle pi/4 (1 + depth lfo) between the sides, scaled by
 *   sqrt 2 so the centre (depth 0) is the input on both: L^2 + R^2 = 2 x^2 always. So a
 *   side panned fully peaks at sqrt 2 times the input (+3 dB): a full-scale input can
 *   clip there. Kept for the constant loudness and the unchanged centre; the slot's Mix
 *   or a lower Amount (width) leaves headroom.
 *
 * - Amount: depth (tremolo) or width (auto-pan);
 * - A: rate, 0.1 .. 10 Hz (log);
 * - B: shape, sine (0) to near-square (1).
 * Any amounts in [0, 1] may arrive on any sample. */

#include <stdint.h>

#include "fx_util.h"
#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_TREMOLO_MIN_HZ 0.1f
#define SID_FX_TREMOLO_RATE_OCTAVES 6.643856f /* log2(10 / 0.1) */
#define SID_FX_TREMOLO_SQUARE 5.0f            /* extra drive into the clip at B = 1 */

typedef struct {
    uint32_t phase, inc;
    float hz_inc, rate;
    float shape, drive, norm; /* derived from B */
    float lfo;                /* last shaped LFO value, -1 .. 1 */
} sid_fx_tremolo;

static inline void sid_fx_tremolo_set_rate(sid_fx_tremolo* t, float rate) {
    t->rate = rate;
    float hz = SID_FX_TREMOLO_MIN_HZ * sid_fx_exp2(SID_FX_TREMOLO_RATE_OCTAVES * rate);
    t->inc = (uint32_t)(int32_t)(hz * t->hz_inc) << 1;
}

static inline void sid_fx_tremolo_set_shape(sid_fx_tremolo* t, float shape) {
    t->shape = shape;
    t->drive = 1.0f + SID_FX_TREMOLO_SQUARE * shape;
    sid_fx_recip_track(&t->norm, sidou_soft_clip(t->drive));
}

static inline void sid_fx_tremolo_init(sid_fx_tremolo* t, uint32_t sample_rate) {
    t->phase = 0;
    t->hz_inc = 2147483648.0f / (float)sample_rate;
    t->norm = 1.0f;
    t->lfo = 0.0f;
    sid_fx_tremolo_set_rate(t, 0.5f);
    t->norm = 1.0f / sidou_soft_clip(1.0f);
    sid_fx_tremolo_set_shape(t, 0.0f);
}

static inline float sid_fx_tremolo_lfo(sid_fx_tremolo* t, float rate, float shape) {
    if (rate != t->rate) {
        sid_fx_tremolo_set_rate(t, rate);
    }
    if (shape != t->shape) {
        sid_fx_tremolo_set_shape(t, shape);
    }
    t->lfo = sidou_soft_clip(t->drive * sid_fx_sin(t->phase)) * t->norm;
    t->phase += t->inc;
    return t->lfo;
}

static inline void sid_fx_tremolo_tick(sid_fx_tremolo* t, float x, float depth, float rate,
                                       float shape, float* l, float* r) {
    float lfo = sid_fx_tremolo_lfo(t, rate, shape);
    float y = x * (1.0f - 0.5f * depth * (1.0f - lfo));
    *l = y;
    *r = y;
}

static inline void sid_fx_autopan_tick(sid_fx_tremolo* t, float x, float width, float rate,
                                       float shape, float* l, float* r) {
    float lfo = sid_fx_tremolo_lfo(t, rate, shape);
    /* angle pi/4 (1 + width lfo) as a phase: a quarter turn is 2^30. */
    uint32_t angle = (uint32_t)(int32_t)(536870912.0f * (1.0f + width * lfo));
    const float root2 = 1.41421356f;
    *l = root2 * x * sid_fx_cos(angle);
    *r = root2 * x * sid_fx_sin(angle);
}

#ifdef __cplusplus
}
#endif

#endif
