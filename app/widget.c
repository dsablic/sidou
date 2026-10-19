// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "widget.h"

_Static_assert(GRID_ROWS < 32u, "the menu's budget flag is not a grid row");

#include "font.h"

void widget_frame(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
                  const char* title) {
    grid_box(g, col, row, w, h, attr, 0);
    if (!title || w < 3u) {
        return;
    }
    uint32_t end = col + w - 1u; /* the other corner */
    uint32_t c = col + 1u;
    grid_put(g, c++, row, ' ', attr);
    while (*title && c < end) {
        grid_put(g, c++, row, (uint8_t)*title++, attr);
    }
    if (c < end) {
        grid_put(g, c, row, ' ', attr);
    }
}

void widget_screen(grid* g, uint8_t ch, uint8_t attr) {
    const uint32_t last = GRID_COLS - 1u;
    grid_fill(g, 0, 0, GRID_COLS, 1, ch, attr);
    grid_fill(g, 0, last, GRID_COLS, 1, ch, attr);
    grid_fill(g, 0, 1, 1, last - 1u, ch, attr);
    grid_fill(g, last, 1, 1, last - 1u, ch, attr);
    grid_fill(g, 1, 1, last - 1u, last - 1u, ' ', attr);
}

void widget_number(grid* g, uint32_t col, uint32_t row, uint32_t width, int32_t value, int sign,
                   uint8_t attr) {
    char buf[12];
    uint32_t n = 0;
    uint32_t v = value < 0 ? 0u - (uint32_t)value : (uint32_t)value;
    do {
        buf[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v && n < 10u);
    if (sign || value < 0) {
        buf[n++] = value < 0 ? '-' : '+';
    }
    for (uint32_t i = 0; i < width; i++) {
        grid_put(g, col + width - 1u - i, row, i < n ? (uint8_t)buf[i] : (uint8_t)' ', attr);
    }
}

void widget_bar(grid* g, uint32_t col, uint32_t row, uint32_t cells, uint32_t value, uint32_t max,
                uint8_t attr) {
    uint32_t px = max ? (value > max ? max : value) * (cells * 8u) / max : 0u;
    for (uint32_t i = 0; i < cells; i++) {
        uint32_t fill = px >= 8u ? 8u : px;
        grid_put(g, col + i, row, (uint8_t)(FONT_BAR0 + fill), attr);
        px -= fill;
    }
}

void menu_init(menu* m, const menu_item* items, uint32_t count, uint32_t rows, void* ctx) {
    m->items = items;
    m->ctx = ctx;
    m->count = (uint8_t)count;
    m->rows = (uint8_t)(rows ? (rows < MENU_MAX_ROWS ? rows : MENU_MAX_ROWS) : 1u);
    m->cursor = 0;
    m->top = 0;
    m->scale = 1;
    m->sel_attr = 0;
    m->budget = 0;
    m->lines = 0;
    m->pending = 0;
    m->valid = 0;
    m->barred = 0;
    m->force_rows = 0;
}

void menu_move(menu* m, int32_t steps) {
    if (!m->count) {
        return;
    }
    int32_t c = (int32_t)m->cursor + steps;
    int32_t last = (int32_t)m->count - 1;
    m->cursor = (uint8_t)(c < 0 ? 0 : c > last ? last : c);
    if (m->cursor < m->top) {
        m->top = m->cursor;
    } else if (m->cursor >= m->top + m->rows) {
        m->top = (uint8_t)(m->cursor + 1u - m->rows);
    }
}

void menu_turn(menu* m, int32_t steps) {
    if (!m->count || !steps) {
        return;
    }
    const menu_item* it = &m->items[m->cursor];
    if (it->turn && (it->value || steps > 0)) {
        it->turn(m->ctx, steps);
    }
}

/* Line k of the menu at scale 2: n characters in text, the colour into *attr. */
static void big_line(const menu* m, uint32_t k, uint32_t n, char* line, uint8_t base,
                     uint8_t* attr, uint32_t* value_at) {
    uint32_t i = m->top + k;
    for (uint32_t c = 0; c < n; c++) {
        line[c] = ' ';
    }
    line[n] = 0;
    *attr = base;
    *value_at = n;
    if (i >= m->count) {
        return;
    }
    const menu_item* it = &m->items[i];
    if (i == m->cursor && m->sel_attr) {
        *attr = m->sel_attr;
    }
    uint32_t c = 0;
    for (const char* s = it->label; *s && c < n; s++) {
        line[c++] = *s;
    }
    if (!it->value) {
        line[n - 1u] = (char)FONT_ARROW_RIGHT;
        return;
    }
    char buf[MENU_VALUE_CHARS];
    buf[0] = 0;
    it->value(m->ctx, buf);
    buf[MENU_VALUE_CHARS - 1u] = 0;
    uint32_t len = 0;
    while (buf[len]) {
        len++;
    }
    uint32_t v = len + 1u < n - c ? n - len : c + 1u;
    *value_at = v;
    for (uint32_t j = 0; buf[j] && v < n; j++) {
        line[v++] = buf[j];
    }
}

/* The menu at scale 2: each item a line of (w - 2) / 2 characters in large text, a cell in from
 * each side (a panel's bezel hides the edge): the label, the value right-aligned or the
 * action's arrow, the selected one in sel_attr (its whole line, the margins too). Lines are
 * drawn as menu.budget and menu.force_rows say. */
void widget_blank_pieces(grid* g, uint32_t col, uint32_t row, uint32_t w) {
    for (uint32_t y = row; y < row + 2u && y < GRID_ROWS; y++) {
        for (uint32_t x = col; x < col + w && x < GRID_COLS; x++) {
            uint32_t i = y * GRID_COLS + x;
            uint32_t piece = (uint32_t)g->ch[i] - GRID_BIG_FIRST;
            if (piece >= 4u) {
                continue;
            }
            const uint8_t* glyph = font_glyphs[g->src[i]] + (piece >> 1) * 4u;
            uint32_t bits = glyph[0] | glyph[1] | glyph[2] | glyph[3];
            if (!(bits & (piece & 1u ? 0x0fu : 0xf0u))) {
                grid_overlay_clear(g, x, y);
                grid_put(g, x, y, ' ', g->attr[i]);
            }
        }
    }
}

__attribute__((noinline, unused)) static void draw_big(menu* m, grid* g, uint32_t col,
                                                    uint32_t row, uint32_t w, uint8_t attr) {
    uint32_t n = (w - 2u) / 2u; /* characters a line */
    char line[GRID_COLS / 2u + 1u];
    if (n >= sizeof line) {
        n = sizeof line - 1u;
    }
    uint32_t drawn = 0;
    uint32_t done = 0;
    m->pending = 0;
    /* The selected line makes progress first; forced restorations then share the remaining
     * quota with ordinary changes. */
    uint32_t sel = m->cursor >= m->top ? (uint32_t)m->cursor - m->top : 0u;
    for (uint32_t pass = 0; pass < 3u; pass++) {
        for (uint32_t j = 0; j < m->rows; j++) {
            uint32_t k = j == 0 ? sel : j <= sel ? j - 1u : j;
            if (done >> k & 1u) {
                continue;
            }
            uint32_t r = row + 3u * k;
            uint32_t guarded = g->big_guard >> r & 3u;
            int forced = (m->force_rows >> r & 3u) != 0u;
            int urgent = k == sel || (m->barred >> k & 1u);
            uint32_t priority = urgent ? 0u : forced ? 1u : 2u;
            if (priority != pass) {
                continue;
            }
            uint8_t a;
            uint32_t value_at;
            big_line(m, k, n, line, attr, &a, &value_at);
            if (!forced && (m->valid >> k & 1u) && g->attr[r * GRID_COLS + col] == a) {
                uint32_t c = 0;
                for (; c < n; c++) {
                    uint32_t i = (r + (guarded & 1u)) * GRID_COLS + col + 1u + 2u * c;
                    uint8_t want = m->value_attr && a == attr && c >= value_at ? m->value_attr : a;
                    if (g->src[i] != (uint8_t)line[c] || g->attr[i] != want) {
                        break;
                    }
                }
                if (c == n) {
                    continue;
                }
            }
            if (!urgent && drawn &&
                (m->lines ? drawn >= m->lines
                          : ((!forced || (m->force_rows & MENU_BUDGET_FORCED)) && m->budget &&
                             grid_pending(g) + MENU_LINE_CELLS > m->budget))) {
                m->pending = 1; /* the next call */
                m->valid = (uint8_t)(m->valid & ~(1u << k));
                if (forced) {
                    for (uint32_t y = r; y < r + 3u && y < GRID_ROWS; y++) {
                        if (m->force_rows >> y & 1u) {
                            grid_row(g, y, &g->shown_ch[y * GRID_COLS], &g->shown_attr[y * GRID_COLS]);
                        }
                    }
                }
                continue;
            }
            drawn++;
            done |= 1u << k;
            if (m->sel_attr && a == m->sel_attr) {
                m->barred = (uint8_t)(m->barred | 1u << k);
            } else {
                m->barred = (uint8_t)(m->barred & ~(1u << k));
            }
            m->valid = (uint8_t)(m->valid | 1u << k);
            grid_fill(g, col, r, w, 3, ' ', a); /* the margins, a selected line's bar, the gap row */
            grid_fill(g, col, r + 2u, w, 1, ' ', attr);
            if (m->value_attr && a == attr && value_at < n) {
                /* The label and the gap, then the value in its own colour: no cell twice. */
                char cut = line[value_at];
                line[value_at] = 0;
                grid_text_big(g, col + 1u, r, 2, line, a);
                line[value_at] = cut;
                grid_text_big(g, col + 1u + 2u * value_at, r, 2, line + value_at, m->value_attr);
            } else {
                grid_text_big(g, col + 1u, r, 2, line, a);
            }
            if (m->lines) {
                widget_blank_pieces(g, col, r, w);
            }
        }
    }
}

void menu_draw(menu* m, grid* g, uint32_t col, uint32_t row, uint32_t w, uint8_t attr,
               int cursor_on) {
    if (w < 2u) {
        return;
    }
    if (m->scale == 2u) {
        draw_big(m, g, col, row, w, attr);
        return;
    }
    uint32_t last = col + w - 1u;
    for (uint32_t k = 0; k < m->rows; k++) {
        uint32_t r = row + k;
        uint32_t i = m->top + k;
        grid_fill(g, col, r, w, 1, ' ', attr);
        if (i >= m->count) {
            continue;
        }
        const menu_item* it = &m->items[i];
        if (i == m->cursor && cursor_on) {
            grid_put(g, col, r, FONT_FULL, attr);
        }
        uint32_t c = col + 1u;
        for (const char* s = it->label; *s && c < last; s++) {
            grid_put(g, c++, r, (uint8_t)*s, attr);
        }
        if (!it->value) {
            grid_put(g, last, r, FONT_ARROW_RIGHT, attr);
            continue;
        }
        char buf[MENU_VALUE_CHARS];
        buf[0] = 0;
        it->value(m->ctx, buf);
        buf[MENU_VALUE_CHARS - 1u] = 0;
        uint32_t n = 0;
        while (buf[n]) {
            n++;
        }
        uint32_t v = n < w - 1u ? last + 1u - n : col + 1u;
        for (uint32_t j = 0; buf[j] && v <= last; j++) {
            grid_put(g, v++, r, (uint8_t)buf[j], attr);
        }
    }
}
