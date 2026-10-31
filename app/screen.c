// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "screen.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "panel.h"

const uint8_t screen_ramp[SCREEN_RAMP] = {GRID_WHITE, GRID_YELLOW,     GRID_LIGHT_GREEN,
                                          GRID_CYAN,  GRID_LIGHT_BLUE, GRID_PURPLE,
                                          GRID_LIGHT_RED};

#define EDGE_ATTR GRID_ATTR(GRID_WHITE, GRID_BLACK)
#define RASTER_MAX 255u
#define TOAST_SHOWING 1u
#define TOAST_WAITING 2u
#define ALL_ROWS ((1u << GRID_ROWS) - 1u)
#define RASTER_CELLS (2u * GRID_COLS + 2u * (GRID_ROWS - 2u))

static void redraw(screen* s, uint32_t mask) {
    s->redraw = 1;
    s->redraw_rows |= mask;
}

static void call(screen* s, void (*fn)(screen*, void*)) {
    if (fn) {
        s->busy = 1;
        fn(s, s->stack[s->depth - 1u].state);
        s->busy = 0;
    }
}

static void switched(screen* s, screen_wipe w) {
    if (w == SCREEN_OVER) {
        w = SCREEN_CUT;
    } else {
        grid_overlay_clear_all(s->g);
        grid_region_clear(s->g);
    }
    if (s->depth < s->capture_depth) {
        screen_capture(s, NULL, NULL);
    }
    s->wipe = (uint8_t)w;
    s->wipe_rows = 0;
    redraw(s, ALL_ROWS);
    grid_hold(s->g, w == SCREEN_CUT ? 0u : (1u << GRID_ROWS) - 1u);
}

void screen_init(screen* s, grid* g, const view* root, void* state) {
    static const screen empty;
    *s = empty;
    s->g = g;
    s->stack[0].v = root;
    s->stack[0].state = state;
    s->depth = 1;
    redraw(s, ALL_ROWS);
    call(s, root->enter);
}

static int push(screen* s, const view* v, void* state, screen_wipe w, uint32_t slots) {
    if (s->busy || s->depth >= slots) {
        return -1;
    }
    s->stack[s->depth].v = v;
    s->stack[s->depth].state = state;
    s->depth++;
    switched(s, w);
    call(s, v->enter);
    return 0;
}

int screen_push(screen* s, const view* v, void* state, screen_wipe w) {
    return push(s, v, state, w, SCREEN_DEPTH);
}

int screen_push_over(screen* s, const view* v, void* state, screen_wipe w) {
    return push(s, v, state, w, SCREEN_SLOTS);
}

int screen_pop(screen* s, screen_wipe w) {
    if (s->busy || s->depth <= 1u) {
        return -1;
    }
    call(s, s->stack[s->depth - 1u].v->exit);
    s->depth--;
    switched(s, w);
    return 0;
}

void screen_reveal(screen* s, screen_wipe w) {
    if (!s->busy) {
        switched(s, w);
    }
}

void screen_capture(screen* s, screen_capture_fn fn, void* ctx) {
    s->capture = fn;
    s->capture_ctx = fn ? ctx : NULL;
    s->capture_depth = fn ? s->depth : 0u;
}

void screen_global(screen* s, screen_capture_fn fn, void* ctx) {
    s->global = fn;
    s->global_ctx = ctx;
}

int screen_input(screen* s, const screen_event* e) {
    if (s->capture && s->capture(s->capture_ctx, e)) {
        return 1;
    }
    if (s->global && s->global(s->global_ctx, e)) {
        return 1;
    }
    const screen_entry* top = &s->stack[s->depth - 1u];
    return top->v->input ? top->v->input(s, top->state, e) : 0;
}

void screen_redraw(screen* s) {
    redraw(s, ALL_ROWS);
}

void screen_raster(screen* s, uint32_t steps) {
    uint32_t frames = steps * SCREEN_RASTER_FRAMES;
    s->raster_frames = (uint8_t)(frames < RASTER_MAX ? frames : RASTER_MAX);
    redraw(s, ALL_ROWS);
}

