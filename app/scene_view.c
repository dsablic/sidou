// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scene_view.h"

#include <stddef.h>
#include <string.h>

#include "live_view.h"
#include "panel.h"

_Static_assert(SCENE_VOICES == LIVE_VOICES, "a piece's voice per voice");
_Static_assert(SCENE_NO_NOTE == LIVE_NO_NOTE, "the same no-note");
_Static_assert(SCOPE_VOICES == SCENE_VOICES, "an envelope per voice");

#define BLANK GRID_ATTR(GRID_BLACK, GRID_BLACK)
static const uint8_t BLANK_CH[GRID_COLS] = {
    ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ',
    ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '};
static const uint8_t BLANK_ATTR[GRID_COLS];
_Static_assert(BLANK == 0u, "a row of zeros is a row of blank cells' colours");

void scene_view_init(scene_view* sv, const live* l, scope* sc) {
    memset(sv, 0, sizeof *sv);
    sv->l = l;
    sv->tap = sc;
    scene_song_init(&sv->song);
    scene_start(&sv->u.piece, sv->which, NULL);
}

void scene_view_song(scene_view* sv, const song_ctl* sc, const song_event* ev, uint32_t n) {
    scene_song_events(&sv->song, sc, ev, n);
}

static void cover_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const grid* g = ctx;
    uint32_t i = y / GRID_CELL_PX * GRID_COLS + x / GRID_CELL_PX;
    if (g->ch[i] != ' ' || g->attr[i] != BLANK) {
        for (uint32_t k = 0; k < n; k++) {
            out[k] = (uint8_t)(g->attr[i] >> 4);
        }
    }
}

static void cover(scene_view_rite* rt, const screen* s) {
    int32_t x0 = SCENE_PX_W, y0 = SCENE_PX_H, x1 = 0, y1 = 0;
    const int32_t px = (int32_t)GRID_CELL_PX;
    if (s->raster_frames) {
        x0 = y0 = 0, x1 = SCENE_PX_W, y1 = SCENE_PX_H;
    }
    if (s->cover_w) {
        x0 = s->cover_col * px < x0 ? s->cover_col * px : x0;
        y0 = s->cover_row * px < y0 ? s->cover_row * px : y0;
        x1 = (s->cover_col + s->cover_w) * px > x1 ? (s->cover_col + s->cover_w) * px : x1;
        y1 = (s->cover_row + s->cover_h) * px > y1 ? (s->cover_row + s->cover_h) * px : y1;
    }
    if (s->toast_on) {
        const int32_t r = (int32_t)SCREEN_TOAST_FIRST_ROW(s) * px, c = (int32_t)SCREEN_TOAST_COL * px;
        const int32_t e = c + (int32_t)s->toast_w * px;
        const int32_t b = r + (int32_t)s->toast_h * px;
        x0 = c < x0 ? c : x0, y0 = r < y0 ? r : y0;
        x1 = e > x1 ? e : x1, y1 = b > y1 ? b : y1;
    }
    static const scene_px_layer none;
    scene_px_layer l = none;
    if (x0 < x1) {
        l.kind = SCENE_PX_CALL;
        l.fn = cover_row;
        l.src = rt->g;
        l.x0 = (int16_t)x0, l.y0 = (int16_t)y0, l.x1 = (int16_t)x1, l.y1 = (int16_t)y1;
    }
    scene_px_set(&rt->px, SCENE_RMR_SLOT_COVER, &l);
}

static void rite_reset(scene_view* sv, grid* g) {
    scene_view_rite* rt = &sv->u.rite;
    rt->g = g;
    scene_px_init(&rt->px, g);
    rt->px.fresh = 1;
    scene_rmr_draw(&rt->rmr, &rt->px);
    rt->px.fresh = 0;
}

static void rite_start(scene_view* sv, grid* g) {
    scene_rmr_init(&sv->u.rite.rmr);
    sv->song.n = 0;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        sv->song.stamp[v] = sv->l->voice[v].stamp;
    }
    rite_reset(sv, g);
}

static void rite_step(scene_view* sv) {
    scene_view_rite* rt = &sv->u.rite;
    scene_song_frame_out m;
    scene_song_frame(&sv->song, sv->l, &m);
    for (uint32_t i = 0; i < m.n; i++) {
        scene_rmr_step(&rt->rmr, &m.beat[i]);
    }
    scene_rmr_frame(&rt->rmr, m.cutoff, m.sounding);
    scene_rmr_draw(&rt->rmr, &rt->px);
}

static void rite_compose(screen* s, scene_view* sv, grid* g) {
    scene_view_rite* rt = &sv->u.rite;
    if (g->region_ctx != &rt->px) {
        rite_reset(sv, g);
    }
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (!(whole >> r & 1u)) {
            continue;
        }
        if (r - SCREEN_TOAST_FIRST_ROW(s) >= s->toast_h && (!s->cover_w || r - s->cover_row >= s->cover_h)) {
            grid_row(g, r, BLANK_CH, BLANK_ATTR);
            continue;
        }
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            if (!screen_covered(s, c, r)) {
                grid_put(g, c, r, ' ', BLANK);
            }
        }
    }
    if (screen_animating(s)) {
        cover(rt, s);
        rt->left = 0;
        return;
    }
    rite_step(sv);
    cover(rt, s);
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < SCENE_VIEW_FRAME_CELLS ? SCENE_VIEW_FRAME_CELLS - used : 0u;
    uint32_t i = sv->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint32_t r = i / GRID_COLS, c = i % GRID_COLS;
        if ((whole >> r & 1u) || (g->ch[i] == ' ' && g->attr[i] == BLANK) ||
            screen_covered(s, c, r)) {
            continue;
        }
        if (!left) {
            sv->cursor = (uint16_t)i;
            sv->held++;
            break;
        }
        grid_put(g, c, r, ' ', BLANK);
        left--;
    }
    if (left && rt->rmr.quiet < SCENE_RMR_CALM_FRAMES) {
        scene_px_refresh(&rt->px, g, 1);
        left--;
    }
    rt->left = (uint16_t)left;
    sv->idle = 1;
}

