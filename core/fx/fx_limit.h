// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_LIMIT_H
#define SIDOU_FX_LIMIT_H

/* The output's soft limiter (docs/decisions/0004-effect-algorithms.md, "The output's soft
 * limiter"). A level in LSBs passes unchanged up to SID_FX_LIMIT_KNEE (-2.5 dBFS) either way;
 * past it, m = |x| becomes C - H^2 / (m - K + H), H = C - K: slope 1 at the knee (no corner),
 * rising ever slower towards SID_FX_LIMIT_CEILING, one inside the rails, which it never
 * reaches (an infinity gives the ceiling). It runs once, where the effect slots convert their
 * output to int16 (fx_slots.h; the two chips' sum reaches them whole, app/chip_mix.h): a soft
 * knee is not idempotent, so nothing before it limits. No setting puts a sample on a rail, and a
 * signal that never passes the knee is the same bit for bit.
 *
 * Below the knee a sample costs the two compares the clamp it replaces cost; past it, a call
 * and a divide (out of line: one copy in SRAM). No libm; host and pi32v2 compute the same
 * bits. */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_LIMIT_KNEE 24576
#define SID_FX_LIMIT_CEILING 32766

/* A level past the knee (|x| > SID_FX_LIMIT_KNEE) through the limiter, rounded half away from
 * zero as sid_render rounds. */
int16_t sid_fx_limit_round(float x);

/* An integer level through the limiter. */
static inline int16_t sid_fx_limit_i32(int32_t v) {
    if (v > SID_FX_LIMIT_KNEE || v < -SID_FX_LIMIT_KNEE) {
        return sid_fx_limit_round((float)v);
    }
    return (int16_t)v;
}

/* Normalised float to int16 as fx_util.h's sid_fx_to_i16 (round half away from zero), through
 * the limiter instead of the clamp: equal to it wherever the level stays within the knee. */
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
