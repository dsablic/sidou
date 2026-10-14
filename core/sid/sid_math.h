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
