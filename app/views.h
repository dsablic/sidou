// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's views on the screen (app/screen.h) and the panel's routing through them
 * (docs/reference/controls.md).
 *
 * The views:
 * - LIVE, the root (app/live_view.h): the voices' oscilloscopes and envelopes, the filter's
 *   curve, the VU meters and the effect slots. What the screen shows from power-on; the tap
 *   on the audio runs only while it shows: on top, or under MIDI learn's frame.
 * - MENU (HOME): PRESET, OCTAVE, CARDS, KEYS (AUTO or A: which chip the keys play during a
 *   song, app/keys_route.h; in RAM until the settings persist) and the views to open (CHIP
 *   first), in a frame. SELECT
 *   moves the cursor, ALGORITHM changes the value under it or opens the view.
 * - CHIP (from the menu, behind THE CHIP's card): chip A's profile, character and effect slots
 *   (app/chip_view.h); SELECT moves the focus, ALGORITHM steps its choice, K1..K4 turn its
 *   values.
 * - STATUS (from the menu): the chip (with two chips both, KEYS marking the keys'), preset,
 *   octave, the sound parameters, the notes and the
 *   effect slots with every control, in numbers.
 * - SCENE (from the menu): generative pieces that react to the voices (app/scene_view.h);
 *   SELECT steps between them. The tap on the audio runs while it shows, as for LIVE.
 * - CHARSET: the font and the palette, the art reference.
 * - ABOUT: the credits, as a cracktro (app/about.h).
 * - BOOT: the boot sequence (app/boot.h), over LIVE at power-on (views_boot).
 * - LEARN: MIDI learn (app/learn_view.h), a frame over any view, from SAVE (views_learn).
 * - TITLE: a title card (app/title_card.h) over a major view entered from the menu: THE CHIP
 *   over CHIP, THE SCENE over SCENE, THE OSCILLATOR over LIVE when HOME goes back to it from
 *   the menu (not at power-on, after the boot, nor when MIDI learn's frame closes). It ends by
 *   itself, on any control (which does nothing else) or on a note (which plays), and the view
 *   under it cuts in (SCREEN_BANDS). The menu's CARDS item turns the cards off and on (on at
 *   power-on).
 *
 * Routing, for each button press and encoder turn:
 * 1. the capture (screen_capture), if a view set one: it sees HOME too, so it can take HOME
 *    as "cancel";
 * 2. global (screen_global): HOME opens the menu on the root view, and goes back one view
 *    anywhere else; SAVE opens LEARN over any view;
 * 3. the view on top;
 * 4. the panel (app/panel.h): what nobody consumed plays the instrument as in M1a and M1c,
 *    in every view: OCT-/OCT+, FX, K1..K4, and the effect slots' EDIT shortcut (EDIT held +
 *    K1..K4, SCL, ENV), which stays global next to the CHIP view (docs/reference/controls.md
 *    says why). K1..K4 turn the panel's page (panel.page): the CHIP view's focused section
 *    while it shows (on top or under MIDI learn's frame), LIVE's knobs in every other view; the
 *    views set it before each event, so the panel and MIDI learn read the same one.
 * The note keys and MASTER do not pass through here: they always play.
 *
 * The views compose only when what they show changed (app/screen.h): STATUS, MENU and CHIP keep
 * a copy of the instrument's fields they show, MENU its cursor and blink as well, and CHARSET
 * never changes. An idle screen costs a comparison a frame. LIVE, SCENE and ABOUT move every
 * frame and compose every frame.
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
#include "chip_view.h"
#include "grid.h"
#include "learn_view.h"
#include "live.h"
#include "live_view.h"
#include "panel.h"
#include "scene_view.h"
#include "scope.h"
#include "screen.h"
#include "title_card.h"
#include "widget.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The status screen's colours: light blue on blue. */
#define VIEWS_TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
/* The menu's items and the rows it shows, the title cards' option, and the items that open a
 * view. */
#define VIEWS_MENU_ITEMS 9u
#define VIEWS_MENU_CHIP 0u
#define VIEWS_MENU_CARDS 3u
#define VIEWS_MENU_KEYS 4u
#define VIEWS_MENU_STATUS 5u
#define VIEWS_MENU_SCENE 6u
#define VIEWS_MENU_CHARSET 7u
#define VIEWS_MENU_ABOUT 8u
/* Raster steps for a chip model change: the ramp once round. */
#define VIEWS_RASTER_STEPS SCREEN_RAMP
/* What the views show, as bytes (app/views.c look): the instrument's model (as asked for),
 * which chip the keys play (with two chips and the KEYS setting), the keys' profile and the
 * other chip's, the preset, octave, cutoff (2), resonance, pulse width (2), volume and selected slot, each
 * voice's sounding note, every slot field, the title cards' option, and the menu's cursor,
 * first row and blink. */
#define VIEWS_LOOK (17u + LIVE_VOICES + LIVE_SLOTS * LIVE_SLOT_FIELDS)

typedef struct {
    screen s;
    live* l;
    scope* sc; /* the audio's tap, NULL: none; on while LIVE or SCENE shows */
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
    panel* p; /* the panel of the last event (views_buttons, views_encoder); NULL before */
    boot boot;
    about about;
    chip_view chip;
    learn_view learn;
    live_view live;
    scene_view scene;
    title_card card;
    uint8_t cards; /* 1: title cards on (the menu's CARDS) */
} views;

extern const view views_status;
extern const view views_menu;
extern const view views_charset;

/* LIVE, the root view, on g, showing l and the audio's tap sc (NULL: none). */
void views_init(views* v, grid* g, live* l, scope* sc, const char* version);
/* The boot sequence over it from the next frame on: the full one if HOME was held at
 * power-on (home_held), whose first HOME press edge is then part of the hold (app/boot.h). */
void views_boot(views* v, int home_held);
/* SAVE arms MIDI learn over ml's bindings from now on (app/learn_view.h); NULL: SAVE does
 * nothing (views_init's default). */
void views_learn(views* v, midi_learn* ml);
/* Button press edges and the held buttons, as panel_buttons takes them: routed as above, and
 * what nobody consumed goes on to panel_buttons. */
void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held);
/* Detent steps of encoder enc, as panel_encoder takes them. */
void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps);
/* Half way between two frames: LIVE or SCENE, showing (on top or under MIDI learn's frame) and
 * still, prepares its next frame (live_view_prepare, scene_view_prepare). */
void views_prepare(views* v);
/* One frame: the boot's progress, a title card's end, the toasts and raster of what changed,
 * MIDI learn's end (learn_view_frame), then screen_frame. */
void views_frame(views* v, uint32_t now_ms);
/* The effect slots' names as the screens show them (at most 9 characters); "?" for an unknown
 * slot or algorithm. */
const char* views_slot_name(uint32_t slot);
const char* views_algorithm_name(uint32_t slot, uint32_t alg);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code 1 a dry run, < 0 a failure) in a frame over the
 * view, straight onto the grid: the main loop is inside the update session and runs no
 * frames. The next frame composes the view again. */
void views_update(views* v, uint32_t step, int32_t code);

#ifdef __cplusplus
}
#endif

#endif
