// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_UTIL_H
#define SIDOU_FX_UTIL_H

#include <stdint.h>

#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_SMOOTH_MS 10.0f
#define SID_FX_SMOOTH_SNAP 1e-4f

static inline float sid_fx_smooth_coef(float ms, uint32_t sample_rate) {
    return 1000.0f / (ms * (float)sample_rate);
}

static inline void sid_fx_smooth(float* cur, float target, float coef) {
    float d = target - *cur;
    *cur = (d < SID_FX_SMOOTH_SNAP && d > -SID_FX_SMOOTH_SNAP) ? target : *cur + coef * d;
}

static inline void sid_fx_smooth_step(float* cur, float target, float coef) {
    *cur += coef * (target - *cur);
}

static inline void sid_fx_smooth_snap(float* cur, float target) {
    float d = target - *cur;
    if (d < SID_FX_SMOOTH_SNAP && d > -SID_FX_SMOOTH_SNAP) {
        *cur = target;
    }
}

static inline float sid_fx_unit(float v) {
    if (!(v > 0.0f)) {
        return 0.0f;
    }
    return v > 1.0f ? 1.0f : v;
}

static inline float sid_fx_flush(float x) {
    return (x + 1e-18f) - 1e-18f;
}

static inline float sid_fx_slew(float cur, float target, float step) {
    float d = target - cur;
    if (d > step) {
        return cur + step;
    }
    if (d < -step) {
        return cur - step;
    }
    return target;
}

static inline float sid_fx_lerp(float a, float b, float frac) {
    return a + frac * (b - a);
}

static inline float sid_fx_abs(float x) {
    return x < 0.0f ? -x : x;
}

static inline float sid_fx_smoothstep(float p) {
    return p * p * (3.0f - 2.0f * p);
}

static inline void sid_fx_recip_track(float* r, float x) {
    float e = 1.0f - x * *r;
    if (e < 0.01f && e > -0.01f) {
        *r += *r * e;
    } else {
        *r = 1.0f / x;
    }
}

static inline uint32_t sid_fx_phase_inc(float hz, uint32_t sample_rate) {
    return (uint32_t)(int32_t)(hz / (float)sample_rate * 2147483648.0f) << 1;
}

static inline float sid_fx_sin(uint32_t phase) {
    float t = (float)((int32_t)phase >> 8) * (1.0f / 8388608.0f);
    float y = 4.0f * t * (1.0f - sid_fx_abs(t));
    return y + 0.225f * (y * sid_fx_abs(y) - y);
}

static inline float sid_fx_cos(uint32_t phase) {
    return sid_fx_sin(phase + 0x40000000u);
}

static inline uint32_t sid_fx_rng_seed(uint32_t seed) {
    seed ^= 0x9e3779b9u;
    return seed ? seed : 1u;
}

static inline uint32_t sid_fx_rng_next(uint32_t* s) {
    uint32_t x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    return x;
}

static inline float sid_fx_rng_bipolar(uint32_t* s) {
    return (float)((int32_t)sid_fx_rng_next(s) >> 8) * (1.0f / 8388608.0f);
}

static inline float sid_fx_rng_unit(uint32_t* s) {
    return (float)(sid_fx_rng_next(s) >> 8) * (1.0f / 16777216.0f);
}

static inline float sid_fx_exp2(float e) {
    int32_t i = (int32_t)e;
    float f = e - (float)i;
    return (float)(1u << i) * (1.0f + f * (0.6565f + 0.3435f * f));
}

static inline float sid_fx_exp_init(float x) {
    float y = x * (1.0f / 16.0f);
    float term = 1.0f;
    float sum = 1.0f;
    for (int k = 1; k <= 8; ++k) {
        term *= y / (float)k;
        sum += term;
    }
    for (int k = 0; k < 4; ++k) {
        sum *= sum;
    }
    return sum;
}

static inline int16_t sid_fx_to_i16(float x) {
    float o = x * 32768.0f;
    o += o >= 0.0f ? 0.5f : -0.5f;
    if (o > 32767.0f) {
        o = 32767.0f;
    } else if (o < -32768.0f) {
        o = -32768.0f;
    }
    return (int16_t)o;
}

#ifdef __cplusplus
}
#endif

#endif
