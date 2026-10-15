// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sine.h"

/* sin(2 pi i / 256) * 127 for the first quarter turn, i = 0..64. */
static const uint8_t QUARTER[65] = {
    0,   3,   6,   9,   12,  16,  19,  22,  25,  28,  31,  34,  37,  40,  43,  46,  49,
    51,  54,  57,  60,  63,  65,  68,  71,  73,  76,  78,  81,  83,  85,  88,  90,  92,
    94,  96,  98,  100, 102, 104, 106, 107, 109, 111, 112, 113, 115, 116, 117, 118, 120,
    121, 122, 122, 123, 124, 125, 125, 126, 126, 126, 127, 127, 127, 127,
};

int32_t sine_q7(uint32_t phase) {
    uint32_t p = phase % SINE_TURN;
    uint32_t q = p % 128u; /* the half turn */
    int32_t v = QUARTER[q <= 64u ? q : 128u - q];
    return p < 128u ? v : -v;
}

int32_t sine_scaled(uint32_t phase, int32_t amp) {
    int32_t x = sine_q7(phase) * amp;
    return x >= 0 ? (x + 63) / 127 : -((-x + 63) / 127);
}
