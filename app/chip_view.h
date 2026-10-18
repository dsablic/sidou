// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* CHIP, from the menu: a chip's profile, its character and the effect slots, a section at a time,
 * for chip A or chip B (design doc section 7; docs/reference/controls.md "CHIP"; drawn to be read
 * at 23 mm after the screen-readability prototype's chipNew).
 *
 * The tabs (M2b Task 5): A and B, which chip the page shows, in the title "CHIP A" at 2x (yellow
 * while the tabs have the focus), with KEYS:A or KEYS:B on the right for the chip the keys
 * play (app/keys_route.h). What a tab shows and edits follows which chip the keys play: chip A
 * during a song is the song's, so its tab shows the song's setup and edits it
 * (keys_route_song_wants; K1..K4 turned by the view itself, PANEL_PAGE_NONE, which MIDI
 * learn binds nothing to); every other tab shows the keys' setup, live's, and edits it (the
 * other chip follows it in the background: with no song the B tab's note says FOLLOWS KEYS).
 * The effect slots are one chain on the mix of both chips: the note says SLOTS SHARED.
 *
 * The page (docs/art/README.md "CHIP"), on the theme's colours (app/theme.h):
 * - row 2: the section tabs CHIP, DIST, MOD, FX across the width; the focused one yellow;
 * - rows 4-6, the section's choice at 3x: the chip's model (6581, 8580) and its profile (R2, R3,
 *   R4AR, R5, WORN, USR 3), or the slot's algorithm; row 7 names them small (and says SAVE TO
 *   USER 3 in yellow once a save target is picked, where the names stand);
 * - rows 9-14, the section's four controls in the knobs' places (K1..K4, as LIVE's knob row):
 *   each a label small in the knob's colour (AGE, DAC, THUMP, BLEED; a slot's, named for what
 *   they do in its algorithm), its value at 3x in white (at 2x from 100: three digits do not fit
 *   a column), a bar in the knob's colour and the knob's name K1..K4;
 * - rows 16-21, the other sections' state at 2x, one line each: DIST, MOD and FX with their
 *   algorithms, or, with a slot in focus, the chip's profile and the other two slots;
 * - rows 27-28: the tab's note (SONG SETUP, FOLLOWS KEYS, SLOTS SHARED...) and the help line.
 * An unused control (the algorithm has no use for it) and every control of a slot that is off are
 * dark grey.
 *
 * Focus: the tabs (A and B), the chip (the CHIP section) or a slot. SELECT moves it, clamped at
 * both ends: the tabs, CHIP, DIST, MOD, FX, EXPERT. ALGORITHM on the tabs switches A and B; K1..K4 there
 * turn the chip's.
 * The slot focus is live's selected slot (fx_select), the one SCL selects and EDIT + K1..K4
 * turns everywhere: SELECT onto a slot selects it, and SCL on a slot moves the focus with it.
 * ALGORITHM steps the focused section's choice, clamped: the profile on the chip, the
 * algorithm on a slot. K1..K4 turn the focused section's four values (panel_knob_param, which
 * MIDI learn reads too, follows chip_view_page): age, DAC, thump and bleed on the chip; Amount,
 * A, B and Mix on a slot. Everything else plays as on LIVE.
 * EXPERT pages four independent controls with ALGORITHM, K1/K2 above K3/K4 in two readable
 * pairs: names at 2x in the knob's colour, values with units at 2x/3x. Empty positions do
 * nothing. EDIT + ALGORITHM and SAVE retain the USER target behavior on EXPERT too.
 * Macro moves release their related expert overrides; the page keeps that reminder visible.
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
#include "user_profile.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cells a frame of CHIP sends at most: 200 cells and their windows are about 17.5 ms of
 * SPI, inside the 20 ms budget (LIVE, SCENE and the title cards hold to the same). */
#define CHIP_VIEW_FRAME_CELLS 200u
/* The ordinary four-section strip; EXPERT is a separate final SELECT stop. */
#define CHIP_VIEW_CHIP 0u
#define CHIP_VIEW_SECTIONS (1u + LIVE_SLOTS)
#define CHIP_VIEW_EXPERT CHIP_VIEW_SECTIONS
#define CHIP_VIEW_EXPERT_PAGES ((SID_EXPERT_COUNT + 3u) / 4u)
/* The other sections' lines on the page: all but the one in focus. */
#define CHIP_VIEW_SECTIONS_SHOWN LIVE_SLOTS
/* chip_view_focus with the focus on the tabs, above the chip. */
#define CHIP_VIEW_TABS 0xffu
/* The page's rows (docs/art/README.md "CHIP"): the title at 2x on rows 0-1, the tabs, the
 * choice at 3x, the names under it, the four controls (labels, values at 3x, bars, K names), the
 * other sections' lines at 2x, and the tab's note and the help at the foot. */
#define CHIP_VIEW_TAB_ROW 2u
#define CHIP_VIEW_CHOICE_ROW 4u
#define CHIP_VIEW_NAMES_ROW 7u
#define CHIP_VIEW_LABEL_ROW 9u
#define CHIP_VIEW_VALUE_ROW 10u
#define CHIP_VIEW_BAR_ROW 13u
#define CHIP_VIEW_TAG_ROW 14u
#define CHIP_VIEW_SUMMARY_ROW 16u
#define CHIP_VIEW_NOTE_ROW 27u
#define CHIP_VIEW_HELP_ROW 28u
/* A control's name on the page, terminator included ("DEPTH"). */
#define CHIP_VIEW_LABEL 6u
/* What the page shows, as bytes (chip_view.c look): the tab's profile and model (asked for),
 * its four character controls, every slot field, the selected slot, the focus, the tab and the
 * tabs' focus in one byte, the keys' chip, what the tab's note says, then the applied USER slot
 * with the save target, whether the target holds a profile, and the theme. */