int screen_blink(const screen* s) {
    return (s->frame / SCREEN_BLINK_FRAMES) % 2u == 0u;
}

int screen_animating(const screen* s) {
    return s->wipe != SCREEN_CUT || s->raster_frames != 0;
}

static uint32_t step_rows(const screen* s) {
    return s->wipe == SCREEN_BANDS ? SCREEN_BAND_ROWS : SCREEN_WIPE_ROWS;
}

uint32_t screen_reach(const screen* s) {
    if (s->wipe != SCREEN_WIPE_DOWN && s->wipe != SCREEN_BANDS) {
        return GRID_ROWS;
    }
    uint32_t n = s->wipe_rows + step_rows(s);
    return n < GRID_ROWS ? n : GRID_ROWS;
}

static void raster(screen* s) {
    if (!s->raster_frames || s->wipe != SCREEN_CUT) {
        return;
    }
    grid* g = s->g;
    uint32_t phase = (s->raster_frames + SCREEN_RASTER_FRAMES - 1u) / SCREEN_RASTER_FRAMES;
    s->raster_frames--;
    redraw(s, ALL_ROWS);
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        uint32_t fg = screen_ramp[(r + phase) % SCREEN_RAMP];
        fg = fg == GRID_PURPLE ? GRID_LIGHT_GREY : fg;
        uint32_t i = r * GRID_COLS;
        uint32_t step = r == 0 || r == GRID_ROWS - 1u ? 1u : GRID_COLS - 1u;
        for (uint32_t c = 0; c < GRID_COLS; c += step) {
            grid_put(g, c, r, g->ch[i + c], (uint8_t)((g->attr[i + c] & 0xF0u) | fg));
        }
    }
}

static void wipe(screen* s) {
    if (s->wipe == SCREEN_CUT) {
        return;
    }
    uint32_t n = s->wipe_rows + step_rows(s);
    if (n >= GRID_ROWS) {
        s->wipe = SCREEN_CUT;
        grid_hold(s->g, 0);
        return;
    }
    s->wipe_rows = (uint8_t)n;
    if (s->wipe == SCREEN_BANDS) {
        grid_hold(s->g, ~((1u << n) - 1u));
        redraw(s, ALL_ROWS);
        return;
    }
    uint32_t edge;
    uint32_t hold;
    if (s->wipe == SCREEN_WIPE_DOWN) {
        edge = n;
        hold = ~((2u << edge) - 1u);
    } else {
        edge = GRID_ROWS - 1u - n;
        hold = (1u << edge) - 1u;
    }
    grid_fill(s->g, 0, edge, GRID_COLS, 1, FONT_FULL, EDGE_ATTR);
    grid_hold(s->g, hold);
    redraw(s, ALL_ROWS);
}

static uint32_t rows_mask(uint32_t row, uint32_t n) {
    return ((1u << n) - 1u) << row;
}

static uint32_t toast_row(const screen* s) {
    return SCREEN_TOAST_FIRST_ROW(s);
}

static void toast_layout(screen* s) {
    uint32_t n = 0;
    while (s->toast[n]) {
        n++;
    }
    uint32_t brk = SCREEN_TOAST_LINE;
    while (brk && s->toast[brk] != ' ') {
        brk--;
    }
    if (!brk || n - brk - 1u > SCREEN_TOAST_LINE) {
        s->toast_h = 1;
        s->toast_w = (uint8_t)n;
        return;
    }
    uint32_t second = n - brk - 1u;
    s->toast_brk = (uint8_t)brk;
    s->toast_w = (uint8_t)(2u * (brk > second ? brk : second));
}

void screen_toast(screen* s, const char* text, uint32_t ms) {
    uint32_t n = 0;
    for (; text[n] && n < SCREEN_TOAST_CHARS; n++) {
        s->toast[n] = text[n];
    }
    s->toast[n] = 0;
    if (n <= SCREEN_TOAST_LINE) {
        s->toast_h = 2;
        s->toast_w = (uint8_t)(2u * n);
    } else {
        s->toast_h = 4;
        s->toast_w = SCREEN_TOAST_CHARS;
        s->toast_brk = 0;
    }
    s->toast_on = TOAST_WAITING;
    s->toast_until = ms;
    s->layers = 1;
    redraw(s, rows_mask(SCREEN_TOAST_ROW - 2u, 4u));
}

