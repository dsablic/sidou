// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LIVE_VIEW_H
#define SIDOU_LIVE_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "scope.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LIVE_VIEW_LANE_ROW 2u
#define LIVE_VIEW_LANE_ROWS 4u
#define LIVE_VIEW_SCOPE_COL 3u
#define LIVE_VIEW_SCOPE_COLS (SCOPE_W / 8u)
#define LIVE_VIEW_SCOPE_ROWS 4u
#define LIVE_VIEW_SCOPE_PX (LIVE_VIEW_SCOPE_ROWS * 8u)
#define LIVE_VIEW_AMP 13
#define LIVE_VIEW_FRAME_CELLS 200u
#define LIVE_VIEW_PAGE_ROW 14u
#define LIVE_VIEW_KNOB_ROW 15u
#define LIVE_VIEW_KNOB_VALUE_ROW 16u
#define LIVE_VIEW_KNOB_BAR_ROW 18u
#define LIVE_VIEW_KNOB_TAG_ROW 19u
#define LIVE_VIEW_TAG_ROW_END 20u
#define LIVE_VIEW_STRIP_ROW 20u
#define LIVE_VIEW_HL_PROFILE 5u
#define LIVE_VIEW_HL_ALGORITHM 6u
#define LIVE_VIEW_BAND_ROW 21u
#define LIVE_VIEW_MODE_ROW 22u
#define LIVE_VIEW_SLOT_ROW 24u
#define LIVE_VIEW_ALG_ROW 25u
#define LIVE_VIEW_METER_ROW 22u
#define LIVE_VIEW_BAND_ROWS 6u
#define LIVE_VIEW_FALL 3u
#define LIVE_VIEW_METERS (SCOPE_VOICES + 2u)
#define LIVE_VIEW_LOOK (1u + 4u + 6u * SCOPE_VOICES + 6u + 11u + 11u + LIVE_VIEW_METERS)
#define LIVE_VIEW_ARP_ROW 27u

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} live_view_canvas;

typedef struct {
    live* l;
    scope* sc;
    uint32_t since;
    uint8_t on;
    uint8_t mutes;
    uint8_t y[SCOPE_VOICES][SCOPE_W];
    uint32_t cols[SCOPE_VOICES][SCOPE_W];
    uint8_t env[SCOPE_VOICES];
    uint8_t state[SCOPE_VOICES];
    uint8_t sustain[SCOPE_VOICES];
    uint8_t meter[LIVE_VIEW_METERS];
    uint8_t color[SCOPE_VOICES];
    uint8_t drawn;
    uint8_t stale;
    uint8_t ovl_free;
    uint8_t next_voice;
    uint32_t updates;
    uint32_t held;
    uint32_t rebuilds;
    uint8_t prepared;
    uint8_t built;
    uint8_t drawn_look[LIVE_VIEW_LOOK];
    _Alignas(4) scope_point pts[SCOPE_READ_MAX];
    _Alignas(4) uint8_t pend[SCOPE_VOICES][SCOPE_W];
    _Alignas(4) live_view_canvas cv;
    uint8_t page;
    uint8_t edit;
    uint8_t arp;
    uint8_t arp_down;
    uint8_t arp_turned;
    uint8_t hl;
    uint32_t hl_until;
    uint32_t owed;
    uint8_t glo;
    uint8_t playing;
    uint8_t beat;
    uint8_t row_seen;
    uint16_t bpm;
    uint32_t beat_until;
} live_view;

extern const view live_view_view;
void live_view_buttons(live_view* lv, uint32_t held);
void live_view_arp_text(const live* l, uint32_t k, char* buf);

void live_view_init(live_view* lv, live* l, scope* sc);
void live_view_tap(live_view* lv, int on);
void live_view_flash(live_view* lv, uint32_t what, uint32_t now_ms);
void live_view_cutoff_text(const live* l, char* buf);
void live_view_fc_text(const live* l, uint32_t fc, char* buf);
void live_view_knob_text(const live* l, uint32_t k, char* buf);
uint8_t live_view_voice_control(const live* l, uint32_t v);
uint8_t live_view_wave_color(uint8_t control);
void live_view_prepare(live_view* lv);

#ifdef __cplusplus
}
#endif

#endif
