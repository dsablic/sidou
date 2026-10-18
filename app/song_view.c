// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_view.h"

#include <stddef.h>

#include "font.h"
#include "live_view.h"
#include "tracker_view.h"

/* A PLAY the player has not taken yet. */
#define WANT_NONE 0u
#define WANT_PLAY 1u
#define WANT_STOP 2u
#define WANT_RESUME 3u

/* The frame clock (app/song_transport.h): the C64's clock and gt2reloc's CIA period base. */
#define PAL_HZ 985248u
#define NTSC_HZ 1022727u
#define PAL_CIA 0x4cc7u
#define NTSC_CIA 0x42c6u

#define ALL_VOICES ((1u << SONG_T_VOICES) - 1u)
#define ALL_KEYS ((1u << PANEL_NOTE_KEYS) - 1u)
/* live's notes are MIDI's: GoatTracker's C-0 is MIDI's 12. */
#define GT2_NOTE0 12u
#define GT2_NOTES 96u

/* ---- the song views, SEQ's order ---- */

/* NULL: not made yet (SEQ skips it). */
static const view* const VIEWS[SONG_VIEW_COUNT] = {NULL, &tracker_view_view, NULL, NULL, NULL};

int song_view_is(const view* v) {
    for (uint32_t i = 0; i < SONG_VIEW_COUNT; i++) {
        if (v && VIEWS[i] == v) {
            return 1;
        }
    }
    return 0;
}

const view* song_view_next(const view* top) {
    uint32_t from = SONG_VIEW_COUNT - 1u; /* not a song view: from the first */
    for (uint32_t i = 0; i < SONG_VIEW_COUNT; i++) {
        if (top && VIEWS[i] == top) {
            from = i;
        }
    }
    for (uint32_t n = 1; n <= SONG_VIEW_COUNT; n++) {
        const view* v = VIEWS[(from + n) % SONG_VIEW_COUNT];
        if (v) {
            return v;
        }
    }
    return NULL;
}

/* ---- the transport ---- */

void song_view_init(song_view* sv, live* l) {
    sv->t = NULL;
    sv->io = NULL;
    sv->l = l;
    sv->st.state = SONG_T_IDLE;
    sv->st.multispeed = 1;
    sv->st.ntsc = 0;
    sv->st.mute = 0;
    sv->st.applied = 0;
    sv->st.speed[0] = 0;
    sv->st.speed[1] = 0;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        sv->st.order_pos[v] = SONG_T_NOWHERE;
        sv->st.row[v] = SONG_T_NOWHERE;
        sv->st.wave[v] = 0;
        sv->st.env[v] = 0;
    }
    sv->mute = 0;
    sv->solo = 0;
    sv->sent = 0xffu;
    sv->rec = 0;
    sv->want = WANT_NONE;
    sv->voice = 0;
    sv->instr = 1;
    sv->order_pos = 0;
    sv->row = 0;
    sv->rows_beat = SONG_VIEW_ROWS_A_BEAT;
    sv->jam_key = SONG_VIEW_NO_KEY;
    sv->jam_voice = 0;
    sv->jam_off = 0;
    sv->jam_keys = 0;
    sv->song = NULL;
    sv->song_edits = 0;
    sv->scroll = 0;
    sv->which = SONG_VIEW_CANVAS_NONE;
    sv->built = 0;
    sv->dirty = 0;
    sv->held = 0;
    sv->unbuilt = 0;
}

uint32_t song_view_mute_mask(const song_view* sv) {
    return sv->solo ? ~(uint32_t)sv->solo & ALL_VOICES : sv->mute;
}

uint32_t song_view_mutes(const song_view* sv) {
    return (uint32_t)sv->mute | (uint32_t)sv->solo << SONG_T_VOICES;
}

/* The mutes to the player, if it has others. */
static void send_mutes(song_view* sv) {
    uint32_t m = song_view_mute_mask(sv);
    if (sv->t && m != sv->sent && sv->t->mute(sv->t->ctx, m)) {
        sv->sent = (uint8_t)m;
    }
}

