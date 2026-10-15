// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "widget.h"

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
    m->rows = (uint8_t)(rows ? rows : 1u);
    m->cursor = 0;
    m->top = 0;
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

void menu_draw(const menu* m, grid* g, uint32_t col, uint32_t row, uint32_t w, uint8_t attr,
               int cursor_on) {
    if (w < 2u) {
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
