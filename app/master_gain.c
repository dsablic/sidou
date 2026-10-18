// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "master_gain.h"

uint32_t master_gain_of(uint32_t value, uint32_t max) {
    if (value >= max) {
        return MASTER_UNITY;
    }
    return (value * MASTER_UNITY + max / 2u) / max;
}

static int16_t master_scaled(int32_t x, int32_t g) {
    return (int16_t)((x * g + (int32_t)(MASTER_UNITY / 2u)) >> 15);
}

_Static_assert(MASTER_BLOCK == 1u << 7, "the ramp's divide is a shift");

void master_gain_apply(int16_t* lr, uint32_t from, uint32_t to) {
    if (from == MASTER_UNITY && to == MASTER_UNITY) {
        return;
    }
    if (from == to) {
        /* Four samples a pass: the loop's own instructions are a quarter of a sample's. */
        const int32_t g = (int32_t)to;
        int16_t* p = lr;
        for (uint32_t i = 0; i < 2u * MASTER_BLOCK; i += 4u, p += 4) {
            int32_t x0 = p[0], x1 = p[1], x2 = p[2], x3 = p[3];
            p[0] = master_scaled(x0, g);
            p[1] = master_scaled(x1, g);
            p[2] = master_scaled(x2, g);
            p[3] = master_scaled(x3, g);
        }
        return;
    }
    const int32_t a = (int32_t)from, d = (int32_t)to - (int32_t)from;
    for (uint32_t f = 0; f < MASTER_BLOCK; f++) {
        int32_t g = a + ((d * (int32_t)(f + 1u)) >> 7);
        lr[2u * f] = master_scaled(lr[2u * f], g);
        lr[2u * f + 1u] = master_scaled(lr[2u * f + 1u], g);
    }
}
