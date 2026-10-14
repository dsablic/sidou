// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_DELAY_H
#define SIDOU_FX_DELAY_H

/* A delay line over a caller-owned float buffer whose length is a power of two, read at
 * fractional delays with linear interpolation. The buffer is passed to each call rather
 * than stored, so the state structs that embed both stay plain copyable values. */

#include <stdint.h>

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

static inline void sid_fx_delay_push(sid_fx_delay* d, float* buf, float x) {
    d->last = (d->last + 1u) & d->mask;
    buf[d->last] = x;
}

/* The sample pushed `delay` samples ago; 0 is the latest. 0 <= delay <= len - 2. */
static inline float sid_fx_delay_read(const sid_fx_delay* d, const float* buf, float delay) {
    uint32_t i = (uint32_t)(int32_t)delay;
    float frac = delay - (float)(int32_t)i;
    float a = buf[(d->last - i) & d->mask];
    float b = buf[(d->last - i - 1u) & d->mask];
    return a + frac * (b - a);
}

#ifdef __cplusplus
}
#endif

#endif
