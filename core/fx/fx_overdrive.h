// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_OVERDRIVE_H
#define SIDOU_FX_OVERDRIVE_H

/* Overdrive: a mid-hump pre-filter (x + hump * high-pass at ~700 Hz), gain, the tape's soft
 * clip through ADAA of the whole clip (fx_adaa.h: the residual form breaks down at this
 * much gain), a post tone low-pass and a DC blocker.
 *
 * - Amount: the drive, after the gain law of Plaits' Overdrive (Emilie Gillet, MIT):
 *   1 + 0.5 a + (24 a^5 - 0.5 a) a^2, so 1 at a = 0 rising to 25 (+28 dB) at a = 1, with
 *   most of the rise in the top half. The hump grows with it (0 to +8 dB at 1.4 kHz).
 *   Makeup keeps a reference level of 0.25 full scale at about the same loudness.
 * - A: tone, a one-pole low-pass at TONE_MIN_HZ (A = 0, dark) to TONE_MAX_HZ (A = 1),
 *   log-mapped and table-driven like the CRT.
 * - B: bias into the clip (up to 0.4), for even harmonics; its static output is removed
 *   and the DC blocker takes what rectification leaves.
 * Any amounts in [0, 1] may arrive on any sample. */

#include <stdint.h>

#include "fx_adaa.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_OD_HUMP_HZ 700.0f
#define SID_FX_OD_HUMP 1.5f
#define SID_FX_OD_REF 0.25f
#define SID_FX_OD_BIAS 0.4f
#define SID_FX_OD_TONE_MIN_HZ 1500.0f
#define SID_FX_OD_TONE_LN_RANGE 2.4849066f /* ln(18000 / 1500) */
#define SID_FX_OD_TONE_STEPS 16u
#define SID_FX_OD_DC_HZ 10.0f

typedef struct {
    float hump_b, dc_r;
    float tone_table[SID_FX_OD_TONE_STEPS + 1u];
    float hump_lp, tone_lp, u_prev, dc_x, dc_y;
    /* Derived from the amounts. */
    float amount, drive, hump, makeup;
    float bias_amount, bias, bias_out;
    float tone, tone_a;
} sid_fx_overdrive;

static inline float sid_fx_od_clip(float u) {
    return u + (u > 1.5f ? 1.0f - u : u < -1.5f ? -1.0f - u : (-4.0f / 27.0f) * u * u * u);
}

static inline void sid_fx_overdrive_set_amount(sid_fx_overdrive* o, float a) {
    float a2 = a * a;
    o->amount = a;
    o->drive = 1.0f + 0.5f * a + (24.0f * a2 * a2 * a - 0.5f * a) * a2;
    o->hump = SID_FX_OD_HUMP * a;
    /* makeup = REF / clip(REF drive) */
    sid_fx_recip_track(&o->makeup, sid_fx_od_clip(SID_FX_OD_REF * o->drive) / SID_FX_OD_REF);
}

static inline void sid_fx_overdrive_set_bias(sid_fx_overdrive* o, float b) {
    o->bias_amount = b;
    o->bias = SID_FX_OD_BIAS * b;
    o->bias_out = sid_fx_od_clip(o->bias);
}

static inline void sid_fx_overdrive_set_tone(sid_fx_overdrive* o, float t) {
    o->tone = t;
    float pos = t * (float)SID_FX_OD_TONE_STEPS;
    int32_t i = (int32_t)pos;
    if (i >= (int32_t)SID_FX_OD_TONE_STEPS) {
        i = (int32_t)SID_FX_OD_TONE_STEPS - 1;
    }
    o->tone_a = sid_fx_lerp(o->tone_table[i], o->tone_table[i + 1], pos - (float)i);
}

static inline void sid_fx_overdrive_reset(sid_fx_overdrive* o) {
    o->hump_lp = 0.0f;
    o->tone_lp = 0.0f;
    o->u_prev = 0.0f;
    o->dc_x = 0.0f;
    o->dc_y = 0.0f;
    o->makeup = 1.0f;
    sid_fx_overdrive_set_amount(o, 0.0f);
    sid_fx_overdrive_set_bias(o, 0.0f);
    sid_fx_overdrive_set_tone(o, 1.0f);
}

static inline void sid_fx_overdrive_init(sid_fx_overdrive* o, uint32_t sample_rate) {
    float w_scale = 2.0f * SIDOU_PI / (float)sample_rate;
    o->hump_b = 1.0f - sid_fx_exp_init(-w_scale * SID_FX_OD_HUMP_HZ);
    o->dc_r = sid_fx_exp_init(-w_scale * SID_FX_OD_DC_HZ);
    float limit = 0.45f * (float)sample_rate;
    for (uint32_t i = 0; i <= SID_FX_OD_TONE_STEPS; ++i) {
        float t = (float)i / (float)SID_FX_OD_TONE_STEPS;
        float hz = SID_FX_OD_TONE_MIN_HZ * sid_fx_exp_init(SID_FX_OD_TONE_LN_RANGE * t);
        o->tone_table[i] = 1.0f - sid_fx_exp_init(-w_scale * (hz < limit ? hz : limit));
    }
    sid_fx_overdrive_reset(o);
}

/* x is normalised to [-1, 1); amount (drive), tone and bias are in [0, 1]. */
static inline float sid_fx_overdrive_tick(sid_fx_overdrive* o, float x, float amount, float tone,
                                          float bias) {
    if (amount != o->amount) {
        sid_fx_overdrive_set_amount(o, amount);
    }
    if (bias != o->bias_amount) {
        sid_fx_overdrive_set_bias(o, bias);
    }
    if (tone != o->tone) {
        sid_fx_overdrive_set_tone(o, tone);
    }
    float lp = o->hump_lp + o->hump_b * (x - o->hump_lp);
    o->hump_lp = sid_fx_flush(lp);
    float u = o->drive * (x + o->hump * (x - lp)) + o->bias;
    float clip = sid_fx_adaa_soft_clip(u, o->u_prev);
    o->u_prev = u;
    float y = (clip - o->bias_out) * o->makeup;
    o->tone_lp = sid_fx_flush(o->tone_lp + o->tone_a * (y - o->tone_lp));
    o->dc_y = sid_fx_flush(o->tone_lp - o->dc_x + o->dc_r * o->dc_y);
    o->dc_x = o->tone_lp;
    return o->dc_y;
}

#ifdef __cplusplus
}
#endif

#endif
