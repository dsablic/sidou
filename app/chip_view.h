// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CHIP_VIEW_H
#define SIDOU_CHIP_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "keys_route.h"
#include "live.h"
#include "screen.h"
#include "song_view.h"
#include "user_profile.h"

#ifdef __cplusplus
extern "C" {
#endif

#define CHIP_VIEW_FRAME_CELLS 200u
#define CHIP_VIEW_CHIP 0u
#define CHIP_VIEW_SECTIONS (1u + LIVE_SLOTS)
#define CHIP_VIEW_EXPERT CHIP_VIEW_SECTIONS
#define CHIP_VIEW_EXPERT_PAGES ((SID_EXPERT_COUNT + 3u) / 4u)
#define CHIP_VIEW_SECTIONS_SHOWN LIVE_SLOTS
#define CHIP_VIEW_TABS 0xffu
#define CHIP_VIEW_TAB_ROW 2u
#define CHIP_VIEW_CHOICE_ROW 4u
#define CHIP_VIEW_NAMES_ROW 7u
#define CHIP_VIEW_LABEL_ROW 9u
#define CHIP_VIEW_VALUE_ROW 10u
#define CHIP_VIEW_BAR_ROW 13u
#define CHIP_VIEW_TAG_ROW 14u
#define CHIP_VIEW_SUMMARY_ROW 16u
#define CHIP_VIEW_NOTE_ROW 27u
#define CHIP_VIEW_LABEL 6u
#define CHIP_VIEW_LOOK (2u + (LIVE_CHAR_FIELDS - 1u) + LIVE_SLOTS * LIVE_SLOT_FIELDS + 8u)
#define CHIP_VIEW_MSG 24u

typedef grid_canvas chip_view_canvas;

typedef struct {
    live* l;
    keys_route* kr;
    uint8_t chip;
    uint8_t tabs;
    uint8_t tab;
    uint8_t expert;
    uint8_t expert_page;
    uint8_t built;
    uint8_t copying;
    uint16_t cursor;
    uint32_t held;
    uint8_t drawn[CHIP_VIEW_LOOK];
    song_view* canvas;
    user_profiles* users;
    uint8_t target;
    uint8_t confirm;
    char msg[CHIP_VIEW_MSG];
    uint8_t hl;
    uint32_t hl_until;
} chip_view;

extern const view chip_view_view;

void chip_view_init(chip_view* cv, live* l, song_view* shared);
void chip_view_set_keys(chip_view* cv, keys_route* kr);
uint32_t chip_view_focus(const chip_view* cv);
uint32_t chip_view_page(const chip_view* cv);
void chip_view_set_users(chip_view* cv, user_profiles* users);
typedef enum {
    CHIP_VIEW_SAVE_NONE = 0,
    CHIP_VIEW_SAVE_PENDING = 1,
    CHIP_VIEW_SAVE_MSG = 2
} chip_view_save_result;
chip_view_save_result chip_view_save(chip_view* cv, uint32_t now_ms);
void chip_view_flash(chip_view* cv, uint32_t k, uint32_t now_ms);
int chip_view_user(const chip_view* cv);
#define CHIP_VIEW_TARGET(cv) ((cv)->target)
int chip_view_song_tab(const chip_view* cv);
int chip_view_label(uint32_t slot, uint32_t alg, uint32_t field, char* buf);
const char* chip_view_expert_name(uint32_t index);
void chip_view_expert_text(uint32_t index, uint32_t value, char* out);
void chip_view_expert_setup_text(const chip_setup* setup, uint32_t index, char* out);
uint32_t chip_view_expert_value(const chip_view* cv, uint32_t index);

#ifdef __cplusplus
}
#endif

#endif