uint32_t screen_toast_take(screen* s) {
    uint32_t left = s->toast_on == TOAST_WAITING ? s->toast_until
                    : s->toast_on && (int32_t)(s->toast_until - s->now) > 0 ? s->toast_until - s->now
                                                                            : 0u;
    if (s->toast_on) {
        s->toast_on = 0;
        redraw(s, rows_mask(SCREEN_TOAST_ROW - 2u, 4u));
    }
    return left;
}

static void toast(screen* s) {
    if (s->toast_h == 4u && !s->toast_brk) {
        toast_layout(s);
    }
    if (s->toast_on == TOAST_WAITING) {
        s->toast_on = TOAST_SHOWING;
        s->toast_until += s->now;
    }
    if (s->toast_on && (int32_t)(s->now - s->toast_until) >= 0) {
        s->toast_on = 0;
    }
    grid* g = s->g;
    if (!s->toast_on) {
        return;
    }
    uint32_t row = toast_row(s);
    const theme* th = screen_theme(s);
    uint8_t ta = GRID_ATTR(th->toast_fg, th->toast_bg);
    if (s->toast_h == 1u) {
        grid_text(g, SCREEN_TOAST_COL, SCREEN_TOAST_ROW, s->toast, ta);
    } else if (s->toast_h == 2u) {
        grid_text_big(g, SCREEN_TOAST_COL, row, 2, s->toast, ta);
    } else {
        char line[SCREEN_TOAST_LINE + 1u];
        uint32_t brk = s->toast_brk;
        for (uint32_t i = 0; i < brk; i++) {
            line[i] = s->toast[i];
        }
        line[brk] = 0;
        grid_text_big(g, SCREEN_TOAST_COL, row, 2, line, ta);
        grid_text_big(g, SCREEN_TOAST_COL, row + 2u, 2, s->toast + brk + 1u, ta);
    }
    redraw(s, rows_mask(row, s->toast_h));
}

#define POPUP_ROLE(th, role, accent) ((th)->role == THEME_ACCENT ? (accent) : (th)->role)
#define POPUP_BODY(th) GRID_ATTR((th)->value, (th)->popup_bg)
#define POPUP_NAME_ROW 1u
#define POPUP_VALUE_ROW 4u
#define POPUP_BAR_ROW 7u
#define POPUP_COL (SCREEN_POPUP_COL + 2u)
_Static_assert(SCREEN_POPUP_COL + SCREEN_POPUP_W + 1u <= GRID_COLS &&
                   SCREEN_POPUP_ROW + SCREEN_POPUP_ROWS <= SCREEN_TOAST_ROW - 2u,
               "the popup is inside the border and above the toast");
_Static_assert(SCREEN_POPUP_BIG * 3u <= SCREEN_POPUP_W - 4u &&
                   SCREEN_POPUP_CHARS * 2u <= SCREEN_POPUP_W - 4u &&
                   SCREEN_POPUP_BAR_CELLS == SCREEN_POPUP_W - 4u,
               "the text and the bar fit the box, a cell of margin either side");
_Static_assert(POPUP_BAR_ROW + 1u < SCREEN_POPUP_ROWS && POPUP_VALUE_ROW + 3u == POPUP_BAR_ROW,
               "the name, the value, the bar and the frame's foot");

static void copy_clipped(char* out, const char* text) {
    uint32_t n = 0;
    for (; text[n] && n < SCREEN_POPUP_CHARS; n++) {
        out[n] = text[n];
    }
    out[n] = 0;
}

