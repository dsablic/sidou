// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_VIEW_H
#define SIDOU_SID_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "screen.h"
#include "sid_transport.h"
#include "song_view.h"
#include "store.h"
#include "theme.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_VIEW_FILES 8u
#define SID_VIEW_NAME_CELLS 14u
#define SID_VIEW_LOOK 28u
#define SID_VIEW_STATUS_LOOK 16u
#define SID_VIEW_BAR_CHARS 7u
#define SID_VIEW_SAVE_AS 2

typedef struct {
    const sid_transport* t;
    song_view* sv;
    const store* st;
    uint8_t cursor, page, count, scanning, skip, built, drawn_theme;
    uint8_t asking;
    uint8_t saving;
    uint8_t mark;
    uint8_t flags;
    uint8_t zones;
    char after[STORE_NAME_MAX + 1u];
    char files[SID_VIEW_FILES][STORE_NAME_MAX + 1u];
    char chosen[STORE_NAME_MAX + 1u];
    uint8_t drawn[SID_VIEW_LOOK];
    uint8_t offer_built;
    uint8_t offer_drawn[4];
} sid_view;

extern const view sid_view_browser;
extern const view sid_view_playing;
extern const view sid_view_offer;

void sid_view_init(sid_view* sd, song_view* sv);
void sid_view_set_transport(sid_view* sd, const sid_transport* t);
void sid_view_open(sid_view* sd, screen* s);
int sid_view_is(const view* v);
uint32_t sid_view_state(const sid_view* sd);
void sid_view_rec(sid_view* sd, screen* s);
int sid_view_offered(const sid_view* sd);
void sid_view_offer_answer(sid_view* sd, screen* s, int go);
int sid_view_play(sid_view* sd, int edit, int here);
int sid_view_home(sid_view* sd);
int sid_view_save(sid_view* sd, screen* s);
void sid_view_frame(sid_view* sd, screen* s);
#define STATUS_LINES 8u
#define STATUS_LINE_ROWS 3u
int sid_view_status_look(const sid_view* sd, uint8_t drawn[SID_VIEW_STATUS_LOOK], int update);
void sid_view_status_draw(const sid_view* sd, grid* g, const theme* th, uint32_t row);

#ifdef __cplusplus
}
#endif

#endif
