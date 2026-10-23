// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The help card (docs/reference/controls.md "Help"): HOME held opens it over the view that shows
 * (app/views.c views_flow), what the controls do there in the state the view is in (CHIP by its
 * focus, STEP with a step held, TRACKER with REC on, SETTINGS on STORE: views.c help_card_of), at
 * 2x in the menu's style: the title bar (HELP, or the page 1/2 on a card of two pages, and the
 * card's name on the right), then up to eight lines a page, the control on the left in `hot` and
 * what it does on the right in `value`.
 *
 * It is a view over the one under it, entered with a cut. On a card of two pages SELECT turns
 * the page; any other button or encoder (SELECT on a card of one page) closes it and does nothing
 * else (its capture takes the event and marks it done; the views pop it at their next frame); a
 * note plays as ever; it stays open after HOME is let go until then. It draws in units (the title
 * bar, then each line, the last one with the rows under it), as many as fit HELP_FRAME_CELLS a
 * frame, so that no frame sends more than 200 cells whatever it covers; a unit is the rows it
 * owns, blanked and written. Nothing draws over it (no toast, raster or popup: app/views.c).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_HELP_VIEW_H
#define SIDOU_HELP_VIEW_H

#include <stdint.h>

#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cards, by the view (and the view's state) they are for. */
typedef enum {
    HELP_CARD_LIVE = 0,
    HELP_CARD_MENU = 1,    /* the main menu */
    HELP_CARD_CHIP = 2,    /* CHIP's focus on the chip */
    HELP_CARD_TRACKER = 3, /* REC off */
    HELP_CARD_SCENE = 4,
    HELP_CARD_STATUS = 5,
    HELP_CARD_STEP = 6,    /* the song views, a card for each */
    HELP_CARD_ORDER = 7,
    HELP_CARD_INSTR = 8,
    HELP_CARD_TABLE = 9,
    HELP_CARD_MIDI = 10,   /* the MIDI page */
    HELP_CARD_SONG = 11,   /* the SONG menu */
    HELP_CARD_SETTINGS = 12, /* the menu's SETTINGS page */
    HELP_CARD_ABOUT = 13,
    HELP_CARD_CHARSET = 14,
    HELP_CARD_FILES = 15,    /* the SONG menu's pages: LOAD's and DELETE's list of files, */
    HELP_CARD_NAME = 16,     /* SAVE AS's name */
    HELP_CARD_CONFIRM = 17,  /* and a confirmation */
    /* The views' states whose controls mean something else: */
    HELP_CARD_LIVE_SLOT = 18,   /* LIVE on a slot's page (the slot and its algorithm: arg) */
    HELP_CARD_CHIP_TABS = 19,   /* CHIP's focus on the A/B tabs */
    HELP_CARD_CHIP_SLOT = 20,   /* on a slot (arg) */
    HELP_CARD_EXPERT = 21,      /* on EXPERT */
    HELP_CARD_STEP_HELD = 22,   /* STEP with a step held */
    HELP_CARD_STEP_REC = 23,    /* STEP with REC on and the song playing (arg: the quantise) */
    HELP_CARD_TRACKER_EDIT = 24, /* TRACKER with REC on, the song stopped */
    HELP_CARD_TRACKER_REC = 25,  /* and playing (arg: the quantise) */
    HELP_CARD_STORE = 26,        /* SETTINGS on STORE */
    HELP_CARD_ARP = 27,          /* TABLE's ARP helper */
    HELP_CARDS = 28
} help_card_id;

/* Lines a page of a card has at most and the pages, characters of a line (the control, a space
 * or more, what it does), a page's units (the title bar and the lines), and the cells a frame
 * draws at most. */
#define HELP_LINES 8u
#define HELP_PAGES 2u
#define HELP_LINE_CHARS 14u
#define HELP_UNITS (1u + HELP_LINES)
#define HELP_FRAME_CELLS 190u

/* What a line does can be the state's own words: a first byte HELP_TEXT_K1..HELP_TEXT_K4 is the
 * name of that knob's control of the slot in `arg` (its slot in the low byte, its algorithm in the
 * next, app/chip_view.h chip_view_label: DRIVE, TONE; UNUSED where the algorithm has none),
 * HELP_TEXT_QUANT the quantise in `arg` (SONG_VIEW_QUANT_*: NEAREST, OFF, COUNT-IN). */
#define HELP_TEXT_K1 1
#define HELP_TEXT_K4 4
#define HELP_TEXT_QUANT 5
/* The longest of those words, its terminator included. */
#define HELP_TEXT_WORD 9u

/* A card: its name (right-aligned in the title bar) and `count` lines, each the control and
 * what it does in one string, a NUL between: "SELECT\0PAGE", "EDIT+ALG\0SAVE"; past HELP_LINES
 * the lines are its second page. */
typedef struct {
    const char* name;
    const char* const* lines;
    uint8_t count;
} help_card;

extern const help_card help_cards[HELP_CARDS];
extern const view help_view_view;

typedef struct {
    uint16_t drawn; /* bit u: unit u is on the grid (0 the title bar, 1.. the lines) */
    uint16_t arg;   /* the state's words (HELP_TEXT_*) */
    uint8_t card;   /* help_card_id */
    uint8_t page;   /* the page shown, 0 or 1 */
    uint8_t done;   /* a button or an encoder: close it (the views pop it) */
} help_view;

/* The card for `card` on top of s, cut in, in the stack's slot kept for it (screen_push_over:
 * over a full stack too), at its first page, `arg` its state's words; its capture set. -1 when
 * no slot is left (a card already open). */
int help_view_open(help_view* h, screen* s, uint32_t card, uint32_t arg);
/* Its capture (screen_capture): takes every event; SELECT turns a card of two pages, anything
 * else marks the card done. */
int help_view_capture(void* ctx, const screen_event* e);
/* What a line of h's card does, `what` as the card has it (after the control's NUL): itself, or
 * the state's word into buf (HELP_TEXT_WORD) for a HELP_TEXT_* code. */
const char* help_view_what(const help_view* h, const char* what, char* buf);

#ifdef __cplusplus
}
#endif

#endif
