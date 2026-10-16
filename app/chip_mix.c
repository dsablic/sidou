// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_mix.h"

_Static_assert(CHIP_MIX_FADE == 1u << 7, "the fade's weight is a 7-bit shift");

static int16_t mix_sat16(int32_t v) {
    return (int16_t)(v > 32767 ? 32767 : v < -32768 ? -32768 : v);
}

void chip_mix(int16_t* a, const int16_t* b, uint32_t n, chip_mix_fade fade) {
    if (fade == CHIP_MIX_FULL) {
        for (uint32_t i = 0; i < n; i++) {
            a[i] = mix_sat16((int32_t)a[i] + b[i]);
        }
        return;
    }
    for (uint32_t i = 0; i < n; i++) {
        int32_t w = fade == CHIP_MIX_IN ? (int32_t)i + 1 : (int32_t)CHIP_MIX_FADE - 1 - (int32_t)i;
        a[i] = mix_sat16((int32_t)a[i] + ((b[i] * w) >> 7));
    }
}
