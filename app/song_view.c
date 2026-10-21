// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "live_view.h"
#include "step_view.h"
#include "tracker_view.h"
#include "song_pages.h"

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
static const view* const VIEWS[SONG_VIEW_COUNT] = {
    &step_view_view, &tracker_view_view, &song_pages_order_view, &song_pages_instr_view, &song_pages_table_view
};

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
    sv->st.subtune = 0;
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
    sv->edit_pending = 0;
    sv->dirty = 0;
    sv->held = 0;
    sv->unbuilt = 0;
    sv->editor = NULL;
    sv->edit_screen = NULL;
    sv->step = 1;
    sv->field = TRACKER_VIEW_FIELD_NOTE;
    sv->clear_armed = 0;
    sv->subtune = 0;
    sv->quant = SONG_VIEW_QUANT_NEAREST;
    sv->counting = 0;
    sv->was_playing = 0;
    sv->rec_knobs = 0;
    sv->count_end = 0;
    sv->rec_cut = 0;
    sv->rec_res = 0;
    sv->rec_n = 0;
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
        ok = t->play_from ? t->play_from(t->ctx, sv->subtune, sv->order_pos) :
                           t->play(t->ctx, sv->order_pos);
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
    if (sv->editor && sv->editor->pending) {
        return;
    }
    sv->t = t;
    sv->song = t ? t->song : NULL;
    sv->built = 0;
    sv->sent = 0xffu;
    sv->want = WANT_NONE;
    sv->jam_off = 0;
    sv->jam_key = SONG_VIEW_NO_KEY;
    if (sv->editor) {
        song_edit_init(sv->editor, t);
    }
    sv->clear_armed = 0;
    if (t) {
        t->status(t->ctx, &sv->st);
        sv->sent = sv->st.mute; /* what the player has: nothing to send for it */
        send_mutes(sv);
    }
}

