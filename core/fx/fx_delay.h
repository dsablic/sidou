// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_DELAY_H
#define SIDOU_FX_DELAY_H

/* A delay line over a caller-owned buffer whose length is a power of two, read at
 * fractional delays. The buffer is passed to each call rather than stored, so the state
 * structs that embed both stay plain copyable values.
 *
 * Two buffer types: float, and int16 (half the memory, for long delays; samples are
 * normalised floats at the interface, stored truncated towards zero). Two
 * reads: linear interpolation (cheap; a gentle low-pass at fractional delays) and 4-point
 * 3rd-order Hermite (Catmull-Rom; flat to a much higher frequency, for chorus and
 * flanger sweeps). */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t mask; /* buffer length - 1 */
    uint32_t last; /* index of the most recent sample */
} sid_fx_delay;

/* `len` must be a power of two. Leaves the buffer as it is. */
static inline void sid_fx_delay_init(sid_fx_delay* d, uint32_t len) {
    d->mask = len - 1u;
    d->last = 0;
}

/* Hermite through n (one newer), a, b, o (two older), at frac between a and b. */
static inline float sid_fx_hermite(float n, float a, float b, float o, float frac) {
    float c1 = 0.5f * (b - n);
    float c2 = n - 2.5f * a + 2.0f * b - 0.5f * o;
    float c3 = 0.5f * (o - n) + 1.5f * (a - b);
    return ((c3 * frac + c2) * frac + c1) * frac + a;
}

/* ---- float buffer ---- */

static inline void sid_fx_delay_push(sid_fx_delay* d, float* buf, float x) {
    d->last = (d->last + 1u) & d->mask;
    buf[d->last] = x;
}

/* The sample pushed `delay` samples ago; 0 is the latest. 0 <= delay <= len - 2. */
static inline float sid_fx_delay_read(const sid_fx_delay* d, const float* buf, float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_lerp(buf[p & d->mask], buf[(p - 1u) & d->mask], frac);
}

/* As sid_fx_delay_read with Hermite interpolation. 1 <= delay <= len - 3. */
static inline float sid_fx_delay_read_hermite(const sid_fx_delay* d, const float* buf,
                                              float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_hermite(buf[(p + 1u) & d->mask], buf[p & d->mask], buf[(p - 1u) & d->mask],
                          buf[(p - 2u) & d->mask], frac);
}

/* ---- int16 buffer ---- */

/* Stored with truncation towards zero (clamped to the int16 range), not rounding: in a
 * feedback loop each pass then shrinks small values, so a decaying loop reaches exact
 * silence instead of settling into a limit cycle of a few LSB. */
static inline void sid_fx_delay16_push(sid_fx_delay* d, int16_t* buf, float x) {
    d->last = (d->last + 1u) & d->mask;
    float v = x * 32768.0f;
    if (v > 32767.0f) {
        v = 32767.0f;
    } else if (v < -32768.0f) {
        v = -32768.0f;
    }
    buf[d->last] = (int16_t)(int32_t)v;
}

static inline float sid_fx_delay16_at(const sid_fx_delay* d, const int16_t* buf, uint32_t p) {
    return (float)buf[p & d->mask] * (1.0f / 32768.0f);
}

/* 0 <= delay <= len - 2. */
static inline float sid_fx_delay16_read(const sid_fx_delay* d, const int16_t* buf,
                                        float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_lerp(sid_fx_delay16_at(d, buf, p), sid_fx_delay16_at(d, buf, p - 1u), frac);
}

/* 1 <= delay <= len - 3. */
static inline float sid_fx_delay16_read_hermite(const sid_fx_delay* d, const int16_t* buf,
                                                float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_hermite(sid_fx_delay16_at(d, buf, p + 1u), sid_fx_delay16_at(d, buf, p),
                          sid_fx_delay16_at(d, buf, p - 1u), sid_fx_delay16_at(d, buf, p - 2u),
                          frac);
}

#ifdef __cplusplus
}
#endif

#endif
