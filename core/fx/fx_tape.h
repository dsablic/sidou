// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_TAPE_H
#define SIDOU_FX_TAPE_H

/* Tape saturation. A pre-emphasis shelf lifts the highs, a biased soft clip saturates
 * (the bias makes it asymmetric, so even harmonics appear), and the exact inverse of the
 * shelf brings the highs back down: the high end hits the clip harder, so it compresses
 * first. Drive and makeup are tied (makeup = 1 / (drive x the clip's slope at the
 * bias)), so small signals keep exactly unity gain. The clip's own DC (bias, rectification) is removed by a DC blocker on the wet
 * difference only.
 *
 * The clip is u + a r(u), with r(u) = c(u) - u and c = sidou_soft_clip: u - (4/27) u^3
 * up to |u| = 1.5, where its slope reaches 0, and flat beyond. So it is smooth (no
 * corner) at every amount, the identity at a = 0, and a full soft clip at a = 1. Only
 * the residual r goes through first-order antiderivative anti-aliasing (ADAA: its mean
 * over the segment from the previous input to this one, in closed form); u passes
 * undelayed, so small signals, where r is negligible, stay flat to Nyquist. The price is
 * that the residual lags u by half a sample: when a high tone is driven far past the
 * knee the two no longer cancel cleanly, and such tones clip less hard than they would
 * without ADAA.
 *
 * At amount a: drive 1 + 2a, shelf boost 1 + 2a above ~3 kHz, bias 0.15 a. The derived
 * coefficients follow the amount on every sample it changes (reciprocals by a Newton
 * step, or a division on a jump), so any amount in [0, 1] may arrive on any sample. */

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
    float amount, drive, boost, g, bias, bias_out;
    float q;      /* tracks 1 / (drive slope (1 + g)): makeup and de-emphasis together */
    float deemph; /* 1 / (1 + g) */
    float g_de;   /* g / (1 + g) */
} sid_fx_tape;

/* The soft clip's residual r(u) = sidou_soft_clip(u) - u: the distortion alone. */
static inline float sid_fx_tape_residual(float u) {
    if (u > SID_FX_TAPE_KNEE) {
        return 1.0f - u;
    }
    if (u < -SID_FX_TAPE_KNEE) {
        return -1.0f - u;
    }
    return (-4.0f / 27.0f) * u * u * u;
}

/* Means of r over [a, b] on each of its three pieces, in closed form. */
static inline float sid_fx_tape_mean_in(float a, float b) {
    return (-1.0f / 27.0f) * (a + b) * (a * a + b * b);
}

static inline float sid_fx_tape_mean_hi(float a, float b) {
    return 1.0f - 0.5f * (a + b);
}

static inline float sid_fx_tape_mean_lo(float a, float b) {
    return -1.0f - 0.5f * (a + b);
}

/* Mean of r over the segment between p and u: first-order ADAA. On one piece it is that
 * piece's closed form (no division, never ill-conditioned). A segment across a knee is
 * split there and the pieces' means are weighted by length, so nothing cancels; its
 * length is not 0, as its ends are on different pieces. */
static inline float sid_fx_tape_residual_adaa(float u, float p) {
    const float k = SID_FX_TAPE_KNEE;
    if (u <= k && u >= -k && p <= k && p >= -k) {
        return sid_fx_tape_mean_in(u, p);
    }
    if (u > k && p > k) {
        return sid_fx_tape_mean_hi(u, p);
    }
    if (u < -k && p < -k) {
        return sid_fx_tape_mean_lo(u, p);
    }
    float lo = u < p ? u : p;
    float hi = u < p ? p : u;
    float a = lo;
    float sum = 0.0f;
    if (a < -k) {
        sum += (-k - a) * sid_fx_tape_mean_lo(a, -k);
        a = -k;
    }
    float b = hi < k ? hi : k;
    sum += (b - a) * sid_fx_tape_mean_in(a, b);
    if (hi > k) {
        sum += (hi - k) * sid_fx_tape_mean_hi(k, hi);
    }
    return sum / (hi - lo);
}

/* *r tracks 1/x: one Newton step while x has moved by under 1 % since (each squares
 * the relative error, and smoothed amounts move far less per sample), a division on a
 * larger jump. */
static inline void sid_fx_tape_recip(float* r, float x) {
    float e = 1.0f - x * *r;
    if (e < 0.01f && e > -0.01f) {
        *r += *r * e;
    } else {
        *r = 1.0f / x;
    }
}

static inline void sid_fx_tape_set_amount(sid_fx_tape* t, float amount) {
    float a2 = amount * amount;
    t->amount = amount;
    t->drive = 1.0f + SID_FX_TAPE_DRIVE * amount;
    t->boost = SID_FX_TAPE_EMPHASIS * amount;
    t->g = t->boost * (1.0f - t->b);
    t->bias = SID_FX_TAPE_BIAS * amount;
    /* The clip's output and slope at zero input, with the bias below the knee (the
     * cubic): bias - a (4/27) bias^3 and 1 - a (4/9) bias^2. The output is removed so
     * silence stays silent; the makeup divides by drive times slope so small signals keep
     * exactly unity gain. */
    t->bias_out = t->bias -
                  (4.0f / 27.0f * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS) *
                      a2 * a2;
    float slope = 1.0f - (4.0f / 9.0f * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS) * a2 * amount;
    /* One reciprocal serves both gains: q = 1 / (drive slope (1 + g)); the de-emphasis
     * 1 / (1 + g) is then q drive slope. */
    float x = t->drive * slope;
    sid_fx_tape_recip(&t->q, x * (1.0f + t->g));
    t->deemph = t->q * x;
    t->g_de = t->g * t->deemph;
}

static inline void sid_fx_tape_reset(sid_fx_tape* t) {
    t->lp_pre = 0.0f;
    t->lp_de = 0.0f;
    t->dc_x = 0.0f;
    t->dc_y = 0.0f;
    t->u_prev = 0.0f;
    t->q = 1.0f;
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
    float clip = u + amount * sid_fx_tape_residual_adaa(u, t->u_prev);
    t->u_prev = u;

    /* Makeup, then de-emphasis, the exact inverse of the shelf: pre = x (1 + g) - g l[n-1]
     * with g = boost (1 - b), so x = (pre + g l[n-1]) / (1 + g). */
    float y = (clip - t->bias_out) * t->q + t->g_de * t->lp_de;
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
