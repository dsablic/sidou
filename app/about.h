// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* ABOUT, from the menu: the credits as a cracktro, after the browser prototype's, trimmed to
 * the SPI budget (docs/art/README.md "About").
 *
 * Top to bottom on black: two raster bars sweeping behind the logo (app/logo.h), a ramp line,
 * the credits pages from CREDITS.md (app/credits.h) fading in and out over a starfield,
 * another ramp line, and a sine scroller.
 *
 * What it costs is planned frame by frame, so that no frame sends much more than the average:
 * - the bars are 5 bands of 4 pixels (a lower-half glyph gives a cell two bands); one of them
 *   moves a band on each odd frame, so each moves at most every 4 frames;
 * - the logo's stripes and wave move on even frames (logo_pulse_train);
 * - the credits fade on even frames, two text rows at most at a time;
 * - the scroller moves ABOUT_SCROLL_PX pixels a frame: each letter is an overlay shifted
 *   across the cells, bobbing a whole cell row at a time on a sine;
 * - the stars are one-pixel overlays in three layers, moving a pixel every 4, 2 or 1 frames.
 *
 * It composes every frame, and holds still while a wipe or a raster runs (screen_animating):
 * the wipe that opens it sends its own rows and nothing else.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_ABOUT_H
#define SIDOU_ABOUT_H

#include <stdint.h>

#include "grid.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A credits page takes this many frames (4.8 s). Its rows cascade in and out top first, each
 * ABOUT_ROW_DELAY frames after the one above and shown ABOUT_ROW_FRAMES frames, fading
 * through 4 levels a level every other frame. */
#define ABOUT_PAGE_FRAMES 120u
#define ABOUT_ROW_DELAY 4u
#define ABOUT_ROW_FRAMES 86u
#define ABOUT_STARS 24u
#define ABOUT_BARS 2u
/* The scroller's speed. */
#define ABOUT_SCROLL_PX 2u

typedef struct {
    const char* version;     /* "V" and the version, on the first page */
    uint32_t frame;          /* frames of the animation shown since it opened */
    uint8_t bar[ABOUT_BARS]; /* each bar's centre, in half cell rows */
} about;

extern const view about_view;

void about_init(about* a, const char* version);

#ifdef __cplusplus
}
#endif

#endif
