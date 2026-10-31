// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_HELP_VIEW_H
#define SIDOU_HELP_VIEW_H

#include <stdint.h>

#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    HELP_CARD_LIVE = 0,
    HELP_CARD_MENU = 1,
    HELP_CARD_CHIP = 2,
    HELP_CARD_TRACKER = 3,
    HELP_CARD_SCENE = 4,
    HELP_CARD_STATUS = 5,
    HELP_CARD_STEP = 6,
    HELP_CARD_ORDER = 7,
    HELP_CARD_INSTR = 8,
    HELP_CARD_TABLE = 9,
    HELP_CARD_MIDI = 10,
    HELP_CARD_SONG = 11,
    HELP_CARD_SETTINGS = 12,
    HELP_CARD_ABOUT = 13,
    HELP_CARD_CHARSET = 14,
    HELP_CARD_FILES = 15,
    HELP_CARD_NAME = 16,
    HELP_CARD_CONFIRM = 17,
    HELP_CARD_LIVE_SLOT = 18,
    HELP_CARD_CHIP_TABS = 19,
    HELP_CARD_CHIP_SLOT = 20,
    HELP_CARD_EXPERT = 21,
    HELP_CARD_STEP_HELD = 22,
    HELP_CARD_STEP_REC = 23,
    HELP_CARD_TRACKER_EDIT = 24,
    HELP_CARD_TRACKER_REC = 25,
    HELP_CARD_STORE = 26,
    HELP_CARD_ARP = 27,
    HELP_CARD_RESET = 28,
    HELP_CARD_SIDS = 29,
    HELP_CARD_PLAYING = 30,
    HELP_CARD_UNSAVED = 31,
    HELP_CARD_LAB_OSC = 32,
    HELP_CARD_LAB_ENVELOPE = 33,
    HELP_CARD_LAB_FILTER = 34,
    HELP_CARD_LAB_MOD = 35,
    HELP_CARD_LAB_FX = 36,
    HELP_CARD_LAB_MACRO = 37,
    HELP_CARD_LAB_NAME = 38,
    HELP_CARD_LIFT = 39,
    HELP_CARDS = 40
} help_card_id;

#define HELP_LINES 8u
#define HELP_PAGES 2u
#define HELP_LINE_CHARS 14u
#define HELP_UNITS (1u + HELP_LINES)
#define HELP_FRAME_CELLS 190u

#define HELP_TEXT_K1 1
#define HELP_TEXT_K4 4
#define HELP_TEXT_QUANT 5
#define HELP_TEXT_WORD 9u

typedef struct {
    const char* name;
    const char* const* lines;
    uint8_t count;
} help_card;

extern const help_card help_cards[HELP_CARDS];
extern const view help_view_view;

typedef struct {
    uint16_t drawn;
    uint16_t arg;
    uint8_t card;
    uint8_t page;
    uint8_t done;
} help_view;

int help_view_open(help_view* h, screen* s, uint32_t card, uint32_t arg);
int help_view_capture(void* ctx, const screen_event* e);
const char* help_view_what(const help_view* h, const char* what, char* buf);

#ifdef __cplusplus
}
#endif

#endif
