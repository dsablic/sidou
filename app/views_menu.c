// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

static int menu_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint8_t was = v->home.cursor;
        menu_move(&v->home, e->steps);
        if (v->home.cursor != was) {
            s->popup_defer = 1;
            s->layers = 1;
        }
        v->format_arm = 0;
        views_sound(v, v->home.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        v->menu_ask = v->home.cursor;
        const menu_item* it = &v->home.items[v->home.cursor];
        int does = it->turn && (it->value || e->steps > 0);
        menu_turn(&v->home, e->steps);
        views_sound(v, does ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
        return 1;
    }
    return 0;
}

static int menu_changed(screen* s, void* state) {
    (void)s;
    const views* v = state;
    return v->home.pending || views_looks_changed(v, 0);
}

void views_big_menu_compose(screen* s, views* v, menu* m, const char* title, grid* g,
                             uint32_t parts) {
    const theme* th = theme_get(v->l->theme);
    const uint8_t text = THEME_ATTR(th, hint);
    const uint8_t blank = GRID_ATTR(th->menu_item, th->bg);
    const uint8_t head_attr = GRID_ATTR(th->title_fg, th->title_bg);
    m->sel_attr = GRID_ATTR(th->sel_fg, th->sel_bg);
    m->value_attr = GRID_ATTR(th->menu_value, th->bg);
    if (parts & MENU_PART_BLANK) {
        grid_fill(g, 0, 0, GRID_COLS, MENU_ROW, ' ', blank);
        grid_fill(g, 0, MENU_ROW + 3u * m->rows, GRID_COLS,
                  MENU_HELP_ROW - MENU_ROW - 3u * m->rows, ' ', blank);
        grid_fill(g, 0, MENU_HELP_ROW, 1, 4, ' ', blank);
        grid_fill(g, 25, MENU_HELP_ROW, GRID_COLS - 25u, 4, ' ', blank);
        grid_fill(g, 0, MENU_HELP_ROW + 4u, GRID_COLS, GRID_ROWS - MENU_HELP_ROW - 4u, ' ', blank);
    }
    if (parts & MENU_PART_TITLE) {
        char head[MENU_CHARS + 1u];
        char pos[8];
        char num[4];
        views_number_text(num, m->cursor + 1u);
        views_join(pos, sizeof pos, num, "/");
        views_number_text(num, m->count);
        views_append(pos, sizeof pos, num);
        views_join(head, sizeof head, title, "");
        uint32_t n = grid_text_length(head);
        while (n < sizeof head - 1u - grid_text_length(pos)) {
            head[n++] = ' ';
        }
        head[n] = 0;
        views_append(head, sizeof head, pos);
        uint32_t at = grid_text_length(title) > 6u ? grid_text_length(title) : 6u;
        if (at < n) {
            head[at] = m->top > 0 ? (char)FONT_ARROW_UP : ' ';
        }
        if (at + 1u < n) {
            head[at + 1u] = m->top + m->rows < m->count ? (char)FONT_ARROW_DOWN : ' ';
        }
        grid_fill(g, 0, 0, GRID_COLS, 3, ' ', head_attr);
        grid_text_big(g, 1, 1, 2, head, head_attr);
    }
    uint32_t layers = screen_layer_cells(s);
    if (s->popup && s->popup->rows) {
        layers += MENU_LINE_CELLS;
    }
    m->budget = (uint16_t)(MENU_FRAME_CELLS > layers ? MENU_FRAME_CELLS - layers : 1u);
    m->force_rows = s->redraw ? s->redraw_rows : 0u;
    if (m->lines) {
        m->lines = layers || s->redraw ? 1u : MIDI_LINES;
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            m->force_rows = 0;
        }
        m->force_rows |= MENU_BUDGET_FORCED;
    } else {
        m->force_rows |= layers || s->redraw_rows != (1u << GRID_ROWS) - 1u ? MENU_BUDGET_FORCED : 0u;
    }
    if (parts & MENU_PART_LINES) {
        menu_draw(m, g, 0, MENU_ROW, GRID_COLS, blank, 0);
    } else {
        m->pending = 1;
    }
    if (parts & MENU_PART_HELP) {
        grid_text_big(g, 1, MENU_HELP_ROW, 2, "SELECT  MOVE", text);
        grid_text_big(g, 1, MENU_HELP_ROW + 2u, 2, m == &v->home ? "SAVE    OPEN" : "ALG   CHANGE",
                      text);
    }
}

static void menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    views_look(v, 0, v->drawn);
    views_big_menu_compose(s, v, &v->home, "MENU", g,
                     MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP | MENU_PART_LINES);
}

const view views_menu = {"MENU", NULL, NULL, menu_input, menu_compose, menu_changed};