/* PLAY's command, if one waits; the state as it will be until the next status says. */
static void send_play(song_view* sv) {
    const song_transport* t = sv->t;
    int ok = 1;
    uint8_t now = sv->st.state;
    if (!t || sv->want == WANT_NONE) {
        return;
    }
    if (sv->want == WANT_STOP) {
        ok = t->stop(t->ctx);
        now = SONG_T_STOPPED;
    } else if (sv->want == WANT_RESUME) {
        ok = t->resume(t->ctx);
        now = SONG_T_PLAYING;
    } else {
        ok = t->play(t->ctx, sv->order_pos);
        now = SONG_T_STARTING;
    }
    if (ok) {
        sv->want = WANT_NONE;
        sv->st.state = now;
    }
}

/* A jam's release, if one waits. */
static void send_jam_off(song_view* sv) {
    if (sv->t && sv->jam_off &&
        sv->t->jam(sv->t->ctx, sv->jam_voice, SONG_T_RELEASE, sv->instr)) {
        sv->jam_off = 0;
    }
}

void song_view_set_transport(song_view* sv, const song_transport* t) {
    sv->t = t;
    sv->song = t ? t->song : NULL;
    sv->built = 0;
    sv->sent = 0xffu;
    sv->want = WANT_NONE;
    sv->jam_off = 0;
    sv->jam_key = SONG_VIEW_NO_KEY;
    if (t) {
        t->status(t->ctx, &sv->st);
        sv->sent = sv->st.mute; /* what the player has: nothing to send for it */
        send_mutes(sv);
    }
}

/* 1 while the song plays or is about to. */
static int going(uint32_t state) {
    return state == SONG_T_STARTING || state == SONG_T_PLAYING;
}

void song_view_play(song_view* sv, int resume) {
    if (!sv->t) {
        return;
    }
    if (sv->want == WANT_NONE) {
        sv->t->status(sv->t->ctx, &sv->st); /* the state now: no frame read it outside */
    }
    /* A PLAY not taken yet counts as done: a second press undoes it. */
    int was = going(sv->st.state);
    int on = sv->want == WANT_NONE ? was : sv->want != WANT_STOP;
    if (on) {
        sv->want = was ? WANT_STOP : WANT_NONE;
    } else if (was) {
        sv->want = WANT_NONE;
    } else {
        sv->want = resume && sv->st.state == SONG_T_STOPPED ? WANT_RESUME : WANT_PLAY;
    }
    send_play(sv);
}

void song_view_rec(song_view* sv) {
    sv->rec = (uint8_t)!sv->rec;
}

void song_view_glo(song_view* sv, uint32_t toggles) {
    sv->mute ^= (uint8_t)(toggles & ALL_VOICES);
    sv->solo ^= (uint8_t)(toggles >> SONG_T_VOICES & ALL_VOICES);
    send_mutes(sv);
}

uint32_t song_view_keys(song_view* sv, uint32_t down, uint32_t pressed, uint32_t owned,
                        int take) {
    /* Let go (or let go and pressed again, which plays again below). */
    uint32_t gone = sv->jam_keys & (~down | pressed);
    if (sv->jam_key != SONG_VIEW_NO_KEY && (gone >> sv->jam_key & 1u)) {
        sv->jam_key = SONG_VIEW_NO_KEY;
        sv->jam_off = 1;
    }
    sv->jam_keys &= ~gone;
    uint32_t taken = 0;
    if (take && sv->t) {
        uint32_t fresh = (pressed | (down & ~owned & ~sv->jam_keys)) & ALL_KEYS;
        for (uint32_t k = 0; fresh >> k; k++) {
            uint32_t bit = 1u << k;
            if (!(fresh & bit)) {
                continue;
            }
            taken |= bit;
            uint8_t n = live_key_note(sv->l, (uint8_t)(PANEL_BASE_NOTE + k));
            if (n == LIVE_NO_NOTE || n < GT2_NOTE0 || n - GT2_NOTE0 >= GT2_NOTES) {
                continue; /* outside the player's notes: the key sounds nothing */
            }
            if (sv->jam_off && sv->jam_voice == sv->voice) {
                sv->jam_off = 0; /* the new note takes the voice over */
            }
            send_jam_off(sv);
            /* A note that cannot go now is dropped. */
            (void)sv->t->jam(sv->t->ctx, sv->voice, n - GT2_NOTE0, sv->instr);
            sv->jam_voice = sv->voice;
            sv->jam_key = (uint8_t)k;
            if (!(down & bit)) {
                sv->jam_key = SONG_VIEW_NO_KEY; /* pressed and let go between two calls */
                sv->jam_off = 1;
            }
        }
        sv->jam_keys |= taken & down;
    }
    send_jam_off(sv);
    return taken;
}

