// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* CHIP, from the menu: a chip's profile, its character and the effect slots on one page, for
 * chip A or chip B (design doc section 7; docs/reference/controls.md "CHIP").
 *
 * The tabs (M2b Task 5): A and B, the one shown in reverse video, KEYS beside the letter of
 * the chip the keys play (app/keys_route.h). What a tab shows and edits follows which chip the
 * keys play: chip A during a song is the song's, so its tab shows the song's setup and edits it
 * (keys_route_edit_song_setup; K1..K4 turned by the view itself, PANEL_PAGE_NONE, which MIDI
 * learn binds nothing to); every other tab shows the keys' setup, live's, and edits it (the
 * other chip follows it in the background: with no song the B tab says FOLLOWS KEYS). The
 * effect slots are one chain on the mix of both chips: their frames show on both tabs, and the
 * B tab's help line says SHARED.
 *
 * The page, in PETSCII frames (docs/art/README.md "CHIP"):
 * - row 2: CHIP and the tabs, the shown one in reverse video (white while the tab focus is on);
 * - CHIP (rows 4-7): the profile (6581 R2, 6581 R3, 6581 R4AR, 8580 R5, WORN 6581) and its
 *   model as asked for (the chip plays them once its spare is prepared, 70 to 150 ms later:
 *   app/reg_ring.h);
 * - CHARACTER (rows 8-13): age/heat, DAC sloppiness, output thump and filter bleed, each a bar
 *   and its value 0..127;
 * - DIST, MOD and FX (4 rows each from row 14): the slot and its algorithm in the frame's
 *   title, and its Amount, A, B and Mix, each named for what it does in that algorithm
 *   (OVERDRIVE: DRIVE, TONE, BIAS); an unused control and a slot that is off are greyed;
 * - row 26: the help line.
 *
 * Focus: the tabs, the chip (its profile and character, both frames) or a slot. A play mark in
 * the margin and white frames show it. SELECT moves it, clamped at both ends: the tabs, CHIP,
 * DIST, MOD, FX. ALGORITHM on the tabs switches A and B; K1..K4 there turn the chip's.
 * The slot focus is live's selected slot (fx_select), the one SCL selects and EDIT + K1..K4
 * turns everywhere: SELECT onto a slot selects it, and SCL on a slot moves the focus with it.
 * ALGORITHM steps the focused section's choice, clamped: the profile on the chip, the
 * algorithm on a slot. K1..K4 turn the focused section's four values (panel_knob_param, which
 * MIDI learn reads too, follows chip_view_page): age, DAC, thump and bleed on the chip; Amount,
 * A, B and Mix on a slot. Everything else plays as on LIVE.
 *
 * It composes when what it shows changed (its changed function): a change from the panel or
 * from MIDI shows at the next frame. The page is built off the grid in a canvas, then its
 * changed cells are copied onto the grid within a budget of CHIP_VIEW_FRAME_CELLS cells a frame
 * less what the toast, the raster and a cover take (screen_layer_cells), as SCENE copies its
 * canvas: what does not fit goes on from that cell the next frame. Cells covered this frame
 * (MIDI learn's frame, a toast that shows) are left alone; the border's cells while the raster
 * runs too (it draws over them). Under a wipe or a banded cut, the rows it reveals are copied
 * whole. An idle page costs a comparison of a few bytes a frame.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_CHIP_VIEW_H
#define SIDOU_CHIP_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "keys_route.h"
#include "live.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cells a frame of CHIP sends at most: 200 cells and their windows are about 17.5 ms of
 * SPI, inside the 20 ms budget (LIVE, SCENE and the title cards hold to the same). */
#define CHIP_VIEW_FRAME_CELLS 200u
/* The sections SELECT moves between: the chip, then the slots. */
#define CHIP_VIEW_CHIP 0u
#define CHIP_VIEW_SECTIONS (1u + LIVE_SLOTS)
/* chip_view_focus with the focus on the tabs, above the chip. */
#define CHIP_VIEW_TABS 0xffu
/* The frames: CHIP from row CHIP_VIEW_CHIP_ROW, CHARACTER from CHIP_VIEW_CHARACTER_ROW, slot k
 * from CHIP_VIEW_SLOT_ROW + CHIP_VIEW_SLOT_ROWS * k; each from column CHIP_VIEW_COL,
 * CHIP_VIEW_W wide. */
#define CHIP_VIEW_COL 2u
#define CHIP_VIEW_W 26u
#define CHIP_VIEW_CHIP_ROW 4u
#define CHIP_VIEW_CHARACTER_ROW 8u
#define CHIP_VIEW_SLOT_ROW 14u
#define CHIP_VIEW_SLOT_ROWS 4u
#define CHIP_VIEW_HELP_ROW 26u
/* A control's name on the page, terminator included ("DEPTH"). */
#define CHIP_VIEW_LABEL 6u
/* What the page shows, as bytes (chip_view.c look): the tab's profile and model (asked for),
 * its four character controls, every slot field, the selected slot, the focus, the tab, the
 * tabs' focus, the keys' chip and what the help line says. */
#define CHIP_VIEW_LOOK (2u + (LIVE_CHAR_FIELDS - 1u) + LIVE_SLOTS * LIVE_SLOT_FIELDS + 6u)

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} chip_view_canvas;

typedef struct {
    live* l;
    keys_route* kr;  /* NULL: one chip's setup (live's) on both tabs, no song (host tests) */
    uint8_t chip;    /* 1: the focus is on the chip; 0: on the selected slot (live's fx_select) */
    uint8_t tabs;    /* 1: the focus is on the tabs (chip is 1 too: K1..K4 turn the chip's) */
    uint8_t tab;     /* the tab shown: 0 A, 1 B */
    uint8_t built;   /* the canvas shows drawn */
    uint8_t copying; /* the canvas has cells the grid does not have yet (the budget) */
    uint16_t cursor; /* the cell the copy goes on from */
    uint32_t held;   /* frames whose copy the budget cut short (diagnostic) */
    uint8_t drawn[CHIP_VIEW_LOOK];
    chip_view_canvas cv;
} chip_view;

extern const view chip_view_view;

/* The focus on the chip, on l, tab A, no keys_route. */
void chip_view_init(chip_view* cv, live* l);
/* The two chips' routing (the device's, app/keys_route.h): which the keys play, the song. */
void chip_view_set_keys(chip_view* cv, keys_route* kr);
/* The section in focus: CHIP_VIEW_TABS, CHIP_VIEW_CHIP, or 1 + the selected slot. */
uint32_t chip_view_focus(const chip_view* cv);
/* What K1..K4 turn while the page shows (app/panel.h panel.page): PANEL_PAGE_CHARACTER on the
 * tabs and the chip, PANEL_PAGE_NONE there on a song's chip A (the view turns them),
 * PANEL_PAGE_SLOT on a slot. */
uint32_t chip_view_page(const chip_view* cv);
/* 1 while the tab shows a song's chip A: its setup is the song's (keys_route.song_want). */
int chip_view_song_tab(const chip_view* cv);
/* The name of a slot's field (LIVE_SLOT_AMOUNT, _A, _B, _MIX) for its algorithm alg, as the
 * page shows it, into buf (CHIP_VIEW_LABEL): what it does in that algorithm (core/fx/fx_slots.h)
 * and 1, or its generic name (AMT, A, B, MIX) and 0 for a field the algorithm does not use and
 * every field of a slot that is off. */
int chip_view_label(uint32_t slot, uint32_t alg, uint32_t field, char* buf);

#ifdef __cplusplus
}
#endif

#endif