void screen_popup(screen* s, const char* name, const char* value, uint32_t fill, uint32_t max) {
    screen_popup_state* p = s->popup;
    if (!p) {
        return;
    }
    copy_clipped(p->name, name);
    copy_clipped(p->value, value);
    p->has_bar = (uint8_t)(max != 0);
    p->accent = screen_theme(s)->hot;
    p->bar = (uint8_t)(max ? (fill > max ? max : fill) * (SCREEN_POPUP_BAR_CELLS * 8u) / max
                           : 0u);
    p->alive = 2;
    p->fresh = 1;
    s->layers = 1;
}

void screen_popup_source(screen* s, screen_popup_state* st, screen_popup_fn fn, void* ctx) {
    static const screen_popup_state empty;
    *st = empty;
    st->source = fn;
    st->source_ctx = ctx;
    s->popup = st;
}

void screen_theme_source(screen* s, const uint8_t* id) {
    if (s->popup) {
        s->popup->theme_id = id;
    }
}

const theme* screen_theme(const screen* s) {
    const uint8_t* id = s->popup ? s->popup->theme_id : NULL;
    return theme_get(id ? *id : THEME_HIGH_CONTRAST);
}

void screen_popup_watch(screen* s, screen_watch_fn fn, void* ctx) {
    if (s->popup) {
        s->popup->watch = fn;
        s->popup->watch_ctx = ctx;
        s->layers = 1;
    }
}

void screen_popup_ask(screen* s, uint32_t ask) {
    s->popup_ask = (uint8_t)ask;
    s->popup_view = NULL;
    s->layers = 1;
}

int screen_popup_shows(const screen* s) {
    return s->popup && (s->popup->rows != 0 || s->popup->alive != 0);
}

static void popup_cancel(screen* s) {
    s->popup_ask = 0;
    s->popup_defer = 0;
    if (s->popup) {
        s->popup->rows = 0;
        s->popup->next = 0;
        s->popup->alive = 0;
    }
    s->layers = (uint8_t)(s->toast_on != 0);
    grid_big_guard(s->g, 0);
}

__attribute__((noinline, unused)) static void popup_step(screen* s) {
    screen_popup_state* p = s->popup;
    if (!p) {
        s->popup_ask = 0;
        s->popup_defer = 0;
        s->layers = (uint8_t)(s->toast_on != 0);
        return;
    }
    int asked = 0;
    int dropped = s->toast_on == TOAST_WAITING && (p->rows | p->alive);
    if (dropped) {
        p->alive = 0;
        s->popup_defer = 0;
    }
    const view* top = s->stack[s->depth - 1u].v;
    if (s->popup_view && s->popup_view != top) {
        popup_cancel(s);
        s->popup_view = NULL;
        if (!s->layers) {
            return;
        }
    }
    if (s->popup_defer) {
        s->popup_defer = 0;
        if (p->alive) {
            p->until = s->now + SCREEN_POPUP_MS;
        }
        p->next = p->rows;
        grid_big_guard(s->g, rows_mask(SCREEN_POPUP_ROW, p->rows));
        screen_cover(s, SCREEN_POPUP_COL, SCREEN_POPUP_ROW, SCREEN_POPUP_W, p->rows, 0u);
        return;
    }
    if (p->watch) {
        p->watch(p->watch_ctx);
    }
    if (s->popup_ask) {
        uint32_t ask = s->popup_ask;
        char name[SCREEN_POPUP_TEXT];
        char value[SCREEN_POPUP_TEXT];
        uint32_t fill = 0;
        uint32_t max = 0;
        uint32_t accent = THEME_ACCENT;
        s->popup_ask = 0;
        if (p->source && p->source(p->source_ctx, ask, name, value, &fill, &max, &accent)) {
            screen_popup(s, name, value, fill, max);
            p->accent = (uint8_t)(accent == THEME_ACCENT ? screen_theme(s)->hot : accent);
            asked = 1;
        }
    }
    if (!s->popup_view) {
        s->popup_view = top;
    }
    if (p->alive == 2u) {
        p->alive = 1;
        p->until = s->now + SCREEN_POPUP_MS;
    } else if (p->alive && (int32_t)(s->now - p->until) >= 0) {
        p->alive = 0;
    }
    uint32_t rows = p->rows;
    uint32_t next = rows;
    if (p->alive) {
        if (rows < SCREEN_POPUP_ROWS && !screen_animating(s) && !asked) {
            next = rows + SCREEN_POPUP_STEP;
            next = next < SCREEN_POPUP_ROWS ? next : SCREEN_POPUP_ROWS;
        } else if (!rows) {
            p->until = s->now + SCREEN_POPUP_MS;
        }
    } else if (rows) {
        next = dropped || rows <= SCREEN_POPUP_STEP ? 0u : rows - SCREEN_POPUP_STEP;
    }
    grid* g = s->g;
    for (uint32_t r = next; r < rows; r++) {
        grid_fill(g, SCREEN_POPUP_COL, SCREEN_POPUP_ROW + r, SCREEN_POPUP_W, 1, ' ',
                  POPUP_BODY(screen_theme(s)));
        redraw(s, 1u << (SCREEN_POPUP_ROW + r));
    }
    p->next = (uint8_t)next;
    grid_big_guard(g, next ? rows_mask(SCREEN_POPUP_ROW, next) : 0u);
    if (next) {
        uint32_t cells = p->fresh ? next * SCREEN_POPUP_W
                                 : next > rows ? SCREEN_POPUP_STEP * SCREEN_POPUP_W : 0u;
        screen_cover(s, SCREEN_POPUP_COL, SCREEN_POPUP_ROW, SCREEN_POPUP_W, next, cells);
    }
}

