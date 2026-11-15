// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GLOW_H
#define SIDOU_GLOW_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t col;
    uint8_t row;
    uint8_t w;
    uint8_t h;
} glow_rect;

void glow_set(grid* g, glow_rect* shown, const glow_rect* lit, uint8_t color);

#ifdef __cplusplus
}
#endif

#endif
