// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_TITLE_CARD_H
#define SIDOU_TITLE_CARD_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    TITLE_CARD_OSCILLATOR = 0,
    TITLE_CARD_SCENE = 1,
    TITLE_CARD_CHIP = 2,
    TITLE_CARDS = 3
} title_card_id;

typedef enum {
    TITLE_CARD_GLOW = 0,
    TITLE_CARD_WIPE = 1,
    TITLE_CARD_HOLD = 2,
    TITLE_CARD_DONE = 3
} title_card_phase;

#define TITLE_CARD_LEVELS 4u
#define TITLE_CARD_LEVEL_HOLD 2u
#define TITLE_CARD_WIPE_COLS 3u
#define TITLE_CARD_HOLD_FRAMES 25u
#define TITLE_CARD_FRAME_CELLS 200u
#define TITLE_CARD_GLOW_HALF 15u

typedef struct {
    const live* l;
    uint32_t note_ons;
    uint16_t frames;
    uint16_t cursor;
    uint8_t id;
    uint8_t phase;
    uint8_t level;
    uint8_t held;
    uint8_t cols;
    uint8_t settled;
    uint8_t skip;
} title_card;

extern const char* const title_card_titles[TITLE_CARDS];

extern const uint8_t title_card_glow[TITLE_CARD_GLOW_HALF][TITLE_CARD_GLOW_HALF];
extern const grid_bitmap title_card_bitmaps[TITLE_CARDS];
extern const uint32_t title_card_cells[TITLE_CARDS][GRID_ROWS];

extern const view title_card_view;

void title_card_init(title_card* c, title_card_id id, const live* l);
int title_card_done(const title_card* c);
int title_card_capture(void* ctx, const screen_event* e);
void title_card_glow_cell(uint32_t col, uint32_t row, uint32_t level, uint8_t* ch,
                          uint8_t* attr);

#ifdef __cplusplus
}
#endif

#endif
