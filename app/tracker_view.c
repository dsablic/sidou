// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "tracker_view.h"

#include <stddef.h>

#include "font.h"
#include "song/gt2_song.h"

#define BLACK_BG GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK)
#define RULE GRID_ATTR(GRID_DARK_GREY, GRID_BLACK)
#define RULE_ROW 27u
#define ORDER_ROW 28u
#define SAY_ROW 29u
#define POS_COL 29u
/* What the rows show, as bytes (song_view.drawn from SONG_VIEW_HEAD_LOOK): the mode, the
 * window's top (2), the bar's row, each voice's pattern, entry and silence, the song's edit
 * count (4). */
#define ROWS_LOOK (5u + 3u * SONG_T_VOICES + 4u)
_Static_assert(SONG_VIEW_HEAD_LOOK + ROWS_LOOK <= SONG_VIEW_LOOK, "the looks fit");

static const char HEX[] = "0123456789ABCDEF";
static const char NOTE_NAMES[12][3] = {"C-", "C#", "D-", "D#", "E-", "F-",
                                       "F#", "G-", "G#", "A-", "A#", "B-"};
/* GoatTracker 2's readme, 4.2: the pattern commands. */
static const char* const COMMANDS[16] = {"",
                                         "PORTAMENTO UP",
                                         "PORTAMENTO DOWN",
                                         "TONEPORTAMENTO",
                                         "VIBRATO",
                                         "SET ATTACK/DECAY",
                                         "SET SUSTAIN/RELEASE",
                                         "SET WAVEFORM",
                                         "WAVETABLE POINTER",
                                         "PULSETABLE POINTER",
                                         "FILTERTABLE POINTER",
                                         "FILTER CONTROL",
                                         "FILTER CUTOFF",
                                         "MASTER VOLUME",
                                         "FUNKTEMPO",
                                         "SET TEMPO"};

const char* tracker_view_command_name(uint32_t cmd) {
    return COMMANDS[cmd & 15u];
}

int32_t tracker_view_top(uint32_t row, int scroll) {
    if (scroll) {
        return (int32_t)row - (int32_t)TRACKER_VIEW_SCROLL_ROW;
    }
    return (int32_t)(row / TRACKER_VIEW_PAGE * TRACKER_VIEW_PAGE) - (int32_t)TRACKER_VIEW_CONTEXT;
}

void tracker_view_cell(const uint8_t cell[4], char out[9]) {
    uint8_t n = cell[0];
    if (n >= GT2_FIRSTNOTE && n <= GT2_LASTNOTE) {
        uint32_t v = n - GT2_FIRSTNOTE;
        out[0] = NOTE_NAMES[v % 12u][0];
        out[1] = NOTE_NAMES[v % 12u][1];
        out[2] = (char)('0' + v / 12u);
    } else {
        char c = n == GT2_KEYOFF ? '=' : n == GT2_KEYON ? '+' : '-';
        out[0] = out[1] = out[2] = c;
    }
    if (cell[1]) {
        out[3] = HEX[cell[1] >> 4 & 15u];
        out[4] = HEX[cell[1] & 15u];
    } else {
        out[3] = out[4] = '.';
    }
    if (cell[2]) {
        out[5] = HEX[cell[2] & 15u];
        out[6] = HEX[cell[3] >> 4];
        out[7] = HEX[cell[3] & 15u];
    } else {
        out[5] = out[6] = out[7] = '.';
    }
    out[8] = 0;
}

void tracker_view_order_entry(uint8_t e, char out[3]) {
    if (e < GT2_REPEAT) {
        out[0] = HEX[e >> 4];
        out[1] = HEX[e & 15u];
    } else if (e < GT2_TRANSDOWN) {
        out[0] = 'R';
        out[1] = HEX[e & 15u]; /* R0: 16 times */
    } else if (e < GT2_TRANSUP) {
        out[0] = '-';
        out[1] = HEX[(GT2_TRANSUP - e) & 15u];
    } else {
        out[0] = '+';
        out[1] = HEX[(e - GT2_TRANSUP) & 15u];
    }
    out[2] = 0;
}

