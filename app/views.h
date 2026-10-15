// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's views on the screen (app/screen.h) and the panel's routing through them
 * (docs/reference/controls.md).
 *
 * The views:
 * - STATUS, the root: chip, preset, octave, the sound parameters, the notes and the effect
 *   slots. LIVE (M1e Task 4) takes its place.
 * - MENU (HOME): CHIP, PRESET, OCTAVE and the views to open, in a frame. SELECT moves the
 *   cursor, ALGORITHM changes the value under it or opens the view.
 * - CHARSET: the font and the palette, the art reference.
 *
 * Routing, for each button press and encoder turn:
 * 1. global: HOME opens the menu on the root view, and goes back one view anywhere else;
 * 2. the screen: the capture, then the view on top (screen_input);
 * 3. the panel (app/panel.h): what nobody consumed plays the instrument as in M1a and M1c,
 *    in every view: OCT-/OCT+, FX, K1..K4, and the effect slots' EDIT layer (EDIT held + K1..K4,
 *    SCL, ENV). The layer stays here, global, until the CHIP view takes the slots over.
 * The note keys and MASTER do not pass through here: they always play.
 *
 * Every frame the instrument's state is compared with the last frame's: a change of octave,
 * effect slot or algorithm shows a toast, whatever made it (panel, menu or MIDI), and a chip
 * model change also runs the raster over the border.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_VIEWS_H
#define SIDOU_VIEWS_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "panel.h"
#include "screen.h"
#include "widget.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The status screen's colours: light blue on blue. */
#define VIEWS_TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
/* The menu's items and the rows it shows. */
#define VIEWS_MENU_ITEMS 4u
/* Raster steps for a chip model change: the ramp once round. */
#define VIEWS_RASTER_STEPS SCREEN_RAMP

typedef struct {
    screen s;
    live* l;
    const char* version; /* "V" and the version, at most 14 characters */
    menu home;
    /* The instrument as the last frame saw it, for the toasts. */
    uint8_t model;
    int8_t octave;
    uint8_t fx_select;
    uint8_t algorithm[LIVE_SLOTS];
} views;

extern const view views_status;
extern const view views_menu;
extern const view views_charset;

/* The status view on g, showing l. */
void views_init(views* v, grid* g, live* l, const char* version);
/* Button press edges and the held buttons, as panel_buttons takes them: routed as above, and
 * what nobody consumed goes on to panel_buttons. */
void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held);
/* Detent steps of encoder enc, as panel_encoder takes them. */
void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps);
/* One frame: the toasts and raster of what changed, then screen_frame. */
void views_frame(views* v, uint32_t now_ms);

#ifdef __cplusplus
}
#endif

#endif