int scene_view_idle(scene_view* sv) {
    if (!sv->on || sv->which != SCENE_RMR || sv->starting) {
        sv->idle = 0;
        return 0;
    }
    scene_view_rite* rt = &sv->u.rite;
    if (!rt->g || !rt->px.ncand || !rt->left) {
        sv->idle = 0;
        return 0;
    }
    uint32_t before = rt->px.marked;
    scene_px_scan(&rt->px, rt->g, SCENE_PX_SCAN_SLICE);
    uint32_t m = rt->px.marked - before;
    rt->left = (uint16_t)(m < rt->left ? rt->left - m : 0u);
    sv->idle = (uint8_t)(rt->px.ncand && rt->left);
    return sv->idle;
}

void scene_view_tap(scene_view* sv, int on) {
    if (!on) {
        sv->prepared = 0;
    }
    sv->on = (uint8_t)(on != 0);
}

static void listened(scene_view* sv) {
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        sv->stamp[v] = sv->l->voice[v].stamp;
    }
}

static void listen(scene_view* sv, scene_music* m) {
    const live* l = sv->l;
    int heard = sv->tap && sv->on;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        const live_voice* x = &l->voice[v];
        uint8_t ctrl = live_view_voice_control(l, v);
        uint8_t gate = (uint8_t)(x->gate && ctrl);
        m->gate[v] = gate;
        m->trig[v] = (uint8_t)(gate && x->stamp != sv->stamp[v]);
        m->note[v] = ctrl ? x->note : SCENE_NO_NOTE;
        m->env[v] = heard && ctrl ? sv->tap->env[v] : 0u;
        m->color[v] = live_view_wave_color(ctrl);
    }
    m->cutoff = l->cutoff_out;
    m->resonance = l->resonance;
    listened(sv);
}

static void step(scene_view* sv) {
    listen(sv, &sv->music);
    scene_step(&sv->u.piece, &sv->music);
}

void scene_view_prepare(scene_view* sv) {
    if (sv->on && !sv->starting && sv->which != SCENE_RMR) {
        step(sv);
        sv->prepared = 1;
    }
}

static void enter(screen* s, void* state) {
    (void)s;
    scene_view* sv = state;
    sv->starting = 1;
    sv->say = 1;
    sv->prepared = 0;
    sv->cursor = 0;
}

static int input(screen* s, void* state, const screen_event* e) {
    scene_view* sv = state;
    if (e->kind != SCREEN_TURN || (e->id != PANEL_ENC_SELECT && e->id != PANEL_ENC_ALGORITHM)) {
        return 0;
    }
    int32_t k = ((int32_t)sv->which + e->steps) % (int32_t)SCENE_PIECES;
    sv->which = (uint8_t)(k < 0 ? k + (int32_t)SCENE_PIECES : k);
    enter(s, sv);
    screen_reveal(s, SCREEN_WIPE_DOWN);
    return 1;
}

static void compose(screen* s, void* state, grid* g) {
    scene_view* sv = state;
    if (sv->say && screen_top(s) == &scene_view_view) {
        sv->say = 0;
        screen_toast(s, scene_names[sv->which], SCREEN_TOAST_MS);
    }
    if (sv->starting) {
        sv->starting = 0;
        if (sv->which == SCENE_RMR) {
            rite_start(sv, g);
        } else {
            if (g->region_painter) {
                grid_region_clear(g);
            }
            listen(sv, &sv->music);
            scene_begin(&sv->u.piece, sv->which);
        }
    }
    if (sv->which == SCENE_RMR) {
        rite_compose(s, sv, g);
        return;
    }
    scene_fill(&sv->u.piece, &sv->music, screen_reach(s));
    if (!screen_animating(s)) {
        if (!sv->prepared) {
            step(sv);
        }
        sv->prepared = 0;
    }
    const scene_canvas* cv = &sv->u.piece.cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_row(g, r, cv->ch[r], cv->attr[r]);
        }
    }
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < SCENE_VIEW_FRAME_CELLS ? SCENE_VIEW_FRAME_CELLS - used : 0u;
    uint32_t i = sv->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint32_t r = i / GRID_COLS;
        uint32_t c = i % GRID_COLS;
        if ((whole >> r & 1u) || (g->ch[i] == cv->ch[r][c] && g->attr[i] == cv->attr[r][c]) ||
            screen_covered(s, c, r)) {
            continue;
        }
        if (!left) {
            sv->cursor = (uint16_t)i;
            sv->held++;
            return;
        }
        grid_put(g, c, r, cv->ch[r][c], cv->attr[r][c]);
        left--;
    }
}

const view scene_view_view = {"SCENE", enter, NULL, input, compose, NULL};