/* ---- what each voice shows ---- */

typedef struct {
    uint8_t pattern; /* 0xFF: none */
    uint8_t entry;   /* its orderlist entry */
    uint8_t row;     /* the row it is on */
    uint8_t silent;
} lane;

/* Voice v's orderlist entry and row (app/song_view.h song_view_position), and the first
 * pattern at or after the entry. */
static lane lane_of(const song_view* sv, uint32_t v) {
    lane ln = {0xffu, 0xffu, 0u, 0u};
    uint32_t entry, row;
    song_view_position(sv, v, &entry, &row);
    ln.row = (uint8_t)row;
    ln.silent = (uint8_t)(song_view_mute_mask(sv) >> v & 1u);
    const gt2_song* s = sv->song;
    if (!s || entry == SONG_T_NOWHERE) {
        return ln;
    }
    const uint8_t* o = gt2_order_data(s, 0, v);
    uint32_t n = gt2_order_entries(s, 0, v);
    for (uint32_t i = entry; o && i < n; i++) {
        if (o[i] < GT2_REPEAT) {
            ln.pattern = o[i];
            ln.entry = (uint8_t)i;
            break;
        }
    }
    return ln;
}

/* A pattern's rows (the end row not counted), 0 for none. */
static uint32_t rows_of(const gt2_song* s, uint32_t pattern) {
    return s && pattern != 0xffu ? gt2_pattern_rows(s, pattern) - 1u : 0u;
}

static void rows_look(const song_view* sv, uint8_t out[ROWS_LOOK]) {
    uint32_t n = 0;
    lane bar = lane_of(sv, sv->voice);
    int32_t top = tracker_view_top(bar.row, sv->scroll);
    out[n++] = sv->scroll;
    out[n++] = (uint8_t)top;
    out[n++] = (uint8_t)((uint32_t)top >> 8);
    out[n++] = bar.row;
    out[n++] = (uint8_t)(sv->song != NULL);
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lane ln = lane_of(sv, v);
        out[n++] = ln.pattern;
        out[n++] = ln.entry;
        out[n++] = ln.silent;
    }
    for (uint32_t i = 0; i < 4u; i++) {
        out[n++] = (uint8_t)(sv->song_edits >> (8u * i));
    }
}

/* ---- the canvas ---- */

static void text(song_view_canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = a;
    }
}

/* One voice's cells on screen row r for its pattern row pr (bg: the bar's or black). */
static void voice_cells(song_view_canvas* c, uint32_t r, uint32_t x, const uint8_t* cell, int on,
                        int silent) {
    char t[9];
    tracker_view_cell(cell, t);
    uint8_t bg = on ? GRID_BLUE : GRID_BLACK;
    uint8_t dim = on ? GRID_LIGHT_BLUE : GRID_DARK_GREY;
    uint8_t note = t[0] == '-' ? dim : t[0] == '=' || t[0] == '+' ? (on ? GRID_LIGHT_GREY : GRID_GREY)
                                                                  : GRID_WHITE;
    uint8_t ins = cell[1] ? (on ? GRID_CYAN : GRID_LIGHT_BLUE) : dim;
    uint8_t cmd = cell[2] ? GRID_YELLOW : dim;
    uint8_t par = cell[2] ? GRID_LIGHT_GREEN : dim;
    if (silent) {
        note = ins = cmd = par = GRID_DARK_GREY;
    }
    for (uint32_t i = 0; i < 8u; i++) {
        uint8_t fg = i < 3u ? note : i < 5u ? ins : i == 5u ? cmd : par;
        c->ch[r][x + i] = (uint8_t)t[i];
        c->attr[r][x + i] = GRID_ATTR(fg, bg);
    }
}

