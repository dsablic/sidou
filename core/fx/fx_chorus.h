// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CHORUS_H
#define SIDOU_FX_CHORUS_H

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CHORUS_LEN 1024u
#define SID_FX_CHORUS_CENTRE_MS 7.0f
#define SID_FX_CHORUS_SLOW_HZ 0.6f
#define SID_FX_CHORUS_FAST_HZ 6.0f
#define SID_FX_CHORUS_SLOW_MS 3.0f
#define SID_FX_CHORUS_FAST_MS 0.4f
#define SID_FX_MOD_SEGMENT 16u

typedef struct {
    float buf[SID_FX_CHORUS_LEN];
    sid_fx_delay line;
    uint32_t slow_phase, fast_phase, slow_inc, fast_inc;
    float slow_hz_inc, fast_hz_inc;
    float centre, slow_depth, fast_depth;
    float depth, depth_coef;
    float delay[6];
    float step[6];
    uint32_t left;
} sid_fx_chorus;

static inline void sid_fx_chorus_set_rate(sid_fx_chorus* c, float rate) {
    float mult = 0.25f * sid_fx_exp2(3.0f * rate);
    c->slow_inc = (uint32_t)(int32_t)(c->slow_hz_inc * mult) << 1;
    c->fast_inc = (uint32_t)(int32_t)(c->fast_hz_inc * mult) << 1;
}

static inline void sid_fx_chorus_plan(sid_fx_chorus* c, float depth, float rate, float ensemble) {
    sid_fx_chorus_set_rate(c, rate);
    c->slow_phase += c->slow_inc * SID_FX_MOD_SEGMENT;
    c->fast_phase += c->fast_inc * SID_FX_MOD_SEGMENT;
    c->depth += c->depth_coef * (depth - c->depth);
    const float h = 0.8660254f;
    float ss = sid_fx_sin(c->slow_phase);
    float sc = sid_fx_cos(c->slow_phase);
    float fs_ = sid_fx_sin(c->fast_phase);
    float fc = sid_fx_cos(c->fast_phase);
    float ds = c->depth * c->slow_depth;
    float df = c->depth * ensemble * c->fast_depth;
    float s1 = 0.5f * ss + h * sc, f1 = 0.5f * fs_ + h * fc;
    float s2 = -0.5f * ss + h * sc, f2 = -0.5f * fs_ + h * fc;
    float to[6] = {c->centre + ds * ss + df * fs_, c->centre + ds * s2 + df * f2,
                   c->centre - ds * s1 - df * f1,  c->centre + ds * s1 + df * f1,
                   c->centre - ds * ss - df * fs_, c->centre - ds * s2 - df * f2};
    for (int i = 0; i < 6; ++i) {
        c->step[i] = (to[i] - c->delay[i]) * (1.0f / (float)SID_FX_MOD_SEGMENT);
    }
    c->left = SID_FX_MOD_SEGMENT;
}

static inline void sid_fx_chorus_setup(sid_fx_chorus* c, uint32_t sample_rate) {
    float fs = (float)sample_rate;
    sid_fx_delay_init(&c->line, SID_FX_CHORUS_LEN);
    c->slow_phase = 0;
    c->fast_phase = 0x2aaaaaabu;
    c->slow_hz_inc = SID_FX_CHORUS_SLOW_HZ / fs * 2147483648.0f;
    c->fast_hz_inc = SID_FX_CHORUS_FAST_HZ / fs * 2147483648.0f;
    c->centre = SID_FX_CHORUS_CENTRE_MS * 0.001f * fs;
    c->slow_depth = SID_FX_CHORUS_SLOW_MS * 0.001f * fs;
    c->fast_depth = SID_FX_CHORUS_FAST_MS * 0.001f * fs;
    for (int i = 0; i < 6; ++i) {
        c->delay[i] = c->centre;
        c->step[i] = 0.0f;
    }
    c->depth = 0.0f;
    float k = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    float keep = 1.0f;
    for (uint32_t i = 0; i < SID_FX_MOD_SEGMENT; ++i) {
        keep *= 1.0f - k;
    }
    c->depth_coef = 1.0f - keep;
    c->left = 0;
    sid_fx_chorus_set_rate(c, 2.0f / 3.0f);
}

static inline void sid_fx_chorus_init(sid_fx_chorus* c, uint32_t sample_rate) {
    for (uint32_t i = 0; i < SID_FX_CHORUS_LEN; ++i) {
        c->buf[i] = 0.0f;
    }
    sid_fx_chorus_setup(c, sample_rate);
}

static inline void sid_fx_chorus_tick(sid_fx_chorus* c, float x, float depth, float rate,
                                      float ensemble, float* l, float* r) {
    if (c->left == 0) {
        sid_fx_chorus_plan(c, depth, rate, ensemble);
    }
    --c->left;
    sid_fx_delay_push(&c->line, c->buf, x);
    for (int i = 0; i < 6; ++i) {
        c->delay[i] += c->step[i];
    }
    const float third = 1.0f / 3.0f;
    *l = third * (sid_fx_delay_read(&c->line, c->buf, c->delay[0]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[1]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[2]));
    *r = third * (sid_fx_delay_read(&c->line, c->buf, c->delay[3]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[4]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[5]));
}

#ifdef __cplusplus
}
#endif

#endif