void song_view_frame(song_view* sv, int shows) {
    const song_transport* t = sv->t;
    if (!t || (!shows && sv->want == WANT_NONE && !sv->jam_off &&
               song_view_mute_mask(sv) == sv->sent)) {
        return; /* outside the song views, most frames: nothing waits */
    }
    send_play(sv);
    if (shows && sv->want == WANT_NONE) {
        t->status(t->ctx, &sv->st);
        sv->song_edits = sv->st.applied;
    }
    send_mutes(sv);
    send_jam_off(sv);
}

/* ---- the tempo and the position the views show ---- */

uint32_t song_view_bpm10(uint32_t speed_sum, uint32_t multispeed, uint32_t ntsc,
                         uint32_t rows_beat) {
    if (speed_sum == 0u || rows_beat == 0u) {
        return 0;
    }
    uint32_t cia = ntsc ? NTSC_CIA : PAL_CIA;
    uint32_t period = multispeed ? cia / (multispeed > 4u ? 4u : multispeed) + 1u : 2u * cia + 1u;
    /* Calls a minute times 20 (tenths of a BPM over two rows), in 1/256ths. */
    uint32_t num = 1200u * (ntsc ? NTSC_HZ : PAL_HZ);
    uint32_t x = num / period * 256u + num % period * 256u / period;
    uint32_t d = speed_sum * rows_beat;
    return (x + d * 128u) / (d * 256u);
}

void song_view_position(const song_view* sv, uint32_t v, uint32_t* order_pos, uint32_t* row) {
    if (going(sv->st.state) && sv->st.row[v] != SONG_T_NOWHERE) {
        *order_pos = sv->st.order_pos[v];
        *row = sv->st.row[v];
    } else {
        *order_pos = sv->order_pos;
        *row = sv->row;
    }
}

int song_view_playing(const song_view* sv) {
    return going(sv->st.state);
}

/* The BPM as the header shows it, rounded; 0xFFFF: none. */
static uint32_t shown_bpm(const song_view* sv) {
    if (!sv->t) {
        return 0xffffu;
    }
    uint32_t b = song_view_bpm10((uint32_t)sv->st.speed[0] + sv->st.speed[1], sv->st.multispeed,
                                 sv->st.ntsc, sv->rows_beat);
    return b ? (b + 5u) / 10u : 0xffffu;
}

uint32_t song_view_shown_bpm(const song_view* sv) {
    return shown_bpm(sv);
}

/* ---- the shared canvas ---- */

int song_view_acquire_canvas(song_view* sv, uint32_t owner) {
    if (sv->which == owner) {
        return 0;
    }
    sv->which = (uint8_t)owner;
    sv->built = 0;
    sv->dirty = 0;
    sv->unbuilt = 0;
    return 1;
}

/* 1 for a cell of the screen's outer ring. */
static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

