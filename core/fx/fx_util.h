// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_UTIL_H
#define SIDOU_FX_UTIL_H

/* Building blocks shared by the effects: parameter smoothing, LFOs, a deterministic
 * random source and a few float helpers. No libm (the firmware has none) and only plain
 * float and integer operations, so host and pi32v2 compute the same bits. */

#include <stdint.h>

#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Parameter smoothing time constant. */
#define SID_FX_SMOOTH_MS 10.0f
/* A smoothed value this close to its target snaps onto it, so it settles exactly. Above
 * the gap where the one-pole step rounds to nothing near 1 (ulp / 2 / coef, 6e-5 at
 * 96 kHz), or it would stall short of the target. */
#define SID_FX_SMOOTH_SNAP 1e-4f

/* One-pole coefficient for a time constant of `ms` at `sample_rate`. */
static inline float sid_fx_smooth_coef(float ms, uint32_t sample_rate) {
    return 1000.0f / (ms * (float)sample_rate);
}

/* One step of a one-pole smoother from *cur towards target, snapping onto the target
 * once within SID_FX_SMOOTH_SNAP. */
static inline void sid_fx_smooth(float* cur, float target, float coef) {
    float d = target - *cur;
    *cur = (d < SID_FX_SMOOTH_SNAP && d > -SID_FX_SMOOTH_SNAP) ? target : *cur + coef * d;
}

/* The same in two parts, for many parameters at once: a step without the snap every
 * sample, and the snap every few samples (on a fixed sample grid, so the result does not
 * depend on how the caller splits its blocks). */
static inline void sid_fx_smooth_step(float* cur, float target, float coef) {
    *cur += coef * (target - *cur);
}

static inline void sid_fx_smooth_snap(float* cur, float target) {
    float d = target - *cur;
    if (d < SID_FX_SMOOTH_SNAP && d > -SID_FX_SMOOTH_SNAP) {
        *cur = target;
    }
}

/* Clamps a parameter to [0, 1]; NaN gives 0. */
static inline float sid_fx_unit(float v) {
    if (!(v > 0.0f)) {
        return 0.0f;
    }
    return v > 1.0f ? 1.0f : v;
}

/* Adding and removing 1e-18 rounds x to a multiple of ulp(1e-18) = 2^-83 (~1e-25) when
 * |x| is below about 1e-18, so anything under ~5e-26 becomes exactly 0 and recursive
 * filters never decay into denormals (slow, and possibly flushed on the target but not
 * on the host). Audio-range values (|x| > 1e-7) pass unchanged. */
static inline float sid_fx_flush(float x) {
    return (x + 1e-18f) - 1e-18f;
}

static inline float sid_fx_lerp(float a, float b, float frac) {
    return a + frac * (b - a);
}

static inline float sid_fx_abs(float x) {
    return x < 0.0f ? -x : x;
}

/* p^2 (3 - 2p) for p in [0, 1]: a crossfade curve with zero slope at both ends, so a
 * fade has no corner in its envelope. */
static inline float sid_fx_smoothstep(float p) {
    return p * p * (3.0f - 2.0f * p);
}

/* *r tracks 1/x: one Newton step while x has moved by under 1 % since (each squares
 * the relative error, and smoothed values move far less per sample), a division on a
 * larger jump. */
static inline void sid_fx_recip_track(float* r, float x) {
    float e = 1.0f - x * *r;
    if (e < 0.01f && e > -0.01f) {
        *r += *r * e;
    } else {
        *r = 1.0f / x;
    }
}

/* Phase increment of an LFO at `hz`: a full turn is 2^32. */
static inline uint32_t sid_fx_phase_inc(float hz, uint32_t sample_rate) {
    return (uint32_t)(int32_t)(hz / (float)sample_rate * 2147483648.0f) << 1;
}

/* sin(2 pi phase / 2^32), within 0.1 % of full scale. A parabola refined with one more
 * term; the top 24 phase bits convert to float exactly. */
static inline float sid_fx_sin(uint32_t phase) {
    float t = (float)((int32_t)phase >> 8) * (1.0f / 8388608.0f); /* [-1, 1): angle pi t */
    float y = 4.0f * t * (1.0f - sid_fx_abs(t));
    return y + 0.225f * (y * sid_fx_abs(y) - y);
}

static inline float sid_fx_cos(uint32_t phase) {
    return sid_fx_sin(phase + 0x40000000u);
}

/* xorshift32. The seed is never 0. */
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

/* Uniform in [-1, 1). */
static inline float sid_fx_rng_bipolar(uint32_t* s) {
    return (float)((int32_t)sid_fx_rng_next(s) >> 8) * (1.0f / 8388608.0f);
}

/* Uniform in [0, 1). */
static inline float sid_fx_rng_unit(uint32_t* s) {
    return (float)(sid_fx_rng_next(s) >> 8) * (1.0f / 16777216.0f);
}

/* 2^e for 0 <= e <= 30, continuous in e: exact at integers, within 0.3 % between. */
static inline float sid_fx_exp2(float e) {
    int32_t i = (int32_t)e;
    float f = e - (float)i;
    return (float)(1u << i) * (1.0f + f * (0.6565f + 0.3435f * f));
}

/* e^x for |x| <= 8, to about 1e-7 relative: for init-time tables, not the audio path.
 * x / 16 by Taylor series, then squared four times. */
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

/* Normalised float [-1, 1) to int16 with the convention of sid_render: round half away
 * from zero, then clamp. */
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