static void build_rows(song_view* sv) {
    song_view_canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    lane lanes[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lanes[v] = lane_of(sv, v);
    }
    lane bar = lanes[sv->voice];
    uint32_t bar_rows = rows_of(s, bar.pattern);
    uint32_t shown = s ? bar_rows : GT2_MAX_ROWS; /* with no song, the numbers alone */
    int32_t top = tracker_view_top(bar.row, sv->scroll);
    for (uint32_t r = TRACKER_VIEW_TOP; r < GRID_ROWS; r++) {
        for (uint32_t k = 0; k < GRID_COLS; k++) {
            c->ch[r][k] = ' ';
            c->attr[r][k] = BLACK_BG;
        }
    }
    for (uint32_t j = 0; j < TRACKER_VIEW_ROWS; j++) {
        int32_t pr = top + (int32_t)j;
        uint32_t r = TRACKER_VIEW_TOP + j;
        if (pr < 0) {
            continue;
        }
        uint32_t u = (uint32_t)pr;
        int on = u == bar.row;
        uint8_t bg = on ? GRID_BLUE : GRID_BLACK;
        if (on) {
            for (uint32_t k = 0; k < POS_COL; k++) {
                c->attr[r][k] = GRID_ATTR(GRID_LIGHT_BLUE, bg);
            }
        }
        if (u < shown) {
            uint8_t fg = on              ? GRID_WHITE
                         : u % 16u == 0u ? GRID_YELLOW
                         : u % 4u == 0u  ? GRID_LIGHT_GREY
                                         : GRID_GREY;
            c->ch[r][0] = (uint8_t)HEX[u >> 4 & 15u];
            c->ch[r][1] = (uint8_t)HEX[u & 15u];
            c->attr[r][0] = c->attr[r][1] = GRID_ATTR(fg, bg);
        }
        for (uint32_t v = 0; s && v < SONG_T_VOICES; v++) {
            uint32_t n = rows_of(s, lanes[v].pattern);
            if (u >= n) {
                continue;
            }
            static const uint8_t EMPTY[4] = {GT2_REST, 0, 0, 0};
            const uint8_t* d = gt2_pattern_data(s, lanes[v].pattern);
            const uint8_t* cell = d ? d + u * GT2_ROW_BYTES : EMPTY;
            voice_cells(c, r, TRACKER_VIEW_COL + TRACKER_VIEW_W * v, cell, on, lanes[v].silent);
        }
    }
    /* column 29: the bar's place in its pattern */
    uint32_t pip = bar_rows ? bar.row * TRACKER_VIEW_ROWS / bar_rows : TRACKER_VIEW_ROWS;
    for (uint32_t j = 0; j < TRACKER_VIEW_ROWS; j++) {
        c->ch[TRACKER_VIEW_TOP + j][POS_COL] = j == pip ? FONT_PIP : FONT_BOX_V;
        c->attr[TRACKER_VIEW_TOP + j][POS_COL] =
            j == pip ? GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK) : RULE;
    }
    for (uint32_t k = 0; k < GRID_COLS; k++) {
        c->ch[RULE_ROW][k] = FONT_BOX_H;
        c->attr[RULE_ROW][k] = RULE;
    }
    /* row 28: the orderlists from each voice's entry */
    text(c, 0, ORDER_ROW, "OL", GRID_ATTR(GRID_GREY, GRID_BLACK));
    for (uint32_t v = 0; s && v < SONG_T_VOICES; v++) {
        const uint8_t* o = gt2_order_data(s, 0, v);
        uint32_t n = gt2_order_entries(s, 0, v);
        if (!o || lanes[v].pattern == 0xffu) {
            continue;
        }
        uint32_t x = TRACKER_VIEW_COL + TRACKER_VIEW_W * v;
        for (uint32_t t = 0, i = lanes[v].entry; t < 3u; t++, i++) {
            char e[6];
            if (i >= n) {
                if (t < 2u) { /* RST and the restart position: five cells */
                    e[0] = 'R', e[1] = 'S', e[2] = 'T';
                    e[3] = HEX[o[n + 1u] >> 4];
                    e[4] = HEX[o[n + 1u] & 15u];
                    e[5] = 0;
                    text(c, x + 3u * t, ORDER_ROW, e, GRID_ATTR(GRID_GREY, GRID_BLACK));
                }
                break;
            }
            tracker_view_order_entry(o[i], e);
            text(c, x + 3u * t, ORDER_ROW, e,
                 t == 0u ? GRID_ATTR(GRID_WHITE, GRID_BLUE) : GRID_ATTR(GRID_GREY, GRID_BLACK));
        }
    }
    /* row 29: the bar row's first command, of the voices that sound */
    for (uint32_t v = 0; s && v < SONG_T_VOICES; v++) {
        const uint8_t* d = gt2_pattern_data(s, lanes[v].pattern);
        if (lanes[v].silent || !d || bar.row >= rows_of(s, lanes[v].pattern)) {
            continue;
        }
        const uint8_t* cell = d + (uint32_t)bar.row * GT2_ROW_BYTES;
        if (!cell[2]) {
            continue;
        }
        char say[9] = "V1 4 22 ";
        say[1] = (char)('1' + v);
        say[3] = HEX[cell[2] & 15u];
        say[5] = HEX[cell[3] >> 4];
        say[6] = HEX[cell[3] & 15u];
        uint8_t a = GRID_ATTR(GRID_YELLOW, GRID_BLACK);
        text(c, 0, SAY_ROW, say, a);
        text(c, 8, SAY_ROW, tracker_view_command_name(cell[2]), a);
        break;
    }
}

