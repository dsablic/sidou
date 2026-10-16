// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_view.h"

#include <stddef.h>

#include "font.h"
#include "live_view.h"

#define HEADER GRID_ATTR(GRID_BLUE, GRID_LIGHT_BLUE)
#define REC_ATTR GRID_ATTR(GRID_RED, GRID_LIGHT_BLUE)
/* The TRACKER's ground (the prototype's): black, the bar light blue on blue. */
#define GROUND GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK)

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
/* A pattern's rows at most (GoatTracker's). */
#define MAX_ROWS 128u

static const char HEX[] = "0123456789ABCDEF";

/* ---- the song views, SEQ's order ---- */

/* NULL: not made yet (SEQ skips it). */
static const view* const VIEWS[SONG_VIEW_COUNT] = {NULL, &song_view_tracker, NULL, NULL, NULL};

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
    sv->l = l;
    sv->st.state = SONG_T_IDLE;
    sv->st.multispeed = 1;
    sv->st.ntsc = 0;
    sv->st.mute = 0;
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
    sv->which = SONG_VIEW_TRACKER;
    sv->built = 0;
    sv->copying = 0;
    sv->cursor = 0;
    sv->held = 0;
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
    sv->sent = 0xffu;
    sv->want = WANT_NONE;
    sv->jam_off = 0;
    sv->jam_key = SONG_VIEW_NO_KEY;
    if (t) {
        t->status(t->ctx, &sv->st);
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

void song_view_frame(song_view* sv) {
    const song_transport* t = sv->t;
    if (!t) {
        return;
    }
    send_play(sv);
    if (sv->want == WANT_NONE) {
        t->status(t->ctx, &sv->st);
    }
    send_mutes(sv);
    send_jam_off(sv);
}

/* ---- the header and the heads ---- */

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

static void row_text(uint8_t ch[GRID_COLS], uint8_t attr[GRID_COLS], uint32_t col,
                     const char* s, uint8_t a) {
    for (; *s && col < GRID_COLS; s++, col++) {
        ch[col] = (uint8_t)*s;
        attr[col] = a;
    }
}

static void hex2(char* out, uint32_t v) {
    out[0] = HEX[v >> 4 & 15u];
    out[1] = HEX[v & 15u];
}

/* The position the header shows: the player's while the song plays and a row event said where,
 * the cursor's otherwise. */
static void position(const song_view* sv, uint32_t* order_pos, uint32_t* row) {
    uint32_t v = sv->voice;
    if (going(sv->st.state) && sv->st.row[v] != SONG_T_NOWHERE) {
        *order_pos = sv->st.order_pos[v];
        *row = sv->st.row[v];
    } else {
        *order_pos = sv->order_pos;
        *row = sv->row;
    }
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

void song_view_header(const song_view* sv, const char* name, uint8_t ch[GRID_COLS],
                      uint8_t attr[GRID_COLS]) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        ch[c] = ' ';
        attr[c] = HEADER;
    }
    for (uint32_t c = 0; name[c] && c < SONG_VIEW_NAME_CHARS; c++) {
        ch[c] = (uint8_t)name[c];
    }
    ch[7] = going(sv->st.state) ? FONT_PLAY : FONT_SQUARE;
    uint32_t bpm = shown_bpm(sv);
    char t[5] = " ---";
    if (bpm != 0xffffu) {
        bpm = bpm > 9999u ? 9999u : bpm;
        for (int i = 3; i >= 0; i--) {
            t[i] = (char)(bpm || i == 3 ? '0' + bpm % 10u : ' ');
            bpm /= 10u;
        }
    }
    row_text(ch, attr, 8, t, HEADER);
    uint32_t pos, row;
    position(sv, &pos, &row);
    char p[14] = "ORD 00 ROW 00";
    hex2(&p[4], pos);
    hex2(&p[11], row);
    row_text(ch, attr, 13, p, HEADER);
    if (sv->rec) {
        row_text(ch, attr, 27, "REC", REC_ATTR);
    }
}

