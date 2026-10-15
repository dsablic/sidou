// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI learn on the screen (app/screen.h) and the panel: the gesture of app/midi_learn.h
 * (docs/reference/controls.md, "MIDI learn").
 *
 * SAVE (LEARN_VIEW_BUTTON) arms learn from any view: the LEARN view opens over it, a frame on
 * rows LEARN_VIEW_ROW.. drawn over whatever the view under it composes (which keeps showing
 * the instrument), and takes the panel's events first (screen_capture):
 * - K1..K4: the knob's parameter, as the panel turns it on the view under the frame with the
 *   buttons held (panel_knob_param: the CHIP view's page, EDIT for the effect-slot shortcut),
 *   is picked; the knob does not turn it. K4 on LIVE's page without EDIT is the preset, which
 *   has no CC: a toast says so;
 * - SAVE: the next step of midi_learn_button (clear all, confirm, forget);
 * - HOME: cancels.
 * Everything else goes on as usual: the note keys play, SCL selects the slot whose controls
 * EDIT + K1..K4 then pick, OCT and FX work; on the CHIP view, SELECT moves the focus and
 * ALGORITHM steps its choice under the frame (the view's input passes them on to CHIP, and to
 * no other view: the menu's ALGORITHM would open a view over the frame).
 * The frame says what to do next: TURN A KNOB, then <PARAM> <- SEND A CC. When learn ends (a
 * CC came over MIDI, the button cleared or forgot, HOME, the timeout) the view closes and a
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

#define LEARN_VIEW_BUTTON PANEL_BTN_SAVE
/* The frame: columns 2..27, rows 23..26 (over the status screen's READY.), the toast's row
 * below it free. Its text: two lines from column 4, a cell in from the frame,
 * LEARN_VIEW_CHARS each. */
#define LEARN_VIEW_COL 2u
#define LEARN_VIEW_ROW 23u
#define LEARN_VIEW_W 26u
#define LEARN_VIEW_H 4u
#define LEARN_VIEW_CHARS 22u
/* A parameter's name on the screen, terminator included ("DIST MIX"). */
#define LEARN_VIEW_NAME 10u

typedef struct {
    live* l;
    midi_learn* ml; /* NULL: SAVE does not arm learn */
    screen* s;      /* while open */
    const panel* p; /* while open: the panel whose page the knobs follow */
    /* What the frame showed at its last compose: the state, the parameter, its bound CC. */
    uint8_t drawn[3];
} learn_view;

extern const view learn_view_view;

/* ml may be NULL: no learn. */
void learn_view_init(learn_view* lv, live* l, midi_learn* ml);
/* SAVE: arms learn and opens the view over the one on top, with its capture; a knob picks its
 * parameter on p's page (panel_knob_param). 0, or -1 with no learn or no room on the stack
 * (nothing happens). */
int learn_view_open(learn_view* lv, screen* s, const panel* p);
/* Every frame, before screen_frame: while the view is on top, the timeout, and the end of
 * learn (by MIDI or the timeout) closes it with its toast. */
void learn_view_frame(learn_view* lv, screen* s, uint32_t now_ms);
/* Parameter param's name, as the frame and the toasts say it, into buf (LEARN_VIEW_NAME). */
void learn_view_param_name(uint32_t param, char* buf);

#ifdef __cplusplus
}
#endif

#endif
