// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_DELAY_H
#define SIDOU_FX_DELAY_H

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t mask;
    uint32_t last;
} sid_fx_delay;

static inline void sid_fx_delay_init(sid_fx_delay* d, uint32_t len) {
    d->mask = len - 1u;
    d->last = 0;
}

static inline float sid_fx_hermite(float n, float a, float b, float o, float frac) {
    float c1 = 0.5f * (b - n);
    float c2 = n - 2.5f * a + 2.0f * b - 0.5f * o;
    float c3 = 0.5f * (o - n) + 1.5f * (a - b);
    return ((c3 * frac + c2) * frac + c1) * frac + a;
}

static inline void sid_fx_delay_push(sid_fx_delay* d, float* buf, float x) {
    d->last = (d->last + 1u) & d->mask;
    buf[d->last] = x;
}

static inline float sid_fx_delay_read(const sid_fx_delay* d, const float* buf, float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_lerp(buf[p & d->mask], buf[(p - 1u) & d->mask], frac);
}

static inline float sid_fx_delay_read_hermite(const sid_fx_delay* d, const float* buf,
                                              float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_hermite(buf[(p + 1u) & d->mask], buf[p & d->mask], buf[(p - 1u) & d->mask],
                          buf[(p - 2u) & d->mask], frac);
}

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

static inline float sid_fx_delay16_read(const sid_fx_delay* d, const int16_t* buf,
                                        float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    uint32_t p = d->last - i;
    return sid_fx_lerp(sid_fx_delay16_at(d, buf, p), sid_fx_delay16_at(d, buf, p - 1u), frac);
}

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
