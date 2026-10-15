// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_MATH_H
#define SIDOU_SID_MATH_H

#if defined(__FAST_MATH__)
#error "SIDOU needs host and pi32v2 float results to be bit-identical; build without -ffast-math."
#endif

#define SIDOU_PI 3.14159265358979f

/* tan for 0 <= x < ~1.4 without libm (the firmware has none). Taylor series of sin and
 * cos; error < 1e-6 relative on that range. Pure float ops, so host and target agree. */
static inline float sidou_tanf(float x) {
    float x2 = x * x;
    float s = x * (1.0f - x2 / 6.0f * (1.0f - x2 / 20.0f * (1.0f - x2 / 42.0f *
                                                                    (1.0f - x2 / 72.0f *
                                                                                (1.0f - x2 / 110.0f)))));
    float c = 1.0f - x2 / 2.0f * (1.0f - x2 / 12.0f * (1.0f - x2 / 30.0f *
                                                             (1.0f - x2 / 56.0f *
                                                                         (1.0f - x2 / 90.0f *
                                                                                     (1.0f - x2 / 132.0f)))));
    return s / c;
}

/* tan for 0 <= x <= 0.45 pi (the filter tables' range, sid.c) without a series: its Pade
 * approximant of order [7/6], within 4e-9 relative there in exact arithmetic (float rounding
 * dominates). One divide, for where sidou_tanf's twelve are too dear (sid_update_filter, the
 * age/heat blend after each filter write). */
static inline float sidou_tanf_fast(float x) {
    float x2 = x * x;
    float x4 = x2 * x2;
    float x6 = x4 * x2;
    return x * (135135.0f - 17325.0f * x2 + 378.0f * x4 - x6) /
           (135135.0f - 62370.0f * x2 + 3150.0f * x4 - 28.0f * x6);
}

/* atan for g >= 0, within 3e-9 relative in exact arithmetic: reduced to [0, 2 - sqrt 3]
 * (atan g = pi/2 - atan 1/g past 1, then pi/6 + atan((sqrt3 g - 1) / (sqrt3 + g))), then
 * the Pade approximant of order [5/4]. Three divides at most. */
static inline float sidou_atanf(float g) {
    const float sqrt3 = 1.73205080757f;
    int inverted = g > 1.0f;
    if (inverted) {
        g = 1.0f / g;
    }
    int shifted = g > 2.0f - sqrt3;
    if (shifted) {
        g = (sqrt3 * g - 1.0f) / (sqrt3 + g);
    }
    float g2 = g * g;
    float a = g * (945.0f + 735.0f * g2 + 64.0f * g2 * g2) /
              (945.0f + 1050.0f * g2 + 225.0f * g2 * g2);
    if (shifted) {
        a += SIDOU_PI / 6.0f;
    }
    return inverted ? SIDOU_PI / 2.0f - a : a;
}

/* Cubic soft clip: identity near 0, flat at +-1 beyond +-1.5. */
static inline float sidou_soft_clip(float x) {
    if (x > 1.5f) {
        return 1.0f;
    }
    if (x < -1.5f) {
        return -1.0f;
    }
    return x - (4.0f / 27.0f) * x * x * x;
}

#endif
