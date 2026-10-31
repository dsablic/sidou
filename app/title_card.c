// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "title_card.h"

#include <stddef.h>

#include "font.h"

const char* const title_card_titles[TITLE_CARDS] = {"OSCILLATOR", "SCENE", "CHIP"};

static const uint8_t STOPS[4] = {GRID_BLACK, GRID_BLUE, GRID_PURPLE, GRID_RED};

_Static_assert(FONT_D0 + 16u == FONT_D16, "the dither glyphs D0..D16 are consecutive codes");
_Static_assert(TITLE_CARD_GLOW_HALF * 2u == GRID_COLS && GRID_COLS == GRID_ROWS,
               "the glow table is a quarter of the screen");
_Static_assert(GRID_COLS % TITLE_CARD_WIPE_COLS == 0u, "the wipe ends on the screen's edge");
_Static_assert(TITLE_CARD_WIPE_COLS * GRID_ROWS <= TITLE_CARD_FRAME_CELLS,
               "a wipe step fits a frame");

void title_card_glow_cell(uint32_t col, uint32_t row, uint32_t level, uint8_t* ch,
                          uint8_t* attr) {
    uint32_t qc = col < TITLE_CARD_GLOW_HALF ? col : GRID_COLS - 1u - col;
    uint32_t qr = row < TITLE_CARD_GLOW_HALF ? row : GRID_ROWS - 1u - row;
    uint32_t l = title_card_glow[qr][qc] * level / TITLE_CARD_LEVELS;
    uint32_t i = l >> 4;
    *ch = (uint8_t)(FONT_D0 + (l & 15u));
    *attr = GRID_ATTR(STOPS[i + 1u], STOPS[i]);
}

void title_card_init(title_card* c, title_card_id id, const live* l) {
    static const title_card empty;
    *c = empty;
    c->l = l;
    c->note_ons = l ? l->note_ons : 0u;
    c->id = (uint8_t)(id < TITLE_CARDS ? id : TITLE_CARD_OSCILLATOR);
    c->level = 1;
}

int title_card_done(const title_card* c) {
    return c->skip || c->phase == TITLE_CARD_DONE || (c->l && c->l->note_ons != c->note_ons);
}

int title_card_capture(void* ctx, const screen_event* e) {
    (void)e;
    title_card* c = ctx;
    c->skip = 1;
    return 1;
}

static void advance(title_card* c) {
    switch (c->phase) {
    case TITLE_CARD_GLOW:
        if (!c->settled || c->held++ < TITLE_CARD_LEVEL_HOLD) {
            return;
        }
        c->held = 0;
        if (c->level < TITLE_CARD_LEVELS) {
            c->level++;
        } else {
            c->phase = TITLE_CARD_WIPE;
            c->cols = TITLE_CARD_WIPE_COLS;
        }
        break;
    case TITLE_CARD_WIPE:
        if (!c->settled) {
            return;
        }
        if (c->cols < GRID_COLS) {
            c->cols = (uint8_t)(c->cols + TITLE_CARD_WIPE_COLS);
        } else {
            c->phase = TITLE_CARD_HOLD;
        }
        break;
    case TITLE_CARD_HOLD:
        if (++c->held >= TITLE_CARD_HOLD_FRAMES) {
            c->phase = TITLE_CARD_DONE;
        }
        return;
    default:
        return;
    }
    c->settled = 0;
    c->cursor = 0;
}

static void enter(screen* s, void* state) {
    screen_capture(s, title_card_capture, state);
}

static void compose(screen* s, void* state, grid* g) {
    title_card* c = state;
    c->frames++;
    advance(c);
    if (c->cols) {
        grid_region(g, 0, 0, c->cols, GRID_ROWS, &title_card_bitmaps[c->id], 0, 0);
    }
    if (c->settled && !s->redraw) {
        return;
    }
    const uint32_t* exact = title_card_cells[c->id];
    uint32_t used = screen_pending(s);
    uint32_t left = used < TITLE_CARD_FRAME_CELLS ? TITLE_CARD_FRAME_CELLS - used : 0u;
    uint32_t i = c->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint32_t col = i % GRID_COLS;
        uint32_t row = i / GRID_COLS;
        uint8_t ch;
        uint8_t attr;
        title_card_glow_cell(col, row, c->level, &ch, &attr);
        if (col < c->cols && (exact[row] >> col & 1u)) {
            ch = ' ';
        }
        if (g->ch[i] == ch && g->attr[i] == attr) {
            continue;
        }
        if (!left) {
            c->cursor = (uint16_t)i;
            return;
        }
        grid_put(g, col, row, ch, attr);
        left--;
    }
    c->settled = 1;
}

const view title_card_view = {"TITLE", enter, NULL, NULL, compose, NULL};
