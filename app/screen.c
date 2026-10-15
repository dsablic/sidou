// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "screen.h"

#include "font.h"

const uint8_t screen_ramp[SCREEN_RAMP] = {GRID_WHITE, GRID_YELLOW,     GRID_LIGHT_GREEN,
                                          GRID_CYAN,  GRID_LIGHT_BLUE, GRID_PURPLE,
                                          GRID_LIGHT_RED};

#define TOAST_ATTR GRID_ATTR(GRID_BLUE, GRID_LIGHT_BLUE)
#define EDGE_ATTR GRID_ATTR(GRID_WHITE, GRID_BLACK)
#define RASTER_MAX 255u

static void enter(screen* s) {
    const screen_entry* e = &s->stack[s->depth - 1u];
    if (e->v->enter) {
        e->v->enter(s, e->state);
    }
}

/* The view on top changed: the old one's overlays go, and the new one comes in. */
static void switched(screen* s, screen_wipe w) {
    grid_overlay_clear_all(s->g);
    s->wipe = (uint8_t)w;
    s->wipe_rows = 0;
    if (w == SCREEN_CUT) {
        grid_hold(s->g, 0);
    }
}

void screen_init(screen* s, grid* g, const view* root, void* state) {
    static const screen empty;
    *s = empty;
    s->g = g;
    s->stack[0].v = root;
    s->stack[0].state = state;
    s->depth = 1;
    enter(s);
}

int screen_push(screen* s, const view* v, void* state, screen_wipe w) {
    if (s->depth >= SCREEN_DEPTH) {
        return -1;
    }
    s->stack[s->depth].v = v;
    s->stack[s->depth].state = state;
    s->depth++;
    switched(s, w);
    enter(s);
    return 0;
}

int screen_pop(screen* s, screen_wipe w) {
    if (s->depth <= 1u) {
        return -1;
    }
    const screen_entry* e = &s->stack[s->depth - 1u];
    if (e->v->exit) {
        e->v->exit(s, e->state);
    }
    s->depth--;
    switched(s, w);
    return 0;
}

const view* screen_top(const screen* s) {
    return s->stack[s->depth - 1u].v;
}

void screen_capture(screen* s, screen_capture_fn fn, void* ctx) {
    s->capture = fn;
    s->capture_ctx = ctx;
}

int screen_input(screen* s, const screen_event* e) {
    if (s->capture && s->capture(s->capture_ctx, e)) {
        return 1;
    }
    const screen_entry* top = &s->stack[s->depth - 1u];
    return top->v->input ? top->v->input(s, top->state, e) : 0;
}

void screen_toast(screen* s, const char* text, uint32_t ms) {
    uint32_t n = 0;
    for (; text[n] && n < SCREEN_TOAST_CHARS; n++) {
        s->toast[n] = text[n];
    }
    s->toast[n] = 0;
    s->toast_on = 1;
    s->toast_until = s->now + ms;
}

void screen_raster(screen* s, uint32_t steps) {
    uint32_t frames = steps * SCREEN_RASTER_FRAMES;
    s->raster_frames = (uint8_t)(frames < RASTER_MAX ? frames : RASTER_MAX);
}

int screen_blink(const screen* s) {
    return (s->frame / SCREEN_BLINK_FRAMES) % 2u == 0u;
}

int screen_animating(const screen* s) {
    return s->wipe != SCREEN_CUT || s->raster_frames != 0;
}

static void toast(screen* s) {
    if (s->toast_on && (int32_t)(s->now - s->toast_until) >= 0) {
        s->toast_on = 0;
    }
    if (s->toast_on) {
        grid_text(s->g, SCREEN_TOAST_COL, SCREEN_TOAST_ROW, s->toast, TOAST_ATTR);
    }
}

/* The border's colours: the ramp in bands by row, moving down a row a step. */
static void raster(screen* s) {
    if (!s->raster_frames || s->wipe != SCREEN_CUT) {
        return;
    }
    grid* g = s->g;
    uint32_t phase = (s->raster_frames + SCREEN_RASTER_FRAMES - 1u) / SCREEN_RASTER_FRAMES;
    s->raster_frames--;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        uint32_t fg = screen_ramp[(r + phase) % SCREEN_RAMP];
        uint32_t i = r * GRID_COLS;
        uint32_t step = r == 0 || r == GRID_ROWS - 1u ? 1u : GRID_COLS - 1u;
        for (uint32_t c = 0; c < GRID_COLS; c += step) {
            grid_put(g, c, r, g->ch[i + c], (uint8_t)((g->attr[i + c] & 0xF0u) | fg));
        }
    }
}

/* Rows [0, n) revealed (bottom up: [GRID_ROWS - n, GRID_ROWS)), the next one the edge, the
 * rest held. */
static void wipe(screen* s) {
    if (s->wipe == SCREEN_CUT) {
        return;
    }
    uint32_t n = s->wipe_rows + SCREEN_WIPE_ROWS;
    if (n >= GRID_ROWS) {
        s->wipe = SCREEN_CUT;
        grid_hold(s->g, 0);
        return;
    }
    s->wipe_rows = (uint8_t)n;
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
}

void screen_frame(screen* s, uint32_t now_ms) {
    s->now = now_ms;
    s->frame++;
    const screen_entry* top = &s->stack[s->depth - 1u];
    top->v->compose(s, top->state, s->g);
    toast(s);
    raster(s);
    wipe(s);
}