static uint32_t popup_scale(const char* t) {
    uint32_t n = 0;
    while (t[n]) {
        n++;
    }
    return n <= SCREEN_POPUP_BIG ? 3u : 2u;
}

static void popup_text(screen* s, uint32_t row, const char* t, uint8_t attr) {
    char pad[SCREEN_POPUP_TEXT];
    uint32_t scale = popup_scale(t);
    uint32_t width = SCREEN_POPUP_BAR_CELLS / scale;
    uint32_t n = 0;
    for (; t[n] && n < width; n++) {
        pad[n] = t[n];
    }
    for (; n < width; n++) {
        pad[n] = ' ';
    }
    pad[n] = 0;
    grid_text_big(s->g, POPUP_COL, SCREEN_POPUP_ROW + row, scale, pad, attr);
}

__attribute__((noinline, unused)) static void popup_draw(screen* s) {
    screen_popup_state* p = s->popup;
    grid* g = s->g;
    const theme* th = screen_theme(s);
    uint8_t body = POPUP_BODY(th);
    uint8_t frame = GRID_ATTR(POPUP_ROLE(th, popup_frame, p->accent), th->popup_bg);
    uint8_t name_attr = GRID_ATTR(POPUP_ROLE(th, popup_name, p->accent), th->popup_bg);
    uint8_t bar_attr = GRID_ATTR(POPUP_ROLE(th, popup_bar, p->accent), th->popup_track);
    uint32_t guard = g->big_guard;
    uint32_t last = SCREEN_POPUP_COL + SCREEN_POPUP_W - 1u;
    uint32_t rows = p->next;
    uint32_t from = s->composed || p->fresh ? 0u : p->rows;
    uint32_t ns = popup_scale(p->name);
    uint32_t vs = popup_scale(p->value);
    int name = POPUP_NAME_ROW + ns <= rows;
    int value = POPUP_VALUE_ROW + vs <= rows;
    g->big_guard = 0;
    (void)grid_big_touched(g);
    for (uint32_t r = from; r < rows; r++) {
        uint32_t y = SCREEN_POPUP_ROW + r;
        if (r == 0 || r == SCREEN_POPUP_ROWS - 1u) {
            grid_fill_top(g, SCREEN_POPUP_COL + 1u, y, SCREEN_POPUP_W - 2u, 1, FONT_BOX_H,
                          frame);
            grid_put_top(g, SCREEN_POPUP_COL, y, r == 0 ? FONT_ROUND_TL : FONT_ROUND_BL,
                         frame);
            grid_put_top(g, last, y, r == 0 ? FONT_ROUND_TR : FONT_ROUND_BR, frame);
            continue;
        }
        grid_put_top(g, SCREEN_POPUP_COL, y, FONT_BOX_V, frame);
        grid_put_top(g, last, y, FONT_BOX_V, frame);
        grid_put_top(g, SCREEN_POPUP_COL + 1u, y, ' ', body);
        grid_put_top(g, last - 1u, y, ' ', body);
        if ((name && r - POPUP_NAME_ROW < ns) || (value && r - POPUP_VALUE_ROW < vs)) {
            continue;
        }
        if (r == POPUP_BAR_ROW && p->has_bar) {
            uint32_t px = p->bar;
            for (uint32_t i = 0; i < SCREEN_POPUP_BAR_CELLS; i++) {
                uint32_t fill = px >= 8u ? 8u : px;
                grid_put_top(g, POPUP_COL + i, y, (uint8_t)(FONT_H0 + fill), bar_attr);
                px -= fill;
            }
            continue;
        }
        grid_fill_top(g, POPUP_COL, y, SCREEN_POPUP_BAR_CELLS, 1, ' ', body);
    }
    if (name && (from == 0 || from < POPUP_NAME_ROW + ns)) {
        popup_text(s, POPUP_NAME_ROW, p->name, name_attr);
    }
    if (value && (from == 0 || from < POPUP_VALUE_ROW + vs)) {
        popup_text(s, POPUP_VALUE_ROW, p->value, body);
    }
    if (!grid_big_touched(g)) {
        grid_unstale(g, SCREEN_POPUP_COL, SCREEN_POPUP_ROW, SCREEN_POPUP_W, rows);
    }
    g->big_guard = guard;
}

