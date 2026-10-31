// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_FLANGER_H
#define SIDOU_FX_FLANGER_H

#include <stdint.h>

#include "fx_chorus.h"
#include "fx_delay.h"
#include "fx_util.h"
#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_FLANGER_LEN 512u
#define SID_FX_FLANGER_MIN_MS 0.1f
#define SID_FX_FLANGER_MAX_MS 10.0f
#define SID_FX_FLANGER_MIN_HZ 0.05f
#define SID_FX_FLANGER_RATE_OCTAVES 6.643856f
#define SID_FX_FLANGER_FEEDBACK 0.9f

typedef struct {
    float buf[SID_FX_FLANGER_LEN];
    sid_fx_delay line;
    uint32_t phase, inc;
    float min_delay, range, hz_inc;
    float depth, depth_coef;
    float delay_l, delay_r;
    float step_l, step_r;
    uint32_t left;
} sid_fx_flanger;

static inline void sid_fx_flanger_set_rate(sid_fx_flanger* f, float rate) {
    float hz = SID_FX_FLANGER_MIN_HZ * sid_fx_exp2(SID_FX_FLANGER_RATE_OCTAVES * rate);
    f->inc = (uint32_t)(int32_t)(hz * f->hz_inc) << 1;
}

static inline void sid_fx_flanger_setup(sid_fx_flanger* f, uint32_t sample_rate) {
    float fs = (float)sample_rate;
    sid_fx_delay_init(&f->line, SID_FX_FLANGER_LEN);
    f->phase = 0;
    f->hz_inc = 2147483648.0f / fs;
    f->min_delay = SID_FX_FLANGER_MIN_MS * 0.001f * fs;
    float max_delay = SID_FX_FLANGER_MAX_MS * 0.001f * fs;
    if (max_delay > (float)(SID_FX_FLANGER_LEN - 4u)) {
        max_delay = (float)(SID_FX_FLANGER_LEN - 4u);
    }
    if (f->min_delay < 1.0f) {
        f->min_delay = 1.0f;
    }
    f->range = max_delay - f->min_delay;
    f->delay_l = f->min_delay;
    f->delay_r = f->min_delay;
    f->step_l = 0.0f;
    f->step_r = 0.0f;
    f->depth = 0.0f;
    float k = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    float keep = 1.0f;
    for (uint32_t i = 0; i < SID_FX_MOD_SEGMENT; ++i) {
        keep *= 1.0f - k;
    }
    f->depth_coef = 1.0f - keep;
    f->left = 0;
    sid_fx_flanger_set_rate(f, 0.5f);
}

static inline void sid_fx_flanger_init(sid_fx_flanger* f, uint32_t sample_rate) {
    for (uint32_t i = 0; i < SID_FX_FLANGER_LEN; ++i) {
        f->buf[i] = 0.0f;
    }
    sid_fx_flanger_setup(f, sample_rate);
}

static inline void sid_fx_flanger_tick(sid_fx_flanger* f, float x, float depth, float rate,
                                       float feedback, float* l, float* r) {
    if (f->left == 0) {
        sid_fx_flanger_set_rate(f, rate);
        f->phase += f->inc * SID_FX_MOD_SEGMENT;
        f->depth += f->depth_coef * (depth - f->depth);
        float sweep = f->depth * f->range * 0.5f;
        float centre = f->min_delay + sweep;
        const float seg = 1.0f / (float)SID_FX_MOD_SEGMENT;
        f->step_l = (centre - sweep * sid_fx_cos(f->phase) - f->delay_l) * seg;
        f->step_r = (centre + sweep * sid_fx_sin(f->phase) - f->delay_r) * seg;
        f->left = SID_FX_MOD_SEGMENT;
    }
    --f->left;
    f->delay_l += f->step_l;
    f->delay_r += f->step_r;
    float tl = sid_fx_delay_read_hermite(&f->line, f->buf, f->delay_l);
    float tr = sid_fx_delay_read_hermite(&f->line, f->buf, f->delay_r);
    float fb = SID_FX_FLANGER_FEEDBACK * (2.0f * feedback - 1.0f);
    sid_fx_delay_push(&f->line, f->buf, sid_fx_flush(x + fb * sidou_soft_clip(tl)));
    *l = 0.5f * (x + tl);
    *r = 0.5f * (x + tr);
}

#ifdef __cplusplus
}
#endif

#endif
