// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_TAPE_H
#define SIDOU_FX_TAPE_H

/* Tape saturation. A pre-emphasis shelf lifts the highs, a biased soft clip saturates
 * (the bias makes it asymmetric, so even harmonics appear), and the exact inverse of the
 * shelf brings the highs back down: the high end hits the clip harder, so it compresses
 * first. Drive and makeup are tied (makeup = 1 / drive), so small signals keep unity
 * gain. The clip's own DC (bias, rectification) is removed by a DC blocker on the wet
 * difference only.
 *
 * The clip is (1 - a) u + a c(u), with c = sidou_soft_clip: u - (4/27) u^3 up to
 * |u| = 1.5, where its slope reaches 0, and flat beyond. So it is smooth (no corner) at
 * every amount, the identity at a = 0, and a full soft clip at a = 1. c is evaluated with
 * first-order antiderivative anti-aliasing (ADAA): the mean of c over the segment from
 * the previous input to this one, which the polynomial gives in closed form. That delays
 * the clipped part by half a sample against the linear part, a gentle dip near Nyquist
 * at intermediate drive (-0.35 dB at 8 kHz at a = 0.5); at a = 1 there is no linear part.
 *
 * At amount a: drive 1 + 2a, shelf boost 1 + 2a above ~3 kHz, bias 0.15 a. The derived
 * coefficients are recomputed (with real divisions) whenever the amount changes, so any
 * amount in [0, 1] may arrive on any sample. */

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
#define SID_FX_TAPE_KNEE 1.5f /* where the soft clip's slope reaches 0 */

typedef struct {
    float b;      /* shelf one-pole coefficient */
    float dc_r;   /* DC blocker pole */
    float lp_pre; /* pre-emphasis low-pass state */
    float lp_de;  /* de-emphasis low-pass state */
    float dc_x, dc_y;
    float u_prev; /* previous clip input, for ADAA */
    /* Derived from `amount`. */
    float amount, drive, makeup, boost, g, deemph, bias, bias_out;
} sid_fx_tape;

/* The soft clip c(u) = sidou_soft_clip(u) and its antiderivative. */
static inline float sid_fx_tape_clip(float u) {
    if (u > SID_FX_TAPE_KNEE) {
        return 1.0f;
    }
    if (u < -SID_FX_TAPE_KNEE) {
        return -1.0f;
    }
    return u - (4.0f / 27.0f) * u * u * u;
}

static inline float sid_fx_tape_clip_int(float u) {
    float a = sid_fx_abs(u);
    if (a > SID_FX_TAPE_KNEE) {
        return 0.9375f + (a - SID_FX_TAPE_KNEE); /* C(1.5) = 1.125 - 0.1875 */
    }
    float a2 = a * a;
    return 0.5f * a2 - (1.0f / 27.0f) * a2 * a2;
}

/* Mean of c over [p, u]: (C(u) - C(p)) / (u - p), in closed form where both ends are on
 * the same piece, so the common case needs no division and is never ill-conditioned. */
static inline float sid_fx_tape_clip_adaa(float u, float p) {
    const float k = SID_FX_TAPE_KNEE;
    if (u <= k && u >= -k && p <= k && p >= -k) {
        return 0.5f * (u + p) - (1.0f / 27.0f) * (u + p) * (u * u + p * p);
    }
    if (u > k && p > k) {
        return 1.0f;
    }
    if (u < -k && p < -k) {
        return -1.0f;
    }
    float d = u - p;
    if (d < 1e-3f && d > -1e-3f) {
        return sid_fx_tape_clip(0.5f * (u + p));
    }
    return (sid_fx_tape_clip_int(u) - sid_fx_tape_clip_int(p)) / d;
}

static inline void sid_fx_tape_set_amount(sid_fx_tape* t, float amount) {
    t->amount = amount;
    t->drive = 1.0f + SID_FX_TAPE_DRIVE * amount;
    t->makeup = 1.0f / t->drive;
    t->boost = SID_FX_TAPE_EMPHASIS * amount;
    t->g = t->boost * (1.0f - t->b);
    t->deemph = 1.0f / (1.0f + t->g);
    t->bias = SID_FX_TAPE_BIAS * amount;
    /* The clip's output at zero input, removed so silence stays silent (the bias is
     * below the knee, so that is the cubic). */
    t->bias_out = t->bias - amount * (4.0f / 27.0f) * t->bias * t->bias * t->bias;
}

static inline void sid_fx_tape_reset(sid_fx_tape* t) {
    t->lp_pre = 0.0f;
    t->lp_de = 0.0f;
    t->dc_x = 0.0f;
    t->dc_y = 0.0f;
    t->u_prev = 0.0f;
    sid_fx_tape_set_amount(t, 0.0f);
}

static inline void sid_fx_tape_init(sid_fx_tape* t, uint32_t sample_rate) {
    float w = 2.0f * SIDOU_PI * SID_FX_TAPE_SHELF_HZ / (float)sample_rate;
    t->b = w / (1.0f + w);
    t->dc_r = 1.0f - 2.0f * SIDOU_PI * SID_FX_TAPE_DC_HZ / (float)sample_rate;
    sid_fx_tape_reset(t);
}

/* x is normalised to [-1, 1); amount is in [0, 1]. */
static inline float sid_fx_tape_tick(sid_fx_tape* t, float x, float amount) {
    if (amount != t->amount) {
        sid_fx_tape_set_amount(t, amount);
    }

    /* Pre-emphasis: x + boost (x - lowpass(x)). */
    float lp = t->lp_pre + t->b * (x - t->lp_pre);
    float pre = x + t->boost * (x - lp);
    t->lp_pre = sid_fx_flush(lp);

    float u = t->drive * pre + t->bias;
    float clip = (1.0f - amount) * u + amount * sid_fx_tape_clip_adaa(u, t->u_prev);
    t->u_prev = u;
    float s = (clip - t->bias_out) * t->makeup;

    /* De-emphasis, the exact inverse of the shelf: pre = x (1 + g) - g l[n-1] with
     * g = boost (1 - b), so x = (pre + g l[n-1]) / (1 + g). */
    float y = (s + t->g * t->lp_de) * t->deemph;
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