__attribute__((noinline, unused)) static void layers_post(screen* s) {
    screen_popup_state* p = s->popup;
    toast(s);
    if (p) {
        if (p->next && (s->composed || p->fresh || p->next > p->rows)) {
            popup_draw(s);
        }
        p->rows = p->next;
        p->fresh = 0;
    }
    s->composed = 0;
    s->layers = (uint8_t)(s->toast_on != 0 || s->popup_ask != 0 ||
                          (p && (p->alive != 0 || p->rows != 0 || p->watch != NULL)));
}

static int toast_shows(const screen* s) {
    if (s->toast_on == TOAST_WAITING) {
        return (int32_t)(0u - s->toast_until) < 0;
    }
    return s->toast_on == TOAST_SHOWING && (int32_t)(s->now - s->toast_until) < 0;
}

void screen_cover(screen* s, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint32_t cells) {
    s->cover_col = (uint8_t)(col < GRID_COLS ? col : GRID_COLS);
    s->cover_row = (uint8_t)(row < GRID_ROWS ? row : GRID_ROWS);
    s->cover_w = (uint8_t)(w < GRID_COLS - s->cover_col ? w : GRID_COLS - s->cover_col);
    s->cover_h = (uint8_t)(h < GRID_ROWS - s->cover_row ? h : GRID_ROWS - s->cover_row);
    s->cover_cells = (uint16_t)(s->cover_w && cells < GRID_CELLS ? cells : 0u);
}

static int in_cover(const screen* s, uint32_t col, uint32_t row) {
    return col - s->cover_col < s->cover_w && row - s->cover_row < s->cover_h;
}

static int in_toast(const screen* s, uint32_t col, uint32_t row) {
    return col - SCREEN_TOAST_COL < s->toast_w && row - toast_row(s) < s->toast_h &&
           toast_shows(s);
}

int screen_covered(const screen* s, uint32_t col, uint32_t row) {
    return in_cover(s, col, row) || (s->toast_on && in_toast(s, col, row));
}

