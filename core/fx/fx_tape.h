// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_TAPE_H
#define SIDOU_FX_TAPE_H

#include <stdint.h>

#include "fx_adaa.h"
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
    float b;
    float dc_r;
    float lp_pre;
    float lp_de;
    float dc_x, dc_y;
    float u_prev;
    float amount, drive, boost, g, bias, bias_out;
    float q;
    float deemph;
    float g_de;
} sid_fx_tape;

static inline void sid_fx_tape_set_amount(sid_fx_tape* t, float amount) {
    float a2 = amount * amount;
    t->amount = amount;
    t->drive = 1.0f + SID_FX_TAPE_DRIVE * amount;
    t->boost = SID_FX_TAPE_EMPHASIS * amount;
    t->g = t->boost * (1.0f - t->b);
    t->bias = SID_FX_TAPE_BIAS * amount;
    t->bias_out = t->bias -
                  (4.0f / 27.0f * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS) *
                      a2 * a2;
    float slope = 1.0f - (4.0f / 9.0f * SID_FX_TAPE_BIAS * SID_FX_TAPE_BIAS) * a2 * amount;
    float x = t->drive * slope;
    sid_fx_recip_track(&t->q, x * (1.0f + t->g));
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

static inline float sid_fx_tape_tick(sid_fx_tape* t, float x, float amount) {
    if (amount != t->amount) {
        sid_fx_tape_set_amount(t, amount);
    }

    float lp = t->lp_pre + t->b * (x - t->lp_pre);
    float pre = x + t->boost * (x - lp);
    t->lp_pre = sid_fx_flush(lp);

    float u = t->drive * pre + t->bias;
    float clip = u + amount * sid_fx_adaa_soft(u, t->u_prev);
    t->u_prev = u;

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
