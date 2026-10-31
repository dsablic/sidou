// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_STEP_VIEW_H
#define SIDOU_STEP_VIEW_H

#include <stdint.h>

#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

#define STEP_VIEW_STEPS 16u
#define STEP_VIEW_NONE 0xffu
#define STEP_VIEW_PAGE_ROW 2u
#define STEP_VIEW_RULER_ROW 3u
#define STEP_VIEW_LANES_ROW 5u
#define STEP_VIEW_LANE_ROWS 4u
#define STEP_VIEW_HELD_ROW 17u
#define STEP_VIEW_PADS_ROW 24u
#define STEP_VIEW_PAD_COL 2u
#define STEP_VIEW_STEP_COL 5u
#define STEP_VIEW_LANES 3u
#define STEP_VIEW_LOOK 52u

#define STEP_VIEW_WAVES 5u

typedef struct {
    uint8_t kind, voice, pattern, row;
    int16_t steps;
} step_act;
#define STEP_VIEW_QUEUE 4u

typedef struct {
    song_view* sv;
    uint8_t page;
    uint8_t held;
    uint8_t heard;
    uint8_t heard_off;
    uint8_t gate;
    uint8_t rev;
    uint8_t zones;
    uint8_t instr[3];
    uint16_t down;
    uint16_t touched;
    uint8_t follow;
    uint8_t follow_pattern, follow_row;
    uint8_t follow_cell[4];
    uint8_t qn;
    step_act q[STEP_VIEW_QUEUE];
    uint8_t drawn[STEP_VIEW_LOOK];
} step_view;

extern const view step_view_view;

void step_view_init(step_view* st, song_view* sv);
void step_view_buttons(step_view* st, screen* s, uint32_t held);
uint32_t step_view_keys(step_view* st, screen* s, uint32_t down, uint32_t pressed, int take);
void step_view_leds(const step_view* st, uint32_t* set, uint32_t* rows, uint32_t* head);
int step_view_popup(const step_view* st, uint32_t encoder, int edit, char name[SCREEN_POPUP_TEXT],
                    char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max);

#ifdef __cplusplus
}
#endif

#endif