#define CHIP_VIEW_LOOK (2u + (LIVE_CHAR_FIELDS - 1u) + LIVE_SLOTS * LIVE_SLOT_FIELDS + 7u)
/* A toast's text, terminator included ("OVERWRITE USER 3?"). */
#define CHIP_VIEW_MSG 24u

/* Cells, colours and the characters of large text's pieces (app/grid.h grid_canvas). */
typedef grid_canvas chip_view_canvas;

typedef struct {
    live* l;
    keys_route* kr;  /* NULL: one chip's setup (live's) on both tabs, no song (host tests) */
    uint8_t chip;    /* 1: the focus is on the chip; 0: selected slot or expert */
    uint8_t tabs;    /* 1: the focus is on the tabs (chip is 1 too: K1..K4 turn the chip's) */
    uint8_t tab;     /* the tab shown: 0 A, 1 B */
    uint8_t expert;  /* final SELECT stop; chip and tabs are both 0 */
    uint8_t expert_page; /* four direct K1..K4 controls per page */
    uint8_t built;   /* the canvas shows drawn */
    uint8_t copying; /* the canvas has cells the grid does not have yet (the budget) */
    uint16_t cursor; /* the cell the copy goes on from */
    uint32_t held;   /* frames whose copy the budget cut short (diagnostic) */
    uint8_t drawn[CHIP_VIEW_LOOK];
    chip_view_canvas cv;
    /* USER profiles (M2a Task 6). users: where a saved slot is written (NULL: only live keeps
     * it). target: n + 1, the slot SAVE stores into (EDIT + ALGORITHM), 0 none; confirm: n + 1,
     * an overwrite SAVE asked about. keys_route.song_user tracks the song tab's USER setup,
     * live.user_applied the keys' tab's. msg: what the last SAVE or pick says. */
    user_profiles* users;
    uint8_t target;
    uint8_t confirm;
    char msg[CHIP_VIEW_MSG];
} chip_view;

extern const view chip_view_view;

/* The focus on the chip, on l, tab A, no keys_route. */
void chip_view_init(chip_view* cv, live* l);
/* The two chips' routing (the device's, app/keys_route.h): which the keys play, the song. */
void chip_view_set_keys(chip_view* cv, keys_route* kr);
/* CHIP_VIEW_TABS, CHIP_VIEW_CHIP, 1 + the selected slot, or CHIP_VIEW_EXPERT. */
uint32_t chip_view_focus(const chip_view* cv);
/* What K1..K4 turn while the page shows (app/panel.h panel.page): PANEL_PAGE_CHARACTER on the
 * tabs and the chip, PANEL_PAGE_NONE there on a song's chip A (the view turns them),
 * PANEL_PAGE_SLOT on a slot, PANEL_PAGE_EXPERT_FIRST + the page on EXPERT (NONE on song A). */
uint32_t chip_view_page(const chip_view* cv);
/* Where a saved USER slot is written (app/user_profile.h); NULL until set. */
void chip_view_set_users(chip_view* cv, user_profiles* users);
/* SAVE's press and release while the view shows (app/save_key.h, the precedence of
 * app/views.c), with the focus on the chip or EXPERT: into the save target; else into the USER slot the
 * tab's setup was picked from; else nothing (CHIP_VIEW_SAVE_NONE: the settings save). A slot
 * that holds a profile asks first (the message OVERWRITE USER 3?; SAVE again stores). The tab's
 * setup is stored: the keys' on the keys' tabs, the song's (keys_route.song_want, never a
 * parameter lock's) on a song's chip A. */
typedef enum {
    CHIP_VIEW_SAVE_NONE = 0,
    CHIP_VIEW_SAVE_PENDING = 1, /* handed to the store: its result comes from user_profiles_tick */
    CHIP_VIEW_SAVE_MSG = 2      /* toast cv->msg now */
} chip_view_save_result;
chip_view_save_result chip_view_save(chip_view* cv, uint32_t now_ms);
/* The USER slot the tab's setup was picked from, 1..8, or 0 (the CHIP frame's PROFILE row says
 * USER n then, and the popup). */
int chip_view_user(const chip_view* cv);
/* The save target (EDIT + ALGORITHM on the chip): USER slot 1..8, or 0 none. */
#define CHIP_VIEW_TARGET(cv) ((cv)->target)
/* 1 while the tab shows a song's chip A: its setup is the song's (keys_route.song_want). */
int chip_view_song_tab(const chip_view* cv);
/* The name of a slot's field (LIVE_SLOT_AMOUNT, _A, _B, _MIX) for its algorithm alg, as the
 * page shows it, into buf (CHIP_VIEW_LABEL): what it does in that algorithm (core/fx/fx_slots.h)
 * and 1, or its generic name (AMT, A, B, MIX) and 0 for a field the algorithm does not use and
 * every field of a slot that is off. */
int chip_view_label(uint32_t slot, uint32_t alg, uint32_t field, char* buf);
/* Expert names and values fit the popup's 12 characters. Values are resolved raw 0..127;
 * the formatter adds the parameter's units or its discrete choice. */
const char* chip_view_expert_name(uint32_t index);
void chip_view_expert_text(uint32_t index, uint32_t value, char* out);
/* Unit readouts use the setup's effective physical value, preserving unquantized defaults. */
void chip_view_expert_setup_text(const chip_setup* setup, uint32_t index, char* out);
/* The shown tab's resolved expert value (the song's setup on song A, the keys' elsewhere). */
uint32_t chip_view_expert_value(const chip_view* cv, uint32_t index);

#ifdef __cplusplus
}
#endif

#endif