uint32_t screen_pending(const screen* s) {
    uint32_t n = grid_pending(s->g) -
                 grid_pending_in(s->g, s->cover_col, s->cover_row, s->cover_w, s->cover_h);
    if (s->toast_on && toast_shows(s)) {
        uint32_t row = toast_row(s);
        uint32_t c0 = s->cover_col > SCREEN_TOAST_COL ? s->cover_col : SCREEN_TOAST_COL;
        uint32_t c1 = s->cover_col + s->cover_w < SCREEN_TOAST_COL + s->toast_w
                          ? s->cover_col + s->cover_w
                          : SCREEN_TOAST_COL + s->toast_w;
        uint32_t r0 = s->cover_row > row ? s->cover_row : row;
        uint32_t r1 = s->cover_row + s->cover_h < row + s->toast_h ? s->cover_row + s->cover_h
                                                                    : row + s->toast_h;
        n -= grid_pending_in(s->g, SCREEN_TOAST_COL, row, s->toast_w, s->toast_h);
        if (c0 < c1 && r0 < r1) {
            n += grid_pending_in(s->g, c0, r0, c1 - c0, r1 - r0);
        }
    }
    return n;
}

uint32_t screen_layer_cells(const screen* s) {
    uint32_t n = s->cover_cells;
    if (s->toast_on == TOAST_WAITING) {
        n += (uint32_t)s->toast_w * s->toast_h;
    }
    if (s->raster_frames && s->wipe == SCREEN_CUT) {
        n += RASTER_CELLS;
    }
    return n;
}

void screen_frame(screen* s, uint32_t now_ms) {
    s->now = now_ms;
    s->frame++;
    if (s->layers) {
        popup_step(s);
    }
    const screen_entry* top = &s->stack[s->depth - 1u];
    if (s->redraw || !top->v->changed || top->v->changed(s, top->state)) {
        top->v->compose(s, top->state, s->g);
        s->composed = 1;
    }
    s->redraw = 0;
    s->redraw_rows = 0;
    if (s->cover_w) {
        screen_cover(s, 0, 0, 0, 0, 0);
    }
    if (s->layers) {
        layers_post(s);
    }
    raster(s);
    wipe(s);
}

void screen_lone_wait(screen_lone* l, uint32_t button, uint32_t waiting) {
    l->button = (uint8_t)button;
    l->waiting = (uint8_t)waiting;
}

uint32_t screen_lone_chord(screen_lone* l, const screen_event* e) {
    if (l->waiting && e->kind == SCREEN_PRESS && e->id == PANEL_BTN_EDIT &&
        (e->held >> l->button & 1u)) {
        l->waiting = SCREEN_LONE_NONE;
        return l->button;
    }
    return SCREEN_LONE_NO_BUTTON;
}

uint32_t screen_lone_take(screen_lone* l) {
    uint32_t due = l->waiting == SCREEN_LONE_HELD_BACK ? l->button : SCREEN_LONE_NO_BUTTON;
    l->waiting = SCREEN_LONE_NONE;
    return due;
}

uint32_t screen_lone_released(screen_lone* l, uint32_t held) {
    return l->waiting && !(held >> l->button & 1u) ? screen_lone_take(l) : SCREEN_LONE_NO_BUTTON;
}

const char screen_note_names[12][2] = {{'C', '-'}, {'C', '#'}, {'D', '-'}, {'D', '#'},
                                       {'E', '-'}, {'F', '-'}, {'F', '#'}, {'G', '-'},
                                       {'G', '#'}, {'A', '-'}, {'A', '#'}, {'B', '-'}};

void screen_note_text(uint32_t note, char out[4]) {
    out[0] = screen_note_names[note % 12u][0];
    out[1] = screen_note_names[note % 12u][1];
    out[2] = (char)('0' + note / 12u % 10u);
    out[3] = 0;
}

uint32_t screen_decimal(char* out, uint32_t n) {
    char reverse[10];
    uint32_t k = 0;
    do {
        reverse[k++] = (char)('0' + n % 10u);
        n /= 10u;
    } while (n);
    for (uint32_t i = 0; i < k; i++) {
        out[i] = reverse[k - 1u - i];
    }
    out[k] = 0;
    return k;
}

void screen_popup_copy(char out[SCREEN_POPUP_TEXT], const char* text) {
    uint32_t n = 0u;
    while (n < SCREEN_POPUP_TEXT - 1u && text[n]) {
        n++;
    }
    memcpy(out, text, n);
    out[n] = 0;
}