void song_view_copy(song_view* sv, screen* s, grid* g) {
    const song_view_canvas* c = &sv->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_canvas_row(g, c, r);
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < SONG_VIEW_FRAME_CELLS ? SONG_VIEW_FRAME_CELLS - used : 0u;
    uint32_t rows = (sv->dirty | (s->redraw ? s->redraw_rows : 0u)) & ~whole;
    for (uint32_t r = 0; rows >> r; r++) {
        if (!(rows >> r & 1u)) {
            continue;
        }
        const uint8_t* gc = &g->ch[r * GRID_COLS];
        const uint8_t* ga = &g->attr[r * GRID_COLS];
        const uint8_t* gs = &g->src[r * GRID_COLS];
        for (uint32_t k = 0; k < GRID_COLS; k++) {
            uint8_t ch = c->ch[r][k];
            /* The same as !grid_canvas_differs, inline: this scan is the busiest part. */
            if ((gc[k] == ch && ga[k] == c->attr[r][k] &&
                 ((uint8_t)(ch - GRID_BIG_FIRST) >= GRID_BIG_PIECES || gs[k] == c->src[r][k])) ||
                screen_covered(s, k, r) || (raster && border(k, r))) {
                continue;
            }
            if (!left) {
                sv->dirty = rows & ~((1u << r) - 1u); /* this row and the ones below: next */
                sv->held++;
                return;
            }
            /* grid_canvas_put, inline: this is the busiest loop of a page turn. */
            uint32_t i = r * GRID_COLS + k;
            g->ch[i] = ch;
            g->attr[i] = c->attr[r][k];
            g->rows |= 1u << r;
            if ((uint8_t)(ch - GRID_BIG_FIRST) < GRID_BIG_PIECES && gs[k] != c->src[r][k]) {
                g->src[i] = c->src[r][k];
                g->big_touched = 1;
                g->stale[i >> 3] |= (uint8_t)(1u << (i & 7u));
            }
            left--;
        }
    }
    /* All of it is on the grid but what the layers hold: those come back when the screen
     * composes again after them (it forces that). */
    sv->dirty = 0;
}

/* The source character is part of a large cell's identity, even if its piece code is unchanged. */
static void canvas_large_put(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t a,
                             int big) {
    if (big) {
        uint32_t i = row * GRID_COLS + col;
        if (g->src[i] != ch) {
            g->src[i] = ch;
            g->big_touched = 1;
            grid_mark(g, col, row);
        }
        ch = (uint8_t)(GRID_BIG_FIRST + (row & 1u) * 2u + (col & 1u));
    }
    grid_put(g, col, row, ch, a);
}

void song_view_copy_large(song_view* sv, screen* s, grid* g, uint32_t big_rows) {
    const song_view_canvas* cv = &sv->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            if (big_rows >> r & 1u) {
                for (uint32_t c = 0; c < GRID_COLS; c++) {
                    canvas_large_put(g, c, r, cv->ch[r][c], cv->attr[r][c], 1);
                }
            } else {
                grid_row(g, r, cv->ch[r], cv->attr[r]);
            }
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < SONG_VIEW_FRAME_CELLS ? SONG_VIEW_FRAME_CELLS - used : 0u;
    uint32_t rows = (sv->dirty | (s->redraw ? s->redraw_rows : 0u)) & ~whole;
    for (uint32_t r = 0; rows >> r; r++) {
        if (!(rows >> r & 1u)) {
            continue;
        }
        int big = (int)(big_rows >> r & 1u);
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            uint32_t i = r * GRID_COLS + c;
            uint8_t ch = big ? (uint8_t)(GRID_BIG_FIRST + (r & 1u) * 2u + (c & 1u)) : cv->ch[r][c];
            if ((g->ch[i] == ch && g->attr[i] == cv->attr[r][c] &&
                 (!big || g->src[i] == cv->ch[r][c])) || screen_covered(s, c, r) ||
                (raster && border(c, r))) {
                continue;
            }
            if (!left) {
                sv->dirty = rows & ~((1u << r) - 1u);
                sv->held++;
                return;
            }
            canvas_large_put(g, c, r, cv->ch[r][c], cv->attr[r][c], big);
            left--;
        }
    }
    sv->dirty = 0;
}
