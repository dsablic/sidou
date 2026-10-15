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

void scene_view_init(scene_view* sv, const live* l, scope* sc) {
    memset(sv, 0, sizeof *sv); /* not a const template: 3 KB of zeros in flash */
    sv->l = l;
    sv->tap = sc;
    scene_start(&sv->piece, sv->which, NULL);
}

void scene_view_tap(scene_view* sv, int on) {
    if (!on) {
        sv->prepared = 0;
    }
    sv->on = (uint8_t)(on != 0);
}

/* The voices' stamps now: a note-on before this is not the piece's. */
static void listened(scene_view* sv) {
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        sv->stamp[v] = sv->l->voice[v].stamp;
    }
}

/* What the music does this frame. */
static void listen(scene_view* sv, scene_music* m) {
    const live* l = sv->l;
    int heard = sv->tap && sv->on;
    for (uint32_t v = 0; v < SCENE_VOICES; v++) {
        const live_voice* x = &l->voice[v];
        uint8_t ctrl = live_view_voice_control(l, v);
        uint8_t gate = (uint8_t)(x->gate && ctrl);
        m->gate[v] = gate;
        m->trig[v] = (uint8_t)(gate && x->stamp != sv->stamp[v]); /* a note-off changes it too */
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
    scene_step(&sv->piece, &sv->music);
}

void scene_view_prepare(scene_view* sv) {
    if (sv->on && !sv->starting) {
        step(sv);
        sv->prepared = 1;
    }
}

/* The piece from its start, at the next compose (compose): not here, in the input's call. */
static void enter(screen* s, void* state) {
    (void)s;
    scene_view* sv = state;
    sv->starting = 1;
    sv->prepared = 0;
    sv->cursor = 0;
}

/* SELECT: the next piece (or the one before), wiped in, its name in a toast. */
static int input(screen* s, void* state, const screen_event* e) {
    scene_view* sv = state;
    if (e->kind != SCREEN_TURN || e->id != PANEL_ENC_SELECT) {
        return 0;
    }
    int32_t k = ((int32_t)sv->which + e->steps) % (int32_t)SCENE_PIECES;
    sv->which = (uint8_t)(k < 0 ? k + (int32_t)SCENE_PIECES : k);
    enter(s, sv);
    screen_reveal(s, SCREEN_WIPE_DOWN);
    screen_toast(s, scene_names[sv->which], SCREEN_TOAST_MS);
    return 1;
}

/* The canvas onto the grid: the changed cells from where the last frame stopped, until the
 * frame's budget is spent; what the screen drew over (a toast, the raster) is put back the
 * same way. What is covered this frame (a toast that shows, MIDI learn's frame:
 * screen_covered) stays as it is and costs nothing. Under a wipe, every row whole: the wipe
 * holds the rows it has not reached, and the ones it reveals must be SCENE's. */
static void compose(screen* s, void* state, grid* g) {
    scene_view* sv = state;
    /* The piece picked (enter) starts as the music now shows it, its canvas drawn as far as
     * the wipe reveals it this frame (scene_fill; PLASMA's rows are costly): whole once no wipe
     * from the top runs. */
    if (sv->starting) {
        sv->starting = 0;
        listen(sv, &sv->music);
        scene_begin(&sv->piece, sv->which);
    }
    scene_fill(&sv->piece, &sv->music,
               s->wipe == SCREEN_WIPE_DOWN ? s->wipe_rows + SCREEN_WIPE_ROWS : GRID_ROWS);
    /* Still while a wipe or a raster runs (app/screen.h). */
    if (!screen_animating(s)) {
        if (!sv->prepared) {
            step(sv);
        }
        sv->prepared = 0;
    }
    const scene_canvas* cv = &sv->piece.cv;
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

/* Composed every frame (no changed function). */
const view scene_view_view = {"SCENE", enter, NULL, input, compose, NULL};