static const char* wave3(uint8_t control) {
    switch (control >> 4) {
    case 0: return "---";
    case 1: return "TRI";
    case 2: return "SAW";
    case 4: return "PUL";
    case 8: return "NOI";
    default: return "MIX";
    }
}

/* Voice v's head: its label's colour, what its name says (0 the waveform, 1 MUT, 2 SOL) and
 * its meter, 0..8. */
typedef struct {
    uint8_t color, says, meter;
} head;

static head head_of(const song_view* sv, uint32_t v) {
    head h;
    uint8_t ctrl = sv->st.wave[v];
    h.color = (song_view_mute_mask(sv) >> v & 1u) ? GRID_GREY : live_view_wave_color(ctrl);
    h.says = (uint8_t)(sv->solo >> v & 1u ? 2u : sv->mute >> v & 1u ? 1u : 0u);
    h.meter = (uint8_t)(((uint32_t)sv->st.env[v] * 8u + 127u) / 255u);
    return h;
}

void song_view_heads(const song_view* sv, uint8_t bg, uint8_t ch[GRID_COLS],
                     uint8_t attr[GRID_COLS]) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        ch[c] = ' ';
        attr[c] = GRID_ATTR(GRID_LIGHT_BLUE, bg);
    }
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        uint32_t x = SONG_VIEW_HEAD_COL + SONG_VIEW_HEAD_W * v;
        head h = head_of(sv, v);
        uint8_t wc = live_view_wave_color(sv->st.wave[v]);
        char label[3] = {'V', (char)('1' + v), 0};
        row_text(ch, attr, x, label, GRID_ATTR(h.color, bg));
        if (h.says == 1u) {
            row_text(ch, attr, x + 3u, "MUT", GRID_ATTR(GRID_LIGHT_RED, bg));
        } else if (h.says == 2u) {
            row_text(ch, attr, x + 3u, "SOL", GRID_ATTR(GRID_WHITE, bg));
        } else {
            row_text(ch, attr, x + 3u, wave3(sv->st.wave[v]), GRID_ATTR(wc, bg));
        }
        ch[x + 7u] = (uint8_t)(FONT_V0 + h.meter);
        attr[x + 7u] = GRID_ATTR(wc, GRID_DARK_GREY);
    }
}

/* ---- the TRACKER placeholder (M2d Task 2 replaces it) ---- */

/* The row the rows show: the player's while it plays, the cursor's otherwise; and the window's
 * first row (may be negative: blank rows above row 0). */
static uint32_t shown_row(const song_view* sv) {
    uint32_t pos, row;
    position(sv, &pos, &row);
    return row;
}

static int32_t window_top(uint32_t row) {
    return (int32_t)(row / SONG_VIEW_PAGE * SONG_VIEW_PAGE) - (int32_t)SONG_VIEW_CONTEXT;
}

static void look(const song_view* sv, uint8_t out[SONG_VIEW_LOOK]) {
    uint32_t n = 0;
    uint32_t pos, row;
    position(sv, &pos, &row);
    uint32_t bpm = shown_bpm(sv);
    out[n++] = sv->which;
    out[n++] = (uint8_t)going(sv->st.state);
    out[n++] = (uint8_t)bpm;
    out[n++] = (uint8_t)(bpm >> 8);
    out[n++] = (uint8_t)pos;
    out[n++] = (uint8_t)row;
    out[n++] = sv->rec;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        head h = head_of(sv, v);
        out[n++] = (uint8_t)(h.color | live_view_wave_color(sv->st.wave[v]) << 4);
        out[n++] = (uint8_t)(h.says | (sv->st.wave[v] & 0xf0u));
        out[n++] = h.meter;
    }
    out[n++] = (uint8_t)(window_top(row) + (int32_t)SONG_VIEW_CONTEXT);
    out[n] = (uint8_t)row;
}