void song_view_loaded(song_view* sv) {
    if (sv->editor) {
        song_edit_init(sv->editor, sv->t);
    }
    sv->edit_pending = 0;
    sv->clear_armed = 0;
    sv->order_pos = sv->row = sv->voice = 0;
    sv->subtune = 0;
    sv->instr = 1;
    sv->want = WANT_NONE;
    sv->jam_key = SONG_VIEW_NO_KEY;
    sv->jam_keys = 0;
    sv->jam_off = 0;
    sv->built = 0;
    sv->unbuilt = 0;
    sv->rec_n = 0;
    sv->counting = 0;
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

/* A count-in of one bar (the rows a beat times four, at the song's tempo) from the last call:
 * REC records nothing until it has run out. */
static void count_in(song_view* sv) {
    uint32_t calls = ((uint32_t)sv->st.speed[0] + sv->st.speed[1]) / 2u;
    sv->count_end = (uint16_t)(sv->st.now_call + (calls ? calls : 6u) * 4u * sv->rows_beat);
    sv->counting = 1;
    if (sv->edit_screen) {
        screen_toast(sv->edit_screen, "COUNT-IN", SCREEN_TOAST_MS);
    }
}

void song_view_rec(song_view* sv) {
    if (sv->t && sv->want == WANT_NONE) {
        sv->t->status(sv->t->ctx, &sv->st); /* the state now: no frame may have read it */
    }
    sv->rec = (uint8_t)!sv->rec;
    sv->rec_knobs = 0; /* the knobs start from LIVE's values again */
    sv->counting = 0;
    if (sv->rec && sv->quant == SONG_VIEW_QUANT_COUNT && sv->t && sv->st.state == SONG_T_PLAYING) {
        count_in(sv);
    }
}

void song_view_glo(song_view* sv, uint32_t toggles) {
    sv->mute ^= (uint8_t)(toggles & ALL_VOICES);
    sv->solo ^= (uint8_t)(toggles >> SONG_T_VOICES & ALL_VOICES);
    send_mutes(sv);
}

uint32_t song_view_keys(song_view* sv, uint32_t down, uint32_t pressed, uint32_t owned,
                        int take, screen* recording_screen, screen* live_screen) {
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
            uint32_t pitch = n < GT2_NOTE0 ? GT2_NOTES : n - GT2_NOTE0;
            if (live_screen && song_view_rec_playing(sv)) {
                if (pitch < GT2_NOTES) {
                    (void)song_view_record_note(sv, live_screen, pitch, sv->instr, 0);
                }
            } else if (sv->rec && recording_screen) {
                tracker_view_note(recording_screen, sv, pitch);
            }
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

__attribute__((noinline, unused)) static void edit_poll(song_view* sv) {
    uint32_t previous = sv->editor->status;
    (void)song_edit_poll(sv->editor);
    sv->edit_pending = sv->editor->pending;
    const char* msg = song_edit_message(sv->editor);
    if (sv->edit_screen && msg && *msg && previous != sv->editor->status) {
        screen_toast(sv->edit_screen, msg, SCREEN_TOAST_MS);
    }
}

/* ---- REC while the song plays ---- */

int song_view_rec_playing(const song_view* sv) {
    return sv->rec && sv->t && sv->song && sv->st.state == SONG_T_PLAYING;
}

int song_view_recording(const song_view* sv) {
    return song_view_rec_playing(sv) &&
           !(sv->counting && (int16_t)(sv->st.now_call - sv->count_end) < 0);
}

uint32_t song_view_quantise(uint32_t quant, uint32_t elapsed_q8, uint32_t row_q8) {
    if (row_q8 == 0u) {
        return 0u;
    }
    return quant == SONG_VIEW_QUANT_OFF ? elapsed_q8 / row_q8
                                        : (2u * elapsed_q8 + row_q8) / (2u * row_q8);
}

/* The first pattern entry at or after `i` in an orderlist of n entries (o[n + 1] is its
 * restart position), or n for none. */
static uint32_t pattern_entry(const uint8_t* o, uint32_t i, uint32_t n) {
    while (i < n && o[i] >= GT2_REPEAT) {
        i++;
    }
    return i;
}

/* Where a key pressed now falls on the cursor's voice: the pattern and row, `ahead` rows of its
 * playing row on (the quantised one, or the row it is in), running into the pattern that plays
 * next, or round, when that is past the end. 0: not placed (no row yet, no orderlist). */
static int rec_place(song_view* sv, int quantised, uint32_t* pattern, uint32_t* row) {
    const song_transport* t = sv->t;
    if (sv->want == WANT_NONE) {
        t->status(t->ctx, &sv->st); /* the clock now */
    }
    uint32_t v = sv->voice;
    const gt2_song* s = sv->song;
    if (!song_view_rec_playing(sv) || sv->st.row[v] == SONG_T_NOWHERE) {
        return 0;
    }
    uint32_t sub = sv->st.subtune;
    const uint8_t* o = gt2_order_data(s, sub, v);
    uint32_t n = gt2_order_entries(s, sub, v);
    uint32_t i = o ? pattern_entry(o, sv->st.order_pos[v], n) : n;
    if (i >= n) {
        return 0;
    }
    uint32_t row_q8 = ((uint32_t)sv->st.speed[0] + sv->st.speed[1]) * 128u;
    uint32_t elapsed = ((uint32_t)(uint16_t)(sv->st.now_call - sv->st.row_call[v]) << 8) +
                       sv->st.now_frac;
    uint32_t ahead = song_view_quantise(quantised ? sv->quant : SONG_VIEW_QUANT_OFF, elapsed,
                                        row_q8 ? row_q8 : 6u * 256u);
    ahead = ahead > 1u ? 1u : ahead; /* a stale row would put it further */
    uint32_t at = sv->st.row[v] + ahead;
    uint32_t pat = o[i], rows = gt2_pattern_rows(s, pat) - 1u;
    if (at >= rows) {
        uint32_t j = pattern_entry(o, i + 1u, n);
        if (j >= n) {
            j = pattern_entry(o, o[n + 1u], n); /* the restart position */
        }
        at -= rows;
        if (j < n) {
            pat = o[j];
            rows = gt2_pattern_rows(s, pat) - 1u;
        }
        at = rows ? at % rows : 0u;
    }
    *pattern = pat;
    *row = at;
    return 1;
}

/* A refusal's toast, on the screen the key came on, else the song view's. */
static void rec_toast(const song_view* sv, screen* s, const char* msg) {
    screen* on = s ? s : sv->edit_screen;
    if (on && msg && *msg) {
        screen_toast(on, msg, SCREEN_TOAST_MS);
        on->popup_ask = 0;
    }
}

__attribute__((noinline, unused)) static void rec_push(song_view* sv, uint32_t pattern, uint32_t row,
                                                      uint32_t note, uint32_t instr) {
    uint8_t* q = sv->rec_q[sv->rec_n++];
    q[0] = (uint8_t)pattern;
    q[1] = (uint8_t)row;
    q[2] = (uint8_t)note;
    q[3] = (uint8_t)instr;
}

int song_view_record_note(song_view* sv, screen* s, uint32_t note, uint32_t instr, int keyoff) {
    uint32_t pattern, row;
    if (!song_view_recording(sv) || !sv->editor || !rec_place(sv, 1, &pattern, &row) ||
        !song_view_recording(sv)) {
        return 0;
    }
    uint32_t rows = gt2_pattern_rows(sv->song, pattern) - 1u;
    int off = keyoff && row + 1u < rows;
    if (sv->rec_n + (off ? 2u : 1u) > SONG_VIEW_REC_QUEUE) {
        rec_toast(sv, s, "WAITING");
        return 0;
    }
    rec_push(sv, pattern, row, note, instr);
    if (off) {
        rec_push(sv, pattern, row + 1u, SONG_VIEW_REC_OFF, 0u);
    }
    song_view_rec_drain(sv);
    return 1;
}

void song_view_rec_drain(song_view* sv) {
    song_editor* ed = sv->editor;
    while (sv->rec_n && ed && sv->song) {
        if (sv->edit_pending && ed->sent) {
            (void)song_edit_poll(ed);
            sv->edit_pending = ed->pending;
        }
        if (ed->pending) {
            return;
        }
        uint8_t q[4];
        memcpy(q, sv->rec_q[0], 4u);
        sv->rec_n--;
        for (uint32_t i = 0; i < sv->rec_n; i++) {
            memcpy(sv->rec_q[i], sv->rec_q[i + 1u], 4u);
        }
        uint8_t cell[4];
        if (!song_edit_read_row(ed, q[0], q[1], cell)) {
            rec_toast(sv, NULL, song_edit_message(ed));
            continue;
        }
        if (q[2] == SONG_VIEW_REC_OFF) {
            if (cell[0] != GT2_REST) {
                continue; /* a note went there meanwhile: it keeps its row */
            }
            cell[0] = GT2_KEYOFF;
        } else {
            cell[0] = (uint8_t)(GT2_FIRSTNOTE + q[2]);
            cell[1] = q[3];
        }
        int ok = song_edit_row(ed, q[0], q[1], cell);
        sv->edit_pending = ed->pending;
        if (!ok || ed->status == SONG_EDIT_TIDYING) {
            rec_toast(sv, NULL, song_edit_message(ed));
        }
        return; /* one write, then wait for its acknowledgement */
    }
}

/* The knobs' running values: from LIVE's until the first move. */
static void rec_knobs_read(song_view* sv) {
    if (!sv->rec_knobs) {
        sv->rec_cut = (uint16_t)live_param_get(sv->l, LIVE_PARAM_CUTOFF);
        sv->rec_res = (uint8_t)live_param_get(sv->l, LIVE_PARAM_RESONANCE);
        sv->rec_knobs = 1;
    }
}

uint32_t song_view_rec_value(const song_view* sv, uint32_t knob) {
    if (sv->rec_knobs) {
        return knob ? sv->rec_res : sv->rec_cut;
    }
    return live_param_get(sv->l, knob ? LIVE_PARAM_RESONANCE : LIVE_PARAM_CUTOFF);
}

int song_view_record_knob(song_view* sv, screen* s, uint32_t knob, int32_t steps) {
    uint32_t pattern, row;
    if (!song_view_recording(sv) || !rec_place(sv, 0, &pattern, &row) || !song_view_recording(sv)) {
        return 0;
    }
    rec_knobs_read(sv);
    int32_t v = knob ? (int32_t)sv->rec_res + steps : (int32_t)sv->rec_cut + steps * 32;
    int32_t max = knob ? 15 : 2047;
    v = v < 0 ? 0 : v > max ? max : v;
    int r = song_lock_put((gt2_song*)sv->song, pattern, row,
                          knob ? SONG_LOCK_RESONANCE : SONG_LOCK_CUTOFF, (uint32_t)v);
    if (r != 0) {
        rec_toast(sv, s, r == -(int)GT2_ERR_SAVING ? "SAVING" : r == -(int)GT2_ERR_PENDING ? "WAITING" :
                      r == -(int)GT2_ERR_SONG_FULL ? "SONG FULL" : "LOCKS FULL");
        return 0;
    }
    if (knob) {
        sv->rec_res = (uint8_t)v;
    } else {
        sv->rec_cut = (uint16_t)v;
    }
    return 1;
}

void song_view_frame(song_view* sv, int shows) {
    const song_transport* t = sv->t;
    if (!t || (!shows && sv->want == WANT_NONE && !sv->jam_off &&
               song_view_mute_mask(sv) == sv->sent && !sv->edit_pending && !sv->rec_n)) {
        return; /* outside the song views, most frames: nothing waits */
    }
    send_play(sv);
    if (shows && sv->want == WANT_NONE) {
        t->status(t->ctx, &sv->st);
        sv->song_edits = sv->st.applied;
        int on = sv->st.state == SONG_T_PLAYING;
        if (on && !sv->was_playing && sv->rec && sv->quant == SONG_VIEW_QUANT_COUNT) {
            count_in(sv); /* a play that starts with REC on */
        } else if (!on) {
            sv->counting = 0;
        }
        sv->was_playing = (uint8_t)on;
    }
    send_mutes(sv);
    send_jam_off(sv);
    if (sv->edit_pending) {
        edit_poll(sv);
    }
    if (sv->rec_n) {
        song_view_rec_drain(sv);
    }
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
