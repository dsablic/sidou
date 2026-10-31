// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_LIMIT_H
#define SIDOU_FX_LIMIT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_LIMIT_KNEE 24576
#define SID_FX_LIMIT_CEILING 32766

int16_t sid_fx_limit_round(float x);

static inline int16_t sid_fx_limit_i32(int32_t v) {
    if (v > SID_FX_LIMIT_KNEE || v < -SID_FX_LIMIT_KNEE) {
        return sid_fx_limit_round((float)v);
    }
    return (int16_t)v;
}

static inline int16_t sid_fx_to_i16_limited(float x) {
    float o = x * 32768.0f;
    if (o > (float)SID_FX_LIMIT_KNEE || o < -(float)SID_FX_LIMIT_KNEE) {
        return sid_fx_limit_round(o);
    }
    o += o >= 0.0f ? 0.5f : -0.5f;
    return (int16_t)o;
}

#ifdef __cplusplus
}
#endif

#endif
