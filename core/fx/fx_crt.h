// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CRT_H
#define SIDOU_FX_CRT_H

/* CRT/TV speaker: two cascaded one-pole low-passes y += a (x - y) whose coefficient falls
 * from 1 (open) at amount 0 to a_min at amount 1, plus quiet 50 Hz mains hum with its
 * 2nd and 3rd harmonics, scaled by the amount. a_min puts each pole at SID_FX_CRT_MIN_HZ
 * (backward Euler), so the pair is -3 dB a little below that. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CRT_MIN_HZ 5000.0f
#define SID_FX_HUM_HZ 50.0f
#define SID_FX_HUM_LEVEL 0.003f /* fundamental peak at amount 1, full scale 1 (-50 dBFS) */
#define SID_FX_HUM_H2 0.5f      /* harmonic levels relative to the fundamental */
#define SID_FX_HUM_H3 0.3f

typedef struct {
    float a_min;
    float lp1, lp2;
    uint32_t hum_phase, hum_inc;
} sid_fx_crt;

static inline void sid_fx_crt_reset(sid_fx_crt* c) {
    c->lp1 = 0.0f;
    c->lp2 = 0.0f;
    c->hum_phase = 0;
}

static inline void sid_fx_crt_init(sid_fx_crt* c, uint32_t sample_rate) {
    float w = 2.0f * SIDOU_PI * SID_FX_CRT_MIN_HZ / (float)sample_rate;
    c->a_min = w / (1.0f + w);
    c->hum_inc = sid_fx_phase_inc(SID_FX_HUM_HZ, sample_rate);
    sid_fx_crt_reset(c);
}

/* x is normalised to [-1, 1); amount is in (0, 1]. */
static inline float sid_fx_crt_tick(sid_fx_crt* c, float x, float amount) {
    float a = 1.0f - amount * (1.0f - c->a_min);
    c->lp1 = sid_fx_flush(c->lp1 + a * (x - c->lp1));
    c->lp2 = sid_fx_flush(c->lp2 + a * (c->lp1 - c->lp2));

    /* sin 2t = 2 s c, sin 3t = 3 s - 4 s^3 */
    float s = sid_fx_sin(c->hum_phase);
    float co = sid_fx_cos(c->hum_phase);
    c->hum_phase += c->hum_inc;
    float hum = s + SID_FX_HUM_H2 * 2.0f * s * co + SID_FX_HUM_H3 * (3.0f - 4.0f * s * s) * s;
    return c->lp2 + amount * SID_FX_HUM_LEVEL * hum;
}

#ifdef __cplusplus
}
#endif

#endif
