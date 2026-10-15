// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's logo as a pluggable asset (docs/art/README.md "Logo"): the boot's title card and the
 * About cracktro draw whichever logo logo_default names, through this interface only.
 *
 * A logo is a table in flash: its footprint in cells, what it costs, a draw function that
 * paints one frame of its animation into the footprint on the cell grid (app/grid.h), and
 * optionally a full-screen scene for the title card. Draw sets every cell of the footprint,
 * glyph, colours, overlay and pixel region (an overlay it does not need is cleared), so the
 * caller composes it like any other part of the screen, every frame or once. Its animation
 * runs on the caller's frame count (25 fps); frame 0 is the still.
 *
 * Metal Chrome, the default, and Cracktro Chrome are bitmaps (app/logo_bitmap.c, generated
 * from assets/logo/metal-chrome.png and cracktro-chrome.png): their draw hands the footprint
 * to the renderer's pixel region (grid_region), which shows the bitmap's cells pixel for
 * pixel. The grid has one region and it is the logo's: a screen shows one bitmap logo at a
 * time, a logo drawn in cells clears it, and so does a view switch (app/screen.h).
 * Pulse Train is drawn in code with font glyphs and overlays.
 *
 * Its background is the logo's own: no transparent one. The chrome logos' is their scene's
 * sky (black, stars and Metal Chrome's dithered glow). Pulse Train's letter cells use all three colours a cell holds
 * (background, the top band as a glyph, the bottom band as an overlay), so raster bars, two
 * colours a cell of their own, cannot pass behind them: tried in About, the bar's lower half
 * took its upper half's colour in every letter cell, and the letters' bottom bands and the
 * cyan wave vanished into bars of the same ramp colours, for 15 more cells a frame
 * (docs/art/README.md "Logo").
 *
 * Where it goes: About gives it ABOUT_LOGO_ROWS rows from ABOUT_LOGO_ROW (app/about.h). The
 * boot's title card shows its scene when it has one, with the card's text over it, and else
 * gives the logo BOOT_LOGO_ROWS rows from BOOT_LOGO_ROW (app/boot.h), centred
 * (logo_center_col); a host test checks each logo fits each.
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
    uint16_t overlays;
    /* Its cost: the most footprint cells one frame changes from the frame before. */
    uint16_t cells_per_frame;
    /* Frame frame of the animation, its top-left cell at (col, row), on colour bg. */
    void (*draw)(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
    /* Frame frame of its full-screen scene: every cell of the screen, the boot's title card
     * (app/boot.h) drawing its text over it. NULL: the card gives the logo a slot instead. */
    void (*scene)(grid* g, uint32_t frame);
} logo;

/* Pulse Train: slotted raster letters on a pulse wave. The stripes roll through the raster
 * ramp one band every 4 frames (frames 2, 6, 10, ...), the pulse wave moves 4 pixels and its
 * duty cycle sweeps every 4 frames (frames 4, 8, ...): odd frames change nothing. */
extern const logo logo_pulse_train;

/* Metal Chrome, chosen by the user (replacing Cracktro Chrome): spiked, blade-cut metal letters
 * with a red to white chrome bevel and a purple extruded shadow, on a dithered purple and red
 * glow. Its draw is the scene's lettering band, 30 x 9 cells (the scene's cell rows
 * LOGO_METAL_BAND_ROW..: the letters, their outline and their shadow, pixel rows 81..145, with
 * the glow and stars around them); its scene the whole 240 x 240 picture, the lettering over
 * stars, raster bars and a cross-flare glint, with a tagline. Still: nothing changes after
 * its first frame. */
extern const logo logo_metal_chrome;
#define LOGO_METAL_BAND_ROW 10u

/* Cracktro Chrome, the logo before Metal Chrome: chunky bevelled chrome letters in raster
 * bands with a purple extruded shadow. Its draw is the scene's lettering band, 30 x 8 cells
 * (the scene's cell rows LOGO_CHROME_BAND_ROW..); its scene the whole 240 x 240 picture, the
 * lettering over stars, raster bars and a grid floor. Still. */
extern const logo logo_cracktro_chrome;
#define LOGO_CHROME_BAND_ROW 9u

/* The scenes, 240 x 240 pixels each (app/logo_bitmap.c, generated from
 * assets/logo/metal-chrome.png and cracktro-chrome.png). */
extern const grid_bitmap logo_metal_chrome_bitmap;
extern const grid_bitmap logo_cracktro_chrome_bitmap;

/* The logo the boot and About show: Metal Chrome. */
extern const logo* const logo_default;

/* l's frame frame at (col, row) on bg. */
void logo_draw(const logo* l, grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg);
/* The column that centres l on the screen. */
uint32_t logo_center_col(const logo* l);

#ifdef __cplusplus
}
#endif

#endif
