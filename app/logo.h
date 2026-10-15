// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's logo as a pluggable asset (docs/art/README.md "Logo"): the boot's title card and the
 * About cracktro draw whichever logo logo_default names, through this interface only.
 *
 * A logo is a table in flash: its footprint in cells, what it costs, and a draw function that
 * paints one frame of its animation into the footprint on the cell grid (app/grid.h). Draw
 * sets every cell of the footprint, glyph, colours and overlay (an overlay it does not need is
 * cleared), so the caller composes it like any other part of the screen, every frame or once.
 * Its animation runs on the caller's frame count (25 fps); frame 0 is the still.
 *
 * The first logo, Pulse Train, is drawn in code with font glyphs and overlays. A pixel-art
 * logo (an indexed bitmap of up to 240 x 240 pixels, 4 bits a pixel, in flash) fits the same
 * table: its draw would hand the footprint to a direct pixel region of the renderer instead of
 * glyphs (docs/art/README.md "Logo bitmaps", not built yet).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LOGO_H
#define SIDOU_LOGO_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const char* name;
    /* The footprint, in cells. */
    uint8_t cols;
    uint8_t rows;
    /* Overlay cells (GRID_OVERLAYS) it takes at most in a frame. */
    uint8_t overlays;
    /* Its cost: the most footprint cells one frame changes from the frame before. */
    uint8_t cells_per_frame;
    /* Frame frame of the animation, its top-left cell at (col, row), on colour bg. */
    void (*draw)(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
} logo;

/* Pulse Train: slotted raster letters on a pulse wave. The stripes roll through the raster
 * ramp one band every 4 frames (frames 2, 6, 10, ...), the pulse wave moves 4 pixels and its
 * duty cycle sweeps every 4 frames (frames 4, 8, ...): odd frames change nothing. */
extern const logo logo_pulse_train;

/* The logo the boot and About show: Pulse Train until the user picks one. */
extern const logo* const logo_default;

/* l's frame frame at (col, row) on bg. */
void logo_draw(const logo* l, grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
/* The column that centres l on the screen. */
uint32_t logo_center_col(const logo* l);

#ifdef __cplusplus
}
#endif

#endif