static int looks_changed(const song_view* sv) {
    uint8_t now[SONG_VIEW_LOOK];
    look(sv, now);
    for (uint32_t i = 0; i < SONG_VIEW_LOOK; i++) {
        if (now[i] != sv->drawn[i]) {
            return 1;
        }
    }
    return 0;
}

static void build_tracker(song_view* sv) {
    song_view_canvas* c = &sv->cv;
    look(sv, sv->drawn);
    sv->built = 1;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        for (uint32_t k = 0; k < GRID_COLS; k++) {
            c->ch[r][k] = ' ';
            c->attr[r][k] = GROUND;
        }
    }
    song_view_header(sv, "TRACKER", c->ch[0], c->attr[0]);
    song_view_heads(sv, GRID_BLACK, c->ch[1], c->attr[1]);
    uint32_t bar = shown_row(sv);
    int32_t top = window_top(bar);
    for (uint32_t j = 0; j < SONG_VIEW_ROWS; j++) {
        int32_t rn = top + (int32_t)j;
        uint32_t r = SONG_VIEW_ROWS_TOP + j;
        if (rn < 0 || rn >= (int32_t)MAX_ROWS) {
            continue;
        }
        uint32_t u = (uint32_t)rn;
        uint8_t bg = u == bar ? GRID_BLUE : GRID_BLACK;
        uint8_t fg = u == bar           ? GRID_WHITE
                     : u % 16u == 0u    ? GRID_YELLOW
                     : u % 4u == 0u     ? GRID_LIGHT_GREY
                                        : GRID_GREY;
        for (uint32_t k = 0; k + 1u < GRID_COLS; k++) {
            c->attr[r][k] = GRID_ATTR(GRID_LIGHT_BLUE, bg);
        }
        c->ch[r][0] = (uint8_t)HEX[u >> 4 & 15u];
        c->ch[r][1] = (uint8_t)HEX[u & 15u];
        c->attr[r][0] = GRID_ATTR(fg, bg);
        c->attr[r][1] = GRID_ATTR(fg, bg);
    }
}

/* Built anew at its first compose: what changed while it was away. */
static void tracker_enter(screen* s, void* state) {
    (void)s;
    song_view* sv = state;
    sv->which = SONG_VIEW_TRACKER;
    sv->built = 0;
    sv->cursor = 0;
}

static int tracker_changed(screen* s, void* state) {
    (void)s;
    const song_view* sv = state;
    return !sv->built || sv->copying || looks_changed(sv);
}

/* 1 for a cell of the screen's outer ring. */
static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

/* The canvas onto the grid as CHIP copies its own (app/chip_view.c): under a wipe the rows it
 * reveals whole; otherwise the changed cells from where the last frame stopped, within the
 * frame's budget, leaving alone what is covered this frame and the border under the raster. */
static void copy(song_view* sv, screen* s, grid* g) {
    const song_view_canvas* c = &sv->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_row(g, r, c->ch[r], c->attr[r]);
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < SONG_VIEW_FRAME_CELLS ? SONG_VIEW_FRAME_CELLS - used : 0u;
    uint32_t i = sv->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint32_t r = i / GRID_COLS;
        uint32_t k = i % GRID_COLS;
        if ((whole >> r & 1u) || (g->ch[i] == c->ch[r][k] && g->attr[i] == c->attr[r][k]) ||
            screen_covered(s, k, r) || (raster && border(k, r))) {
            continue;
        }
        if (!left) {
            sv->cursor = (uint16_t)i;
            sv->held++;
            sv->copying = 1;
            return;
        }
        grid_put(g, k, r, c->ch[r][k], c->attr[r][k]);
        left--;
    }
    sv->copying = 0;
}

static void tracker_compose(screen* s, void* state, grid* g) {
    song_view* sv = state;
    if (!sv->built || looks_changed(sv)) {
        build_tracker(sv);
    }
    copy(sv, s, g);
}

const view song_view_tracker = {"TRACKER", tracker_enter, NULL, NULL, tracker_compose,
                                tracker_changed};