/* Built anew at its first compose: what changed while it was away. */
static void enter(screen* s, void* state) {
    (void)s;
    song_view* sv = state;
    sv->which = SONG_VIEW_TRACKER;
    sv->built = 0;
}

static int looks_changed(const song_view* sv) {
    uint8_t head[SONG_VIEW_HEAD_LOOK], rows[ROWS_LOOK];
    song_view_head_look(sv, head);
    rows_look(sv, rows);
    for (uint32_t i = 0; i < SONG_VIEW_HEAD_LOOK; i++) {
        if (head[i] != sv->drawn[i]) {
            return 1;
        }
    }
    for (uint32_t i = 0; i < ROWS_LOOK; i++) {
        if (rows[i] != sv->drawn[SONG_VIEW_HEAD_LOOK + i]) {
            return 1;
        }
    }
    return 0;
}

static int changed(screen* s, void* state) {
    (void)s;
    const song_view* sv = state;
    return !sv->built || sv->dirty || looks_changed(sv) || sv->which != SONG_VIEW_TRACKER;
}

static void compose(screen* s, void* state, grid* g) {
    song_view* sv = state;
    uint8_t head[SONG_VIEW_HEAD_LOOK], rows[ROWS_LOOK];
    song_view_head_look(sv, head);
    rows_look(sv, rows);
    int fresh = !sv->built || sv->which != SONG_VIEW_TRACKER;
    int head_new = fresh, rows_new = fresh;
    for (uint32_t i = 0; i < SONG_VIEW_HEAD_LOOK; i++) {
        head_new |= head[i] != sv->drawn[i];
    }
    for (uint32_t i = 0; i < ROWS_LOOK; i++) {
        rows_new |= rows[i] != sv->drawn[SONG_VIEW_HEAD_LOOK + i];
    }
    if (head_new) {
        song_view_header(sv, "TRACKER", sv->cv.ch[0], sv->cv.attr[0]);
        song_view_heads(sv, GRID_BLACK, sv->cv.ch[1], sv->cv.attr[1]);
        sv->dirty |= 3u;
        for (uint32_t i = 0; i < SONG_VIEW_HEAD_LOOK; i++) {
            sv->drawn[i] = head[i];
        }
    }
    if (rows_new) {
        build_rows(sv);
        sv->dirty |= ((1u << GRID_ROWS) - 1u) & ~3u;
        for (uint32_t i = 0; i < ROWS_LOOK; i++) {
            sv->drawn[SONG_VIEW_HEAD_LOOK + i] = rows[i];
        }
    }
    sv->which = SONG_VIEW_TRACKER;
    sv->built = 1;
    song_view_copy(sv, s, g);
}

const view tracker_view_view = {"TRACKER", enter, NULL, NULL, compose, changed};
