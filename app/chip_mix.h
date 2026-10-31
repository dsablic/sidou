// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CHIP_MIX_H
#define SIDOU_CHIP_MIX_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CHIP_MIX_FADE 128u

typedef enum {
    CHIP_MIX_FULL = 0,
    CHIP_MIX_IN = 1,
    CHIP_MIX_OUT = 2
} chip_mix_fade;

int chip_mix(int16_t* a, const int16_t* b, int16_t* over, uint32_t n, chip_mix_fade fade);

#ifdef __cplusplus
}
#endif

#endif
