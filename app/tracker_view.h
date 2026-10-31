// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_TRACKER_VIEW_H
#define SIDOU_TRACKER_VIEW_H

#include <stdint.h>

#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TRACKER_VIEW_TOP 3u
#define TRACKER_VIEW_ROWS 12u
#define TRACKER_VIEW_ROW_CELLS 2u
#define TRACKER_VIEW_BUILD_ROWS 6u
#define TRACKER_VIEW_SCROLL_ROW 5u
#define TRACKER_VIEW_PAGE 8u
#define TRACKER_VIEW_CONTEXT 4u
#define TRACKER_VIEW_NUM_COL 0u
#define TRACKER_VIEW_NOTE_COL 3u
#define TRACKER_VIEW_INS_COL 10u
#define TRACKER_VIEW_CMD_COL 15u
#define TRACKER_VIEW_BAND_W 18u
#define TRACKER_VIEW_STRIP_COL 26u
#define TRACKER_VIEW_STRIP_STEP 2u
#define TRACKER_VIEW_TAG_ROW 2u
#define TRACKER_VIEW_ORDER_ROW 27u
#define TRACKER_VIEW_SAY_ROW 28u
#define TRACKER_VIEW_HINT_ROW 29u
#define TRACKER_VIEW_CHANCE_COL 2u
#define TRACKER_VIEW_DICE_ROW 6u
#define TRACKER_VIEW_FIELD_NONE 0u
#define TRACKER_VIEW_FIELD_NOTE 1u
#define TRACKER_VIEW_FIELD_INSTRUMENT 2u
#define TRACKER_VIEW_FIELD_COMMAND 3u

extern const view tracker_view_view;

int32_t tracker_view_top(uint32_t row, int scroll);
void tracker_view_cell(const uint8_t cell[4], char out[9]);
void tracker_view_order_entry(uint8_t entry, char out[4]);
const char* tracker_view_command_name(uint32_t cmd);
void tracker_view_note(screen* s, song_view* sv, uint32_t note);
int tracker_view_popup(const song_view* sv, uint32_t encoder, int edit, char* name, char* value,
                       uint32_t* fill, uint32_t* max);
void tracker_view_buttons(song_view* sv, screen* s, uint32_t held);

#ifdef __cplusplus
}
#endif

#endif
