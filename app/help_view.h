// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The help card (docs/reference/controls.md "Help"): HOME held opens it over the view that
 * shows (app/views.c views_flow), a short list of what the controls do there, at 2x in the
 * menu's style: the title bar (HELP, and the card's name on the right), then up to eight lines,
 * the control on the left in `hot` and what it does on the right in `value`.
 *
 * It is a view over the one under it, entered with a cut. Any button or encoder closes it and
 * does nothing else (its capture takes the event and marks it done; the views pop it at their
 * next frame); a note plays as ever; it stays open after HOME is let go until then. It draws in
 * units (the title bar, then each line, the last one with the rows under it), as many as fit
 * HELP_FRAME_CELLS a frame, so that no frame sends more than 200 cells whatever it covers; a
 * unit is the rows it owns, blanked and written. Nothing draws over it (no toast, raster or
 * popup: app/views.c).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_HELP_VIEW_H
#define SIDOU_HELP_VIEW_H

#include <stdint.h>

#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cards, by the view they are for. */
typedef enum {
    HELP_CARD_LIVE = 0,
    HELP_CARD_MENU = 1,    /* the menu and the MIDI page */
    HELP_CARD_CHIP = 2,
    HELP_CARD_TRACKER = 3, /* the song views */
    HELP_CARD_SCENE = 4,
    HELP_CARD_ALL = 5,     /* STATUS, ABOUT, CHARSET and the rest */
    HELP_CARDS = 6
} help_card_id;

/* Lines a card has at most, characters of a line (the control, a space or more, what it does),
 * its units (the title bar and the lines), and the cells a frame draws at most. */
#define HELP_LINES 8u
#define HELP_LINE_CHARS 14u
#define HELP_UNITS (1u + HELP_LINES)
#define HELP_FRAME_CELLS 190u

/* A card: its name (right-aligned in the title bar) and `count` lines, each the control and
 * what it does in one string, a NUL between: "SELECT\0PAGE", "EDIT+ALG\0SAVE". */
typedef struct {
    const char* name;
    const char* const* lines;
    uint8_t count;
} help_card;

extern const help_card help_cards[HELP_CARDS];
extern const view help_view_view;

typedef struct {
    uint16_t drawn; /* bit u: unit u is on the grid (0 the title bar, 1.. the lines) */
    uint8_t card;   /* help_card_id */
    uint8_t done;   /* a button or an encoder: close it (the views pop it) */
} help_view;

/* The card for `card` on top of s, cut in; its capture set. -1 when the stack is full. */
int help_view_open(help_view* h, screen* s, uint32_t card);
/* Its capture (screen_capture): takes every event and marks the card done. */
int help_view_capture(void* ctx, const screen_event* e);

#ifdef __cplusplus
}
#endif

#endif
