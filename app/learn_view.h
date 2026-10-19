// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI learn on the screen (app/screen.h) and the panel: the gesture of app/midi_learn.h
 * (docs/reference/controls.md, "MIDI learn" and "SAVE").
 *
 * A knob turned while SAVE is held (app/save_key.h) opens learn from any view on the knob's
 * parameter (learn_view_open): the LEARN view opens over the view on top, a frame on rows
 * LEARN_VIEW_ROW.. drawn over whatever the view under it composes (which keeps showing the
 * instrument), and takes the panel's events first (screen_capture):
 * - K1..K4: the knob's parameter, as the panel turns it on the view under the frame with the
 *   buttons held (panel_knob_param: the CHIP view's page, EDIT for the effect-slot shortcut),
 *   is picked instead; the knob does not turn it. K4 on LIVE's page without EDIT is the
 *   preset, which has no CC: a toast says so (and the frame does not open on it);
 * - HOME: cancels (from the clear-all question too: the bindings stay).
 * SAVE never reaches the capture: the views take it (app/views.c) and call
 * learn_view_save (SAVE pressed and released: forget, or confirm the question) and
 * learn_view_ask_clear (SAVE held + HOME).
 * Everything else goes on as usual: the note keys play, SCL selects the slot whose controls
 * EDIT + K1..K4 then pick, OCT and FX work; on the CHIP view, SELECT moves the focus and
 * ALGORITHM steps its choice under the frame (the view's input passes them on to CHIP, and to
 * no other view: the menu's ALGORITHM would open a view over the frame).
 * The frame says what to do next: <PARAM> <- SEND A CC, and SAVE: FORGET CC n when the
 * parameter has one (else HOME: CANCEL). When learn ends (a CC came over MIDI, SAVE cleared or
 * forgot, HOME, the timeout) the view closes and a
 * toast says how: LEARNED CC 21 -> CUTOFF, and so on. A CC ends learn in the main loop, so the
 * view closes at the next frame, or at the panel's next event if that comes first: a knob then
 * turns as usual, and SAVE or HOME do nothing more (the toast stays).
 * The frame hides the overlays the view under it draws on its cells; the others stay, there
 * and back (SCREEN_OVER). It covers its cells (screen_cover), so a view under it that holds to
 * a budget of cells (LIVE, SCENE) leaves them alone and counts what the frame sends.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LEARN_VIEW_H
#define SIDOU_LEARN_VIEW_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "panel.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The full-width band, rows 20..25, above the toast: three 2x lines from column 1,
 * LEARN_VIEW_CHARS each, with a cell of margin either side. */
#define LEARN_VIEW_COL 0u
#define LEARN_VIEW_ROW 20u
#define LEARN_VIEW_W 30u
#define LEARN_VIEW_H 6u
#define LEARN_VIEW_CHARS 14u
#define LEARN_VIEW_FRAME_CELLS 200u
/* A parameter's name on the screen, terminator included ("DIST MIX"). */
#define LEARN_VIEW_NAME 10u

typedef struct {
    live* l;
    midi_learn* ml; /* NULL: no learn */
    screen* s;      /* while open */
    const panel* p; /* while open: the panel whose page the knobs follow */
    /* What the frame showed at its last compose: the state, the parameter, its bound CC. */
    uint8_t drawn[3];
} learn_view;

extern const view learn_view_view;

/* ml may be NULL: no learn. */
void learn_view_init(learn_view* lv, live* l, midi_learn* ml);
/* Knob enc turned while SAVE is held (the buttons held: held): opens the view over the one on
 * top, with its capture, learning the knob's parameter on p's page (panel_knob_param). 0, or
 * -1 with no learn, no room on the stack or a parameter with no CC (a toast says so). */
int learn_view_open(learn_view* lv, screen* s, const panel* p, uint32_t enc, uint32_t held);
/* 1 while the view is open. */
int learn_view_is_open(const learn_view* lv);
/* SAVE pressed and released while the view is open: the clear-all question confirmed, or the
 * picked parameter's CC forgotten (or, with none, a toast and the frame stays). */
void learn_view_save(learn_view* lv);
/* SAVE held + HOME while the view is open: CLEAR ALL MAPPINGS? */
void learn_view_ask_clear(learn_view* lv);
/* Every frame, before screen_frame: while the view is on top, the timeout, and the end of
 * learn (by MIDI or the timeout) closes it with its toast. */
void learn_view_frame(learn_view* lv, screen* s, uint32_t now_ms);
/* Parameter param's name, as the frame and the toasts say it, into buf (LEARN_VIEW_NAME). */
void learn_view_param_name(uint32_t param, char* buf);

#ifdef __cplusplus
}
#endif

#endif
