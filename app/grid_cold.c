// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "grid.h"

#include "font.h"

void grid_box(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
              int round) {
    if (w < 2u || h < 2u) {
        return;
    }
    uint32_t right = col + w - 1u;
    uint32_t bottom = row + h - 1u;
    grid_fill(g, col + 1u, row, w - 2u, 1, FONT_BOX_H, attr);
    grid_fill(g, col + 1u, bottom, w - 2u, 1, FONT_BOX_H, attr);
    grid_fill(g, col, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    grid_fill(g, right, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    grid_put(g, col, row, round ? FONT_ROUND_TL : FONT_BOX_TL, attr);
    grid_put(g, right, row, round ? FONT_ROUND_TR : FONT_BOX_TR, attr);
    grid_put(g, col, bottom, round ? FONT_ROUND_BL : FONT_BOX_BL, attr);
    grid_put(g, right, bottom, round ? FONT_ROUND_BR : FONT_BOX_BR, attr);
}
