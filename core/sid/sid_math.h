// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_MATH_H
#define SIDOU_SID_MATH_H

#if defined(__FAST_MATH__)
#error "SIDOU needs host and pi32v2 float results to be bit-identical; build without -ffast-math."
#endif

#define SIDOU_PI 3.14159265358979f

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

static inline float sidou_tanf_fast(float x) {
    float x2 = x * x;
    float x4 = x2 * x2;
    float x6 = x4 * x2;
    return x * (135135.0f - 17325.0f * x2 + 378.0f * x4 - x6) /
           (135135.0f - 62370.0f * x2 + 3150.0f * x4 - 28.0f * x6);
}

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
