// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LOGO_H
#define SIDOU_LOGO_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const char* name;
    uint8_t cols;
    uint8_t rows;
    uint16_t overlays;
    uint16_t cells_per_frame;
    void (*draw)(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
    void (*scene)(grid* g, uint32_t frame);
} logo;

extern const logo logo_pulse_train;

extern const logo logo_metal_chrome;
#define LOGO_METAL_BAND_ROW 10u

extern const logo logo_cracktro_chrome;
#define LOGO_CHROME_BAND_ROW 9u

extern const grid_bitmap logo_metal_chrome_bitmap;
extern const grid_bitmap logo_cracktro_chrome_bitmap;

extern const logo* const logo_default;

void logo_draw(const logo* l, grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
uint32_t logo_center_col(const logo* l);

#ifdef __cplusplus
}
#endif

#endif
