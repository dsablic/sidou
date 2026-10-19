// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "master_gain.h"

/* MASTER_TAPER[0] = 0 and MASTER_TAPER[i] = round(32768 * 10^(-3 * (1 - i / 64))), i = 1..64:
 * -60 dB at 1, rising 0.94 dB an entry to unity at 64, so the pot's steps are even in dB. */
static const uint16_t MASTER_TAPER[65] = {
    0, 37, 41, 45, 50, 56, 63, 70, 78, 87, 96, 107, 120,
    133, 148, 165, 184, 205, 229, 255, 284, 316, 352, 392, 437, 487,
    542, 604, 673, 750, 835, 930, 1036, 1154, 1286, 1432, 1596, 1778, 1980,
    2206, 2457, 2737, 3049, 3397, 3784, 4215, 4696, 5231, 5827, 6491, 7231, 8055,
    8973, 9996, 11135, 12404, 13818, 15393, 17147, 19102, 21279, 23704, 26406, 29415, 32768,
};

uint32_t master_gain_of(uint32_t value, uint32_t max) {
    if (value >= max) {
        return MASTER_UNITY;
    }
    uint32_t x = (value * 1024u + max / 2u) / max; /* 0..1023 */
    uint32_t i = x >> 4, f = x & 15u;
    return (MASTER_TAPER[i] * (16u - f) + MASTER_TAPER[i + 1u] * f + 8u) >> 4;
}

static int16_t master_scaled(int32_t x, int32_t g) {
    return (int16_t)((x * g + (int32_t)(MASTER_UNITY / 2u)) >> 15);
}

_Static_assert(MASTER_BLOCK == 1u << MASTER_BLOCK_LOG2, "the block is a power of two");

void master_gain_apply(int16_t* lr, uint32_t frames_log2, uint32_t from, uint32_t to) {
    if (from == MASTER_UNITY && to == MASTER_UNITY) {
        return;
    }
    if (from == to) {
        /* Four samples a pass: the loop's own instructions are a quarter of a sample's. */
        const int32_t g = (int32_t)to;
        int16_t* p = lr;
        for (uint32_t i = 0; i < 2u << frames_log2; i += 4u, p += 4) {
            int32_t x0 = p[0], x1 = p[1], x2 = p[2], x3 = p[3];
            p[0] = master_scaled(x0, g);
            p[1] = master_scaled(x1, g);
            p[2] = master_scaled(x2, g);
            p[3] = master_scaled(x3, g);
        }
        return;
    }
    const int32_t a = (int32_t)from, d = (int32_t)to - (int32_t)from;
    for (uint32_t f = 0; f < 1u << frames_log2; f++) {
        int32_t g = a + ((d * (int32_t)(f + 1u)) >> frames_log2);
        lr[2u * f] = master_scaled(lr[2u * f], g);
        lr[2u * f + 1u] = master_scaled(lr[2u * f + 1u], g);
    }
}
