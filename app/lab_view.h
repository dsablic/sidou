// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LAB: one LIVE sound shaped on five pages, each with its shape drawn from the live values above
 * four cards for K1..K4 (docs/plans/2026-10-25-lab-design.md; docs/art/README.md "LAB").
 *
 * The pages, in SELECT's order (clamped at both ends): OSC, ENVELOPE, FILTER, MOD, FX. On each:
 * - rows 0-1, the header at 2x: the page's name in yellow, and its place `2/5` on the right;
 * - rows 2-3, the sound's name at 2x in white with `*` while it differs from its base
 *   (live_preset_name);
 * - rows 4-17, the drawing's area (LAB_VIEW_ART_ROW..);
 * - rows 19-20, ALGORITHM's choice at 2x (`SAW SYNC`, `AUTHENTIC`, `NOTCH`, `ARP UP`, `DELAY`),
 *   `ALG` small on the right in the legend colour (ENVELOPE's also says ALL SOUNDS: the envelope
 *   mode is the instrument's, not the sound's);
 * - rows 22-27, the four cards in the knobs' places (LIVE's and CHIP's columns of 8, 7, 7 and 8
 *   cells): the card's name small in the knob's colour, its value in white at 3x (2x when it
 *   takes 3 characters), a bar in the knob's colour and the knob's name K1..K4;
 * - row 29, the hint line: LAB's gestures, a phrase at a time (LAB_VIEW_HINT_MS each), so that
 *   nothing in LAB is found only on the help card (a toast, on rows 27-28, leaves it).
 * A card or the choice that was just turned is lit (its name in reverse video) for
 * SCREEN_POPUP_MS, in place of the value popup (app/views.c lab_frame). A card the page has no
 * use for (FILTER's K4; a slot's control its algorithm does not use) is dark grey.
 *
 * The cards and the choice, by page (panel_lab_param names a card's parameter):
 * - OSC: PULSE (the pulse width, LIVE's walk of LIVE_PULSE_STEP; RATIO on a mono-sync sound
 *   without the pulse bit, LIVE's K3 on SYNC LEAD), PWM (the LFO's depth, 32 steps a detent), PWM
 *   HZ (its rate, 0.05 Hz a detent), TUNE (cents); the waveform, the combined ones, then the
 *   triangle's ring and the saw's and pulse's sync (lab_view_wave_choices).
 * - ENVELOPE: ATTACK, DECAY, SUSTAIN, RELEASE (0..15); the envelope mode.
 * - FILTER: CUTOFF (LIVE's K1 walk), RESO, VOICES (the voices routed, 1 2 3); LP, BP, HP, NOTCH.
 * - MOD: the vibrato's rate (VIB HZ), depth and delay, and the velocity target (VELO); ARP off
 *   or its order.
 * - FX: the selected slot's Amount, A, B and Mix (named for its algorithm, app/chip_view.h);
 *   the slot's algorithm, OFF included. SCL selects the next slot, as everywhere.
 *
 * Input: SELECT steps the pages; K1..K4 turn the cards (EDIT held too: no hidden combination);
 * ALGORITHM steps the choice (clamped); ENV, LFO and FX open ENVELOPE, MOD and FX. Everything
 * else goes on to the views' global handler and the panel (the keys play; OCT, PLAY, SEQ, REC,
 * GLO, MASTER, PRESETS and SCL as everywhere). HOME goes back to LIVE (the menu's LAB replaces
 * the menu: app/views.c lab_open).
 *
 * It composes as CHIP does (app/chip_view.h): the text is built in the song views' shared canvas
 * (SONG_VIEW_CANVAS_LAB) when what it shows changed (a hash of it, lab_view.look), and the
 * canvas's changed cells are copied onto the grid within LAB_VIEW_FRAME_CELLS a frame less what
 * the screen's layers take. LAB composes under MIDI learn's frame too.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LAB_VIEW_H
#define SIDOU_LAB_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "scope.h"
#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The pages, in SELECT's order. */
#define LAB_PAGE_OSC 0u
#define LAB_PAGE_ENVELOPE 1u
#define LAB_PAGE_FILTER 2u
#define LAB_PAGE_MOD 3u
#define LAB_PAGE_FX 4u
#define LAB_PAGES 5u
/* The cells a frame of LAB sends at most (as CHIP's and LIVE's: about 17.5 ms of SPI). */
#define LAB_VIEW_FRAME_CELLS 200u
/* The page's rows (above). */
#define LAB_VIEW_NAME_ROW 2u
#define LAB_VIEW_ART_ROW 4u
#define LAB_VIEW_ART_ROWS 14u
#define LAB_VIEW_CHOICE_ROW 19u
#define LAB_VIEW_LABEL_ROW 22u
#define LAB_VIEW_VALUE_ROW 23u
#define LAB_VIEW_BAR_ROW 26u
#define LAB_VIEW_TAG_ROW 27u
#define LAB_VIEW_HINT_ROW 29u
/* A hint phrase shows this long, then the next. */
#define LAB_VIEW_HINT_MS 2000u
#define LAB_VIEW_HINTS 3u
/* lab_view.hl: the choice (ALGORITHM) lit; 1..4 a card. */
#define LAB_VIEW_HL_CHOICE 5u
/* OSC's choices: TRIANGLE .. PULSE SYNC. */
#define LAB_VIEW_WAVES 11u
/* A card's or the choice's text, terminator included ("TRI+SAW+PUL"). */
#define LAB_VIEW_TEXT 14u

typedef struct {
    live* l;
    song_view* canvas; /* borrows canvas->cv; nothing else of the song views' */
    const scope* sc;   /* the audio's tap (NULL: none) */
    uint32_t look;     /* the hash of what the canvas shows */
    uint32_t hl_until; /* the lit card's or choice's time */
    uint16_t cursor;   /* the cell the copy goes on from */
    uint8_t page;      /* LAB_PAGE_* */
    uint8_t built;     /* the canvas shows drawn */
    uint8_t copying;   /* the canvas has cells the grid does not have yet (the budget) */
    uint8_t hl;        /* 1..4 the card lit, LAB_VIEW_HL_CHOICE the choice, 0 none */
} lab_view;

extern const view lab_view_view;

/* On l, the page OSC; shared's canvas borrowed while it composes; sc the tap (NULL: none). */
void lab_view_init(lab_view* lv, live* l, song_view* shared, const scope* sc);
/* The page LAB shows (and opens at). */
void lab_view_page_set(lab_view* lv, uint32_t page);
/* What K1..K4 turn while it shows (app/panel.h panel.page): PANEL_PAGE_LAB_FIRST + the page. */
uint32_t lab_view_panel_page(const lab_view* lv);
/* Knob k (0..3), or the choice (LAB_VIEW_HL_CHOICE - 1), just turned: lit for SCREEN_POPUP_MS
 * from now_ms, in place of the value popup (app/views.c). */
void lab_view_flash(lab_view* lv, uint32_t k, uint32_t now_ms);
/* The page's name ("ENVELOPE"). */
const char* lab_view_page_name(uint32_t page);
/* Card k (0..3) of the page as the page shows it: its name into name and its value into value
 * (LAB_VIEW_TEXT each), its bar's fill out of *max (max 0: no bar); 1, or 0 for a card the page
 * has no use for (dark grey). */
int lab_view_card(const lab_view* lv, uint32_t page, uint32_t k, char* name, char* value,
                  uint32_t* fill, uint32_t* max);
/* The page's choice as it shows it into out (LAB_VIEW_TEXT). */
void lab_view_choice(const lab_view* lv, uint32_t page, char* out);
/* The hint line's phrase n (0..LAB_VIEW_HINTS - 1): at most GRID_COLS characters. */
const char* lab_view_hint(uint32_t n);
/* OSC's choice i (0..LAB_VIEW_WAVES - 1) as $d404's waveform, ring and sync bits, and the sound's
 * as a choice: its own, else the one of its waveform, else 0. */
uint8_t lab_view_wave_control(uint32_t i);
uint32_t lab_view_wave_of(uint8_t control);

#ifdef __cplusplus
}
#endif

#endif
