// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The title cards (docs/art/README.md "Title cards"): a short full-screen card in the logo's
 * metal lettering, THE and a title (THE SCENE, THE OSCILLATOR, THE CHIP), shown as a view over
 * a major view entered from the menu (app/views.h), after the logo page's chapter cards.
 *
 * The sequence, frame by frame (25 fps):
 * - the glow fades in through TITLE_CARD_LEVELS ordered-dither levels: a purple and red glow
 *   drawn a cell at a time with the font's dither glyphs (D0..D15) in two neighbouring stops
 *   of black, blue, purple and red, from a table of levels (title_card_glow). Each level
 *   holds TITLE_CARD_LEVEL_HOLD frames once all its cells are out;
 * - the title wipes in from the left, TITLE_CARD_WIPE_COLS columns a frame: the card's bitmap
 *   (title_card_bitmaps, generated from assets/logo/title-<name>.png) in the grid's pixel region,
 *   growing over the same bitmap cells, so each frame sends the cells it adds and no others.
 *   The bitmap holds the pixels of the cells that differ from the glow (the letters, their
 *   halos, the rule, the glint, the stars; title_card_cells, a bit each), which turn blank
 *   so that the bitmap shows alone, and the glow's background stop flat in the others,
 *   under the glyph they keep;
 * - it holds TITLE_CARD_HOLD_FRAMES frames (1 s);
 * - it ends (title_card_done), and the views cut to the view under it in bands
 *   (SCREEN_BANDS).
 * The held frame is assets/logo/title-<name>.png pixel for pixel.
 *
 * The budget: every frame sends at most TITLE_CARD_FRAME_CELLS cells. A step that changes more
 * (the first level changes every cell of the menu it covers) goes out in bands, top first,
 * from where the last frame stopped, until every cell of it is out; then the next step.
 *
 * It holds nothing up and swallows no note: a button or an encoder ends it at the next frame
 * and does nothing else (its capture takes every event), and a note from the keys or MIDI
 * plays and ends it (live's note-on count tells, however short the note). The audio, the
 * keys and MIDI run as ever. Nothing draws over it (no toast or raster, app/views.c).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_TITLE_CARD_H
#define SIDOU_TITLE_CARD_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cards, in the order of the generated tables. */
typedef enum {
    TITLE_CARD_OSCILLATOR = 0, /* LIVE, back from the menu */
    TITLE_CARD_SCENE = 1,      /* SCENE */
    TITLE_CARD_CHIP = 2,       /* the CHIP view, M1c's: ready, not shown yet */
    TITLE_CARDS = 3
} title_card_id;

typedef enum {
    TITLE_CARD_GLOW = 0, /* the glow fading in */
    TITLE_CARD_WIPE = 1, /* the title wiping in */
    TITLE_CARD_HOLD = 2, /* the card held */
    TITLE_CARD_DONE = 3  /* over: the views cut to the view under it */
} title_card_phase;

/* The glow's levels. */
#define TITLE_CARD_LEVELS 4u
/* Frames each level holds once all its cells are out (the page's preview: 2). */
#define TITLE_CARD_LEVEL_HOLD 2u
/* Title columns revealed a frame: 3 x 30 cells, 90 at most. */
#define TITLE_CARD_WIPE_COLS 3u
/* Frames the whole card holds (1 s). */
#define TITLE_CARD_HOLD_FRAMES 25u
/* Cells a frame at most: 200 cells and their windows are about 17.5 ms of SPI, inside the
 * 20 ms budget (LIVE and SCENE hold to the same). */
#define TITLE_CARD_FRAME_CELLS 200u
/* The glow table's side: the screen's top-left quarter, mirrored (the glow is symmetric about
 * the screen's centre). */
#define TITLE_CARD_GLOW_HALF 15u

typedef struct {
    const live* l;
    uint32_t note_ons; /* live's note-on count at the start: a note since then ends it */
    uint16_t frames;   /* frames composed */
    uint16_t cursor;   /* the next cell the copy looks at */
    uint8_t id;        /* title_card_id */
    uint8_t phase;     /* title_card_phase */
    uint8_t level;     /* the glow's level being drawn, 1..TITLE_CARD_LEVELS */
    uint8_t held;      /* frames the step has held */
    uint8_t cols;      /* the title's columns revealed */
    uint8_t settled;   /* every cell of the step is out */
    uint8_t skip;      /* a button or an encoder */
} title_card;

/* What each card says, its title after THE: "OSCILLATOR", "SCENE", "CHIP". */
extern const char* const title_card_titles[TITLE_CARDS];

/* Generated (app/title_card_bitmap.c, from assets/logo/title-glow.png and title-<name>.png):
 * the glow's level in each cell of the screen's top-left quarter (0..47: 16 a stop, the
 * glyph's dither n the rest), each card's bitmap, and its cells that differ from the glow
 * (bit c of row r). */
extern const uint8_t title_card_glow[TITLE_CARD_GLOW_HALF][TITLE_CARD_GLOW_HALF];
extern const grid_bitmap title_card_bitmaps[TITLE_CARDS];
extern const uint32_t title_card_cells[TITLE_CARDS][GRID_ROWS];

extern const view title_card_view;

/* Card id, from its start; l's note-ons since now end it. */
void title_card_init(title_card* c, title_card_id id, const live* l);
/* 1 once it is over: skipped by a button or an encoder, a note played, or held to its end. */
int title_card_done(const title_card* c);
/* Its capture (screen_capture): takes every event, and any ends it. */
int title_card_capture(void* ctx, const screen_event* e);
/* Cell (col, row) of the glow at level (1..TITLE_CARD_LEVELS) as a font code and colours. */
void title_card_glow_cell(uint32_t col, uint32_t row, uint32_t level, uint8_t* ch,
                          uint8_t* attr);

#ifdef __cplusplus
}
#endif

#endif
