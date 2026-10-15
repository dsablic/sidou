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
 * - ABOUT: the credits, as a cracktro (app/about.h).
 * - BOOT: the boot sequence (app/boot.h), over STATUS at power-on (views_boot).
 *
 * Routing, for each button press and encoder turn:
 * 1. the capture (screen_capture), if a view set one: it sees HOME too, so it can take HOME
 *    as "cancel";
 * 2. global (screen_global): HOME opens the menu on the root view, and goes back one view
 *    anywhere else;
 * 3. the view on top;
 * 4. the panel (app/panel.h): what nobody consumed plays the instrument as in M1a and M1c,
 *    in every view: OCT-/OCT+, FX, K1..K4, and the effect slots' EDIT layer (EDIT held + K1..K4,
 *    SCL, ENV). The layer stays here, global, until the CHIP view takes the slots over.
 * The note keys and MASTER do not pass through here: they always play.
 *
 * The views compose only when what they show changed (app/screen.h): STATUS and MENU keep a
 * copy of the instrument's fields they show, MENU its cursor and blink as well, and CHARSET
 * never changes. An idle screen costs a comparison a frame.
 *
 * Every frame the instrument's state is compared with the last frame's: a change of octave,
 * effect slot or algorithm shows a toast, whatever made it (panel, menu or MIDI), and a chip
 * model change also runs the raster over the border; not while the boot shows, which nothing
 * draws over (its changes are taken as seen).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_VIEWS_H
#define SIDOU_VIEWS_H

#include <stdint.h>

#include "about.h"
#include "boot.h"
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
/* The menu's items and the rows it shows, and the items that open a view. */
#define VIEWS_MENU_ITEMS 5u
#define VIEWS_MENU_CHARSET 3u
#define VIEWS_MENU_ABOUT 4u
/* Raster steps for a chip model change: the ramp once round. */
#define VIEWS_RASTER_STEPS SCREEN_RAMP
/* What the views show, as bytes (app/views.c look): the instrument's model, preset, octave,
 * cutoff (2), resonance, pulse width (2), volume and selected slot, each voice's sounding
 * note, every slot field, and the menu's cursor, first row and blink. */
#define VIEWS_LOOK (13u + LIVE_VOICES + LIVE_SLOTS * LIVE_SLOT_FIELDS)

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
    /* What the view on top showed at its last compose: it composes again when this changes
     * (the views' changed functions). */
    uint8_t drawn[VIEWS_LOOK];
    boot boot;
    about about;
} views;

extern const view views_status;
extern const view views_menu;
extern const view views_charset;

/* The status view on g, showing l. */
void views_init(views* v, grid* g, live* l, const char* version);
/* The boot sequence over it from the next frame on: the full one if HOME was held at
 * power-on (home_held), whose first HOME press edge is then part of the hold (app/boot.h). */
void views_boot(views* v, int home_held);
/* Button press edges and the held buttons, as panel_buttons takes them: routed as above, and
 * what nobody consumed goes on to panel_buttons. */
void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held);
/* Detent steps of encoder enc, as panel_encoder takes them. */
void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps);
/* One frame: the boot's progress, the toasts and raster of what changed, then
 * screen_frame. */
void views_frame(views* v, uint32_t now_ms);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code 1 a dry run, < 0 a failure) in a frame over the
 * view, straight onto the grid: the main loop is inside the update session and runs no
 * frames. The next frame composes the view again. */
void views_update(views* v, uint32_t step, int32_t code);

#ifdef __cplusplus
}
#endif

#endif
