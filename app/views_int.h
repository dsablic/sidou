// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_VIEWS_INT_H
#define SIDOU_VIEWS_INT_H

#include "views.h"

#include <stddef.h>
#include <string.h>

#include "chip_spread.h"
#include "font.h"
#include "lab_song.h"

#define VERSION_END_COL 21u
#define MIDI_LINES 2u
#define STATUS_READY_ROW 28u
#define MENU_ROW 4u
#define MENU_HELP_ROW 24u
#define MENU_CHARS ((GRID_COLS - 2u) / 2u)
#define MENU_FRAME_CELLS 178u
#define ASK_BASE 0x20u
#define ASK_EDIT 8u
#define ASK_MASTER 7u
#define ASK_PARAM 0x40u
#define MENU_PART_BLANK 1u
#define MENU_PART_TITLE 2u
#define MENU_PART_HELP 4u
#define MENU_PART_LINES 8u

enum { ARMED_NONE, ARMED_FORMAT, ARMED_RESET };
enum { SONG_MENU_ROOT, SONG_MENU_DIR, SONG_MENU_NAME, SONG_MENU_CONFIRM };

static inline void* song_state(views* v, const view* w) {
    return w == &tracker_view_view ? (void*)&v->song : w == &step_view_view ? (void*)&v->step :
           (void*)&v->pages;
}

static inline void views_octave_text(char* buf, int32_t octave) {
    buf[0] = octave < 0 ? '-' : '+';
    buf[1] = (char)('0' + (octave < 0 ? -octave : octave));
    buf[2] = 0;
}

extern const char* const SLOT_NAMES[LIVE_SLOTS];
extern const menu_item SETTINGS_ITEMS[VIEWS_SETTINGS_ITEMS + 1u];
extern const menu_item MIDI_ITEMS[VIEWS_MIDI_ITEMS];

void views_put(grid* g, uint8_t attr, uint32_t row, uint32_t col, const char* s);
void views_append(char* buf, uint32_t cap, const char* s);
void views_join(char* buf, uint32_t cap, const char* a, const char* b);
void views_profile_label(char* buf, uint32_t id);
void views_number_text(char* buf, uint32_t n);
void views_sound(views* v, ui_sound_id id);
const view* views_shows(const views* v);
int curtain(const views* v);
void views_big_menu_compose(screen* s, views* v, menu* m, const char* title, grid* g, uint32_t parts);

void views_look(const views* v, uint32_t blink, uint8_t out[VIEWS_LOOK]);
int views_looks_changed(const views* v, uint32_t blink);

int song_refused(views* v);
void song_open(void* ctx, int32_t steps);
int song_menu_back(views* v);
void song_menu_ok(views* v);
void song_name(views* v, int direct);
int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill, uint32_t* max,
               uint32_t* accent);
int global(void* ctx, const screen_event* e);

#endif
