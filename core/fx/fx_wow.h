// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_WOW_H
#define SIDOU_FX_WOW_H

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_WOW_LEN 1024u
#define SID_FX_WOW_HZ 0.5f
#define SID_FX_WOW_CENTS 20.0f
#define SID_FX_WOW_DRIFT_MS 3.0f
#define SID_FX_WOW_DRIFT_HZ 0.5f
#define SID_FX_FLUTTER_HZ 8.0f
#define SID_FX_FLUTTER_CENTS 8.0f
#define SID_FX_WOW_FADE_MS 10.0f
#define SID_FX_WOW_FADE_TURN 32.0f
#define SID_FX_WOW_DEPTH_S 1.0f
#define SID_FX_FLUTTER_DEPTH_MS 50.0f

typedef struct {
    float buf[SID_FX_WOW_LEN];
    sid_fx_delay line;
    uint32_t rng;
    uint32_t wow_phase, wow_inc;
    uint32_t flutter_phase, flutter_inc;
    uint32_t drift_count, drift_period;
    uint32_t filled, centre_samples;
    float wow_depth, drift_depth, flutter_depth;
    float centre;
    float drift_coef, drift_target, drift1, drift2;
    float wow_slew, flutter_slew, fade_step, fade_accel;
    float wow, flutter;
    float fade;
    float fade_v;
    float delay;
} sid_fx_wow;

static inline void sid_fx_wow_reset(sid_fx_wow* w) {
    w->wow_phase = 0;
    w->flutter_phase = 0;
    w->drift1 = 0.0f;
    w->drift2 = 0.0f;
    w->drift_count = w->drift_period;
    w->drift_target = sid_fx_rng_bipolar(&w->rng);
    w->wow = 0.0f;
    w->flutter = 0.0f;
    w->fade = 0.0f;
    w->fade_v = 0.0f;
    w->filled = 0;
    w->delay = w->centre;
}

static inline float sid_fx_wow_depth(float cents, float hz, uint32_t sample_rate) {
    float x = cents * (0.69314718f / 1200.0f);
    float ratio = x * (1.0f + 0.5f * x);
    return ratio * (float)sample_rate / (2.0f * SIDOU_PI * hz);
}

static inline void sid_fx_wow_setup(sid_fx_wow* w, uint32_t sample_rate, uint32_t seed) {
    sid_fx_delay_init(&w->line, SID_FX_WOW_LEN);
    float fs = (float)sample_rate;
    w->rng = sid_fx_rng_seed(seed);
    w->wow_inc = sid_fx_phase_inc(SID_FX_WOW_HZ, sample_rate);
    w->flutter_inc = sid_fx_phase_inc(SID_FX_FLUTTER_HZ, sample_rate);
    w->drift_period = sample_rate / 4u;
    w->drift_coef = 2.0f * SIDOU_PI * SID_FX_WOW_DRIFT_HZ / fs;
    w->wow_depth = sid_fx_wow_depth(SID_FX_WOW_CENTS, SID_FX_WOW_HZ, sample_rate);
    w->drift_depth = SID_FX_WOW_DRIFT_MS * 0.001f * fs;
    w->flutter_depth = sid_fx_wow_depth(SID_FX_FLUTTER_CENTS, SID_FX_FLUTTER_HZ, sample_rate);
    float swing = w->wow_depth + w->drift_depth + w->flutter_depth;
    float room = 0.5f * (float)(SID_FX_WOW_LEN - 4u);
    if (swing > room) {
        float scale = room / swing;
        w->wow_depth *= scale;
        w->drift_depth *= scale;
        w->flutter_depth *= scale;
        swing = room;
    }
    w->centre_samples = (uint32_t)(int32_t)swing + 2u;
    w->centre = (float)(int32_t)w->centre_samples;
    w->wow_slew = 1.0f / (SID_FX_WOW_DEPTH_S * fs);
    w->flutter_slew = 1000.0f / (SID_FX_FLUTTER_DEPTH_MS * fs);
    w->fade_step = 1000.0f / (SID_FX_WOW_FADE_MS * fs);
    w->fade_accel = w->fade_step / SID_FX_WOW_FADE_TURN;
    sid_fx_wow_reset(w);
}

static inline void sid_fx_wow_init(sid_fx_wow* w, uint32_t sample_rate, uint32_t seed) {
    for (uint32_t i = 0; i < SID_FX_WOW_LEN; ++i) {
        w->buf[i] = 0.0f;
    }
    sid_fx_wow_setup(w, sample_rate, seed);
}

static inline int sid_fx_wow_busy(const sid_fx_wow* w) {
    return w->fade != 0.0f;
}

static inline float sid_fx_wow_tick(sid_fx_wow* w, float x, float wow, float flutter) {
    sid_fx_delay_push(&w->line, w->buf, x);
    if (w->filled < w->centre_samples) {
        ++w->filled;
    }
    w->wow = sid_fx_slew(w->wow, wow, w->wow_slew);
    w->flutter = sid_fx_slew(w->flutter, flutter, w->flutter_slew);

    float slow = sid_fx_sin(w->wow_phase);
    float fast = sid_fx_sin(w->flutter_phase);
    w->wow_phase += w->wow_inc;
    w->flutter_phase += w->flutter_inc;
    if (--w->drift_count == 0) {
        w->drift_count = w->drift_period;
        w->drift_target = sid_fx_rng_bipolar(&w->rng);
    }
    w->drift1 += w->drift_coef * (w->drift_target - w->drift1);
    w->drift2 += w->drift_coef * (w->drift1 - w->drift2);

    int on = wow > 0.0f || flutter > 0.0f;
    if (!(on && w->fade == 1.0f)) {
        float v = on && w->filled >= w->centre_samples ? w->fade_step : -w->fade_step;
        w->fade_v = sid_fx_slew(w->fade_v, v, w->fade_accel);
        w->fade += w->fade_v;
        if (w->fade >= 1.0f) {
            w->fade = 1.0f;
            w->fade_v = 0.0f;
        } else if (w->fade <= 0.0f) {
            w->fade = 0.0f;
            w->fade_v = 0.0f;
        }
    }

    w->delay = w->centre + w->wow * (w->wow_depth * slow + w->drift_depth * w->drift2) +
               w->flutter * (w->flutter_depth * fast);
    float wet = sid_fx_delay_read(&w->line, w->buf, w->delay);
    if (w->fade == 1.0f) {
        return wet;
    }
    return x + sid_fx_smoothstep(w->fade) * (wet - x);
}

#ifdef __cplusplus
}
#endif

#endif
