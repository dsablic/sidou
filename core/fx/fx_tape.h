// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_TAPE_H
#define SIDOU_FX_TAPE_H

/* Tape saturation. A pre-emphasis shelf lifts the highs, a biased cubic soft clip
 * saturates (the bias makes it asymmetric, so even harmonics appear), and the exact
 * inverse of the shelf brings the highs back down: the high end hits the clip harder, so
 * it compresses first. Drive and makeup are tied (makeup = 1 / drive), so small signals
 * keep unity gain. The clip's own DC (bias, rectification) is removed by a DC blocker on
 * the wet difference only. Every term scales with the amount, so it is the identity at 0.
 *
 * At amount a: drive 1 + 2a, shelf boost 1 + 2a above ~3 kHz, bias 0.15 a, cubic
 * coefficient (4/27) a (at a = 1 the clip is sidou_soft_clip). */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_TAPE_DRIVE 2.0f
#define SID_FX_TAPE_EMPHASIS 2.0f
#define SID_FX_TAPE_BIAS 0.15f
#define SID_FX_TAPE_SHELF_HZ 3000.0f
#define SID_FX_TAPE_DC_HZ 10.0f

typedef struct {
    float b;      /* shelf one-pole coefficient */
    float dc_r;   /* DC blocker pole */
    float lp_pre; /* pre-emphasis low-pass state */
    float lp_de;  /* de-emphasis low-pass state */
    float dc_x, dc_y;
    float makeup; /* tracks 1 / drive */
    float deemph; /* tracks 1 / (1 + boost (1 - b)) */
} sid_fx_tape;

static inline void sid_fx_tape_reset(sid_fx_tape* t) {
    t->lp_pre = 0.0f;
    t->lp_de = 0.0f;
    t->dc_x = 0.0f;
    t->dc_y = 0.0f;
    t->makeup = 1.0f;
    t->deemph = 1.0f;
}

static inline void sid_fx_tape_init(sid_fx_tape* t, uint32_t sample_rate) {
    float w = 2.0f * SIDOU_PI * SID_FX_TAPE_SHELF_HZ / (float)sample_rate;
    t->b = w / (1.0f + w);
    t->dc_r = 1.0f - 2.0f * SIDOU_PI * SID_FX_TAPE_DC_HZ / (float)sample_rate;
    sid_fx_tape_reset(t);
}

/* x is normalised to [-1, 1); amount is in (0, 1]. */
static inline float sid_fx_tape_tick(sid_fx_tape* t, float x, float amount) {
    float drive = 1.0f + SID_FX_TAPE_DRIVE * amount;
    float boost = SID_FX_TAPE_EMPHASIS * amount;
    float bias = SID_FX_TAPE_BIAS * amount;
    float k = (4.0f / 27.0f) * amount;

    /* Pre-emphasis: x + boost (x - lowpass(x)). */
    float lp = t->lp_pre;
    float pre = x + boost * (x - (lp + t->b * (x - lp)));
    t->lp_pre = sid_fx_flush(lp + t->b * (x - lp));

    float u = drive * pre + bias;
    if (u > 1.5f) {
        u = 1.5f;
    } else if (u < -1.5f) {
        u = -1.5f;
    }
    t->makeup = sid_fx_recip_step(t->makeup, drive);
    float s = ((u - k * u * u * u) - (bias - k * bias * bias * bias)) * t->makeup;

    /* De-emphasis, the exact inverse of the shelf: pre = x (1 + g) - g l[n-1] with
     * g = boost (1 - b), so x = (pre + g l[n-1]) / (1 + g). */
    float g = boost * (1.0f - t->b);
    t->deemph = sid_fx_recip_step(t->deemph, 1.0f + g);
    float y = (s + g * t->lp_de) * t->deemph;
    t->lp_de = sid_fx_flush(t->lp_de + t->b * (y - t->lp_de));

    float diff = y - x;
    t->dc_y = sid_fx_flush(diff - t->dc_x + t->dc_r * t->dc_y);
    t->dc_x = diff;
    return x + t->dc_y;
}

#ifdef __cplusplus
}
#endif

#endif
