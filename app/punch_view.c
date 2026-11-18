// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "punch_view.h"

#include <stddef.h>

#include "theme.h"

PUNCH_VIEW_STATE punch_view punch_view_state;

#define BIG 2u
#define PAD_COL 2u

static const char* const PAD_NAMES[SONG_T_PUNCH_PADS] = {"MUTE",    "FILTER SLAM", "PITCH DIVE",
                                                         "STUTTER", "ECHO THROW",  "CHIP SWAP"};

const char* punch_view_pad_name(uint32_t pad) {
    return pad < SONG_T_PUNCH_PADS ? PAD_NAMES[pad] : "";
}

void punch_view_init(punch_view* pv, song_view* sv) {
    pv->sv = sv;
    pv->held = 0;
    pv->sent = 0;
    pv->playing = 0;
    pv->built = 0;
}

void punch_view_keys(punch_view* pv, uint32_t down) {
    pv->held = (uint8_t)(down & PUNCH_VIEW_ALL);
}

static uint32_t beat_calls(const song_view* sv, const song_status* st) {
    uint32_t rows = sv->rows_beat ? sv->rows_beat : SONG_VIEW_ROWS_A_BEAT;
    return rows * ((uint32_t)st->speed[0] + st->speed[1]) / 2u;
}

void punch_view_frame(punch_view* pv, int shows) {
    const song_transport* t = pv->sv->t;
    if (!shows) {
        pv->held = 0;
    }
    if (!t || !t->punch || !(shows || pv->sent)) {
        return;
    }
    song_status st;
    t->status(t->ctx, &st);
    pv->playing = st.state == SONG_T_PLAYING;
    uint8_t want = pv->playing ? pv->held : 0u;
    if (want != pv->sent && t->punch(t->ctx, want, beat_calls(pv->sv, &st))) {
        pv->sent = want;
    }
}

uint32_t punch_view_lit(const punch_view* pv) {
    return pv->sent;
}

static uint8_t pad_attr(const theme* th, int lit, int playing, int digit) {
    if (lit) {
        return GRID_ATTR(th->bg, th->hot);
    }
    if (!playing) {
        return THEME_ATTR(th, dim);
    }
    return digit ? THEME_ATTR(th, hot) : THEME_ATTR(th, value);
}

static uint32_t pad_cells(uint32_t pad) {
    return BIG * BIG * (2u + grid_text_length(PAD_NAMES[pad]));
}

static void draw_pad(grid* g, const theme* th, uint32_t pad, int lit, int playing) {
    uint32_t row = PUNCH_VIEW_PAD_ROW + PUNCH_VIEW_PAD_STEP * pad;
    const char digit[3] = {(char)('1' + pad), ' ', 0};
    grid_text_big(g, PAD_COL, row, BIG, digit, pad_attr(th, lit, playing, 1));
    grid_text_big(g, PAD_COL + 2u * BIG, row, BIG, PAD_NAMES[pad], pad_attr(th, lit, playing, 0));
}

static uint8_t dim_target(const punch_view* pv) {
    return pv->playing ? 0u : (uint8_t)PUNCH_VIEW_ALL;
}

static void catch_up(punch_view* pv) {
    uint8_t lit = (uint8_t)punch_view_lit(pv);
    uint8_t dim = dim_target(pv);
    uint32_t cells = 0;
    if (pv->drawn_playing != pv->playing) {
        cells = BIG * BIG * grid_text_length("PLAY TO PUNCH");
        pv->drawn_playing = pv->playing;
    }
    for (uint32_t pad = 0; pad < SONG_T_PUNCH_PADS; pad++) {
        uint32_t bit = 1u << pad;
        if (!(((pv->drawn_pads ^ lit) | (pv->drawn_dim ^ dim)) & bit)) {
            continue;
        }
        cells += pad_cells(pad);
        if (cells > PUNCH_VIEW_FRAME_CELLS) {
            return;
        }
        pv->drawn_pads = (uint8_t)((pv->drawn_pads & ~bit) | (lit & bit));
        pv->drawn_dim = (uint8_t)((pv->drawn_dim & ~bit) | (dim & bit));
    }
}

static void compose(screen* s, void* state, grid* g) {
    (void)s;
    punch_view* pv = state;
    const theme* th = theme_get(pv->sv->l->theme);
    grid_fill(g, 0, 0, GRID_COLS, GRID_ROWS, ' ', THEME_ATTR(th, value));
    if (!pv->built) {
        pv->built = 1;
        pv->drawn_playing = pv->playing;
        pv->drawn_pads = 0;
        pv->drawn_dim = dim_target(pv);
    } else {
        catch_up(pv);
    }
    pv->drawn_theme = pv->sv->l->theme;
    grid_text_big(g, PAD_COL, PUNCH_VIEW_TITLE_ROW, BIG, "PUNCH", THEME_ATTR(th, hot));
    grid_text(g, PAD_COL, PUNCH_VIEW_HOLD_ROW, "HOLD 1-6, SEVERAL AT ONCE", THEME_ATTR(th, label));
    for (uint32_t pad = 0; pad < SONG_T_PUNCH_PADS; pad++) {
        draw_pad(g, th, pad, (int)(pv->drawn_pads >> pad & 1u), !(pv->drawn_dim >> pad & 1u));
    }
    int playing = pv->drawn_playing;
    grid_text_big(g, PAD_COL, PUNCH_VIEW_HINT_ROW, BIG, playing ? "KEYS ARE PADS" : "PLAY TO PUNCH",
                  playing ? THEME_ATTR(th, label) : THEME_ATTR(th, alert));
    grid_text(g, PAD_COL, PUNCH_VIEW_BACK_ROW, "HOME BACK", THEME_ATTR(th, hint));
}

static int changed(screen* s, void* state) {
    (void)s;
    const punch_view* pv = state;
    return !pv->built || pv->drawn_theme != pv->sv->l->theme ||
           pv->drawn_playing != pv->playing || pv->drawn_pads != (uint8_t)punch_view_lit(pv) ||
           pv->drawn_dim != dim_target(pv);
}

static void enter(screen* s, void* state) {
    (void)s;
    punch_view* pv = state;
    pv->built = 0;
    pv->held = 0;
}

static void leave(screen* s, void* state) {
    (void)s;
    punch_view* pv = state;
    pv->held = 0;
}

const view punch_view_view = {"PUNCH", enter, leave, NULL, compose, changed};
