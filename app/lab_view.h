// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_VIEW_H
#define SIDOU_LAB_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "lab_edit.h"
#include "live.h"
#include "scope.h"
#include "screen.h"
#include "song_view.h"
#include "user_sound.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_PAGE_OSC 0u
#define LAB_PAGE_ENVELOPE 1u
#define LAB_PAGE_FILTER 2u
#define LAB_PAGE_MOD 3u
#define LAB_PAGE_FX 4u
#define LAB_PAGE_MACRO 5u
#define LAB_PAGES 6u
#define LAB_VIEW_FRAME_CELLS 200u
#define LAB_VIEW_NAME_ROW 2u
#define LAB_VIEW_ART_ROW 4u
#define LAB_VIEW_ART_ROWS 14u
#define LAB_VIEW_CHOICE_ROW 19u
#define LAB_VIEW_LABEL_ROW 22u
#define LAB_VIEW_VALUE_ROW 23u
#define LAB_VIEW_BAR_ROW 26u
#define LAB_VIEW_TAG_ROW 27u
#define LAB_VIEW_HINT_ROW 29u
#define LAB_VIEW_HINT_MS 2000u
#define LAB_VIEW_HINTS 4u
#define LAB_VIEW_HL_CHOICE 5u
#define LAB_VIEW_WAVES 11u
#define LAB_VIEW_TEXT 14u
#define LAB_VIEW_MODE_PAGES 0u
#define LAB_VIEW_MODE_NAME 1u
#define LAB_VIEW_MODE_ASK 2u
#define LAB_VIEW_NAME_SLOT_ROW 3u
#define LAB_VIEW_NAME_USED_ROW 5u
#define LAB_VIEW_NAME_LABEL_ROW 7u
#define LAB_VIEW_NAME_CHARS_ROW 9u
#define LAB_VIEW_NAME_LEGEND_ROW 12u
#define LAB_VIEW_NAME_ASK_ROW 24u
#define LAB_VIEW_AB_OFF 0u
#define LAB_VIEW_AB_A 1u
#define LAB_VIEW_AB_B 2u
#define LAB_VIEW_AB_COL 28u
#define LAB_VIEW_NO_RUN 0xFFu
#define LAB_VIEW_VARY_TRIES 8u
#define LAB_VIEW_ROLL_TRIES 16u
#define LAB_VIEW_NAME_KEYS 27u

typedef struct {
    live* l;
    user_sounds* us;
    song_view* canvas;
    const scope* sc;
    uint32_t look;
    uint32_t hl_until;
    uint16_t cursor;
    uint8_t page;
    uint8_t built;
    uint8_t copying;
    uint8_t hl;
    uint8_t hl_link;
    uint8_t art_done;
    uint8_t art_turn;
    uint32_t art;
    uint8_t kept_new;
    uint8_t mode;
    uint8_t slot;
    uint8_t cursor_at;
    uint8_t named;
    char name[LAB_SOUND_NAME + 1u];
    screen_lone lone;
    uint8_t edit_down;
    uint8_t edit_spoiled;
    uint8_t ab;
    uint8_t peek;
    uint8_t loads;
    uint8_t other[LAB_SOUND_BYTES];
    lab_edit undo;
    uint8_t run;
    uint16_t rolled;
    uint32_t rng;
    uint16_t sent;
    uint32_t sent_until;
} lab_view;

extern const view lab_view_view;

void lab_view_init(lab_view* lv, live* l, user_sounds* us, song_view* shared, const scope* sc);
void lab_view_page_set(lab_view* lv, uint32_t page);
uint32_t lab_view_panel_page(const lab_view* lv);
void lab_view_flash(lab_view* lv, uint32_t k, uint32_t now_ms);
uint32_t lab_view_name_keys(lab_view* lv, uint32_t pressed);
void lab_view_buttons(lab_view* lv, screen* s, uint32_t pressed, uint32_t held, int top);
void lab_view_lone_released(lab_view* lv, uint32_t held);
int lab_view_home(lab_view* lv);
void lab_view_save(lab_view* lv, screen* s, int edit);
void lab_view_kept(const lab_view* lv, uint32_t slot, int gaps, screen* s);
#define LAB_VIEW_SENT_MS 6000u
void lab_view_sent(lab_view* lv, uint32_t notes, uint32_t now);
void lab_view_pull(lab_view* lv, screen* s, const uint8_t sound[LAB_SOUND_BYTES], uint32_t instr,
                   uint32_t notes);
const char* lab_view_hint_text(const lab_view* lv, uint32_t now, char* out);

const char* lab_view_page_name(uint32_t page);
int lab_view_card(const lab_view* lv, uint32_t page, uint32_t k, char* name, char* value,
                  uint32_t* fill, uint32_t* max);
void lab_view_choice(const lab_view* lv, uint32_t page, char* out);
const char* lab_view_hint(uint32_t n);
uint8_t lab_view_wave_control(uint32_t i);
uint32_t lab_view_wave_of(uint8_t control);

#ifdef __cplusplus
}
#endif

#endif
