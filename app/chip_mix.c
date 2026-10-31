// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_mix.h"

_Static_assert(CHIP_MIX_FADE == 1u << 7, "the fade's weight is a 7-bit shift");

static int32_t mix_weight(chip_mix_fade fade, uint32_t i) {
    return fade == CHIP_MIX_FULL ? 128
           : fade == CHIP_MIX_IN ? (int32_t)i + 1
                                 : (int32_t)CHIP_MIX_FADE - 1 - (int32_t)i;
}

__attribute__((noinline, unused)) static void mix_past(int16_t* a, const int16_t* b,
                                                       int16_t* over, uint32_t from, uint32_t n,
                                                       chip_mix_fade fade) {
    for (uint32_t i = 0; i < from; i++) {
        over[i] = 0;
    }
    for (uint32_t i = from; i < n; i++) {
        int32_t v = (int32_t)a[i] + ((b[i] * mix_weight(fade, i)) >> 7);
        int32_t s = v > 32767 ? 32767 : v < -32768 ? -32768 : v;
        a[i] = (int16_t)s;
        over[i] = (int16_t)(v - s);
    }
}

int chip_mix(int16_t* a, const int16_t* b, int16_t* over, uint32_t n, chip_mix_fade fade) {
    if (fade == CHIP_MIX_FULL) {
        for (uint32_t i = 0; i < n; i++) {
            int32_t v = (int32_t)a[i] + b[i];
            if (v > 32767 || v < -32768) {
                mix_past(a, b, over, i, n, fade);
                return 1;
            }
            a[i] = (int16_t)v;
        }
        return 0;
    }
    for (uint32_t i = 0; i < n; i++) {
        int32_t v = (int32_t)a[i] + ((b[i] * mix_weight(fade, i)) >> 7);
        if (v > 32767 || v < -32768) {
            mix_past(a, b, over, i, n, fade);
            return 1;
        }
        a[i] = (int16_t)v;
    }
    return 0;
}
