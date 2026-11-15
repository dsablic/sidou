// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "glow.h"

#define GLOW_RING_SOLID 1
#define GLOW_RING_DENSE 2
#define GLOW_RING_SPARSE 3

static int gap(int v, int lo, int hi) {
    return v < lo ? lo - v : v > hi ? v - hi : 0;
}

static int ring(int dx, int dy) {
    return (dx > dy ? dx : dy) + (dx && dy);
}

static int lit_pixel(int ring_at, int x, int y) {
    return ring_at == GLOW_RING_SOLID ||
           (ring_at == GLOW_RING_DENSE && ((x + y) & 3)) ||
           (ring_at == GLOW_RING_SPARSE && ((x + y) & 1));
}

static int cell_mask(uint8_t mask[8], int col, int row, const glow_rect* r) {
    int x0 = r->col * 8;
    int y0 = r->row * 8;
    int x1 = x0 + r->w * 8 - 1;
    int y1 = y0 + r->h * 8 - 1;
    int any = 0;
    if (!r->w) {
        return 0;
    }
    for (int y = row * 8; y < row * 8 + 8; y++) {
        int bits = 0;
        for (int x = col * 8; x < col * 8 + 8; x++) {
            bits = bits << 1 | (lit_pixel(ring(gap(x, x0, x1), gap(y, y0, y1)), x, y));
        }
        *mask++ = (uint8_t)bits;
        any |= bits;
    }
    return any;
}

static void sweep(grid* g, const glow_rect* area, const glow_rect* lit, uint8_t color) {
    if (!area->w) {
        return;
    }
    int c0 = area->col > 0 ? area->col - 1 : 0;
    int r0 = area->row > 0 ? area->row - 1 : 0;
    for (int row = r0; row <= area->row + area->h && row < (int)GRID_ROWS; row++) {
        for (int col = c0; col <= area->col + area->w && col < (int)GRID_COLS; col++) {
            uint8_t mask[8];
            if (cell_mask(mask, col, row, lit)) {
                (void)grid_overlay(g, (uint32_t)col, (uint32_t)row, mask, color);
            } else {
                grid_overlay_clear(g, (uint32_t)col, (uint32_t)row);
            }
        }
    }
}

void glow_set(grid* g, glow_rect* shown, const glow_rect* lit, uint8_t color) {
    sweep(g, shown, lit, color);
    sweep(g, lit, lit, color);
    *shown = *lit;
}
