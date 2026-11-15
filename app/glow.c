// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "glow.h"

#define GLOW_KINDS_PER_SIDE 3

static const uint8_t RING_CELL[GLOW_KINDS_PER_SIDE * GLOW_KINDS_PER_SIDE][8] = {
    {0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x03},
    {0x00, 0x00, 0x00, 0x00, 0x00, 0xAA, 0xDD, 0xFF},
    {0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x80},
    {0x07, 0x03, 0x05, 0x03, 0x07, 0x03, 0x05, 0x03},
    {0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
    {0xC0, 0xE0, 0xC0, 0xA0, 0xC0, 0xE0, 0xC0, 0xA0},
    {0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
    {0xFF, 0xEE, 0x55, 0x00, 0x00, 0x00, 0x00, 0x00},
    {0x40, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
};

static uint32_t side_of(int v, int first, int size) {
    return v < first + 1 ? 0u : v > first + size - 2 ? 2u : 1u;
}

static void cell(grid* g, const glow_rect* r, int col, int row, uint8_t color, int on) {
    if (col < 0 || row < 0 || col >= (int)GRID_COLS || row >= (int)GRID_ROWS) {
        return;
    }
    if (on) {
        (void)grid_overlay(g, (uint32_t)col, (uint32_t)row,
                           RING_CELL[side_of(row, r->row - 1, r->h + 2) * GLOW_KINDS_PER_SIDE +
                                     side_of(col, r->col - 1, r->w + 2)],
                           color);
    } else {
        grid_overlay_clear(g, (uint32_t)col, (uint32_t)row);
    }
}

static void ring(grid* g, const glow_rect* r, uint8_t color, int on) {
    int last_col = r->col + r->w;
    int last_row = r->row + r->h;
    for (int col = r->col - 1; col <= last_col; col++) {
        cell(g, r, col, r->row - 1, color, on);
        cell(g, r, col, last_row, color, on);
    }
    for (int row = r->row; row < last_row; row++) {
        cell(g, r, r->col - 1, row, color, on);
        cell(g, r, last_col, row, color, on);
    }
}

void glow_set(grid* g, glow_rect* shown, const glow_rect* lit, uint8_t color) {
    if (shown->w) {
        ring(g, shown, 0, 0);
    }
    if (lit->w) {
        ring(g, lit, color, 1);
    }
    *shown = *lit;
}
