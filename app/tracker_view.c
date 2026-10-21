// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "tracker_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "song/gt2_song.h"

/* What the header shows, as bytes (song_view.drawn): the play state, the tempo (2), REC, the shown
 * voice, the mutes and the solos, its pattern, the theme; then what the rows show: the mode, the
 * window's top (2), the bar's row, whether there is a song, the shown voice, the theme, the
 * orderlist position, each voice's pattern, entry and silence, the song's edit count (4). */
#define HEAD_LOOK 8u
#define ROWS_LOOK (8u + 3u * SONG_T_VOICES + 4u + 3u)
_Static_assert(HEAD_LOOK + ROWS_LOOK <= SONG_VIEW_LOOK, "the looks fit");
/* The rows' look bytes the bar's step changes alone (rows_look): the bar's row, the position. */
#define ROWS_BAR 3u
#define ROWS_POS 7u
#define ROWS_BAR_BIT (1u << ROWS_BAR)
#define ROWS_POS_BIT (1u << ROWS_POS)

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

void tracker_view_order_entry(uint8_t e, char out[4]) {
    if (e == GT2_TRANSDOWN) {
        memcpy(out, "-10", 4u);
        return;
    }
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

static uint32_t subtune_of(const song_view* sv, uint32_t v) {
    return song_view_playing(sv) && !(sv->rec && v == sv->voice) ? sv->st.subtune : sv->subtune;
}

/* Voice v's orderlist entry and row (app/song_view.h song_view_position), and the first
 * pattern at or after the entry. */
static lane lane_of(const song_view* sv, uint32_t v) {
    lane ln = {0xffu, 0xffu, 0u, 0u};
    uint32_t entry, row;
    song_view_position(sv, v, &entry, &row);
    if (sv->rec && v == sv->voice) {
        entry = sv->order_pos;
        row = sv->row;
    }
    ln.row = (uint8_t)row;
    ln.silent = (uint8_t)(song_view_mute_mask(sv) >> v & 1u);
    const gt2_song* s = sv->song;
    if (!s || entry == SONG_T_NOWHERE) {
        return ln;
    }
    uint32_t sub = subtune_of(sv, v);
    const uint8_t* o = gt2_order_data(s, sub, v);
    uint32_t n = gt2_order_entries(s, sub, v);
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

static void head_look(const song_view* sv, uint8_t out[HEAD_LOOK]) {
    uint32_t bpm = song_view_shown_bpm(sv);
    out[0] = (uint8_t)song_view_playing(sv);
    out[1] = (uint8_t)bpm;
    out[2] = (uint8_t)(bpm >> 8);
    out[3] = (uint8_t)(sv->rec | (sv->io && song_io_saving(sv->io) ? 2u : 0u));
    out[4] = sv->voice;
    out[5] = (uint8_t)(sv->mute | sv->solo << 3);
    out[6] = lane_of(sv, sv->voice).pattern;
    out[7] = sv->l->theme;
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
    out[n++] = sv->voice;
    out[n++] = sv->l->theme;
    uint32_t pos, row;
    song_view_position(sv, sv->voice, &pos, &row);
    out[n++] = sv->rec ? sv->order_pos : (uint8_t)pos;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lane ln = lane_of(sv, v);
        out[n++] = ln.pattern;
        out[n++] = ln.entry;
        out[n++] = ln.silent;
    }
    for (uint32_t i = 0; i < 4u; i++) {
        out[n++] = (uint8_t)(sv->song_edits >> (8u * i));
    }
    out[n++] = sv->rec;
    out[n++] = sv->field;
    out[n++] = (uint8_t)subtune_of(sv, sv->voice);
}

/* ---- the canvas ---- */

typedef song_view_canvas canvas;

static void small(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = a;
    }
}

static void big(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    (void)grid_canvas_text_big(c, col, row, 2, s, a);
}

static void blank(canvas* c, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t a) {
    for (uint32_t r = row; r < row + h; r++) {
        memset(&c->ch[r][col], ' ', w);
        memset(&c->attr[r][col], a, w);
    }
}

/* The colour of voice v's number and strip: its own, grey while the song silences it. */
static uint8_t voice_color(const song_view* sv, const theme* th, uint32_t v) {
    return (song_view_mute_mask(sv) >> v & 1u) ? th->dim : th->voice[v];
}

/* The header, rows 0-2. */
static void build_head(song_view* sv) {
    canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    const uint8_t base = THEME_ATTR(th, value);
    blank(c, 0, 0, GRID_COLS, TRACKER_VIEW_TOP, base);
    uint32_t v = sv->voice;
    char vn[3] = {'V', (char)('1' + v), 0};
    big(c, 0, 0, vn, GRID_ATTR(voice_color(sv, th, v), th->bg));
    lane ln = lane_of(sv, v);
    char pat[7] = "PAT --";
    if (ln.pattern != 0xffu) {
        pat[4] = HEX[ln.pattern >> 4 & 15u];
        pat[5] = HEX[ln.pattern & 15u];
    }
    big(c, 5, 0, sv->io && song_io_saving(sv->io) ? "SAVING" : pat, base);
    /* MUT or SOL small by the pattern; the play mark, the tempo and REC on the right. */
    if (sv->solo >> v & 1u) {
        small(c, 18, 0, "SOL", base);
    } else if (sv->mute >> v & 1u) {
        small(c, 18, 0, "MUT", THEME_ATTR(th, alert));
    }
    c->ch[0][22] = song_view_playing(sv) ? FONT_PLAY : FONT_SQUARE;
    c->attr[0][22] = base;
    uint32_t bpm = song_view_shown_bpm(sv);
    char t[5] = " ---";
    if (bpm != SONG_VIEW_NO_BPM) {
        bpm = bpm > 9999u ? 9999u : bpm;
        for (int i = 3; i >= 0; i--) {
            t[i] = (char)(bpm || i == 3 ? '0' + bpm % 10u : ' ');
            bpm /= 10u;
        }
    }
    small(c, 26, 0, t, THEME_ATTR(th, label));
    small(c, 26, 1, "BPM", THEME_ATTR(th, dim));
    if (sv->rec) {
        small(c, 22, 1, "REC", THEME_ATTR(th, alert));
    }
    /* The other voices' numbers over their strips. */
    uint32_t col = TRACKER_VIEW_STRIP_COL;
    for (uint32_t w = 0; w < SONG_T_VOICES; w++) {
        if (w != v) {
            char d[2] = {(char)('1' + w), 0};
            small(c, col, TRACKER_VIEW_TAG_ROW, d, GRID_ATTR(voice_color(sv, th, w), th->bg));
            col += TRACKER_VIEW_STRIP_STEP;
        }
    }
}

/* One pattern row, its two cell rows from screen row r: u is the row's number, `cell` its four
 * bytes (NULL: none, a pattern shorter than the window), `on` the current row (the bar behind it),
 * `field` the field to highlight (TRACKER_VIEW_FIELD_*, an edit cursor's; NONE for the bar
 * alone). A muted voice's rows keep their colours: the header and the strips say it (a recolour of
 * a whole voice's cells is what a mute would otherwise send). */
static void draw_row(canvas* c, const theme* th, uint32_t r, uint32_t voice, uint32_t u,
                     const uint8_t* cell, int on, uint32_t field) {
    uint8_t bg = on ? th->cursor : th->bg;
    blank(c, 0, r, TRACKER_VIEW_BAND_W, TRACKER_VIEW_ROW_CELLS, GRID_ATTR(th->value, bg));
    blank(c, TRACKER_VIEW_BAND_W, r, GRID_COLS - TRACKER_VIEW_BAND_W, TRACKER_VIEW_ROW_CELLS,
          THEME_ATTR(th, value));
    char num[3] = {HEX[u >> 4 & 15u], HEX[u & 15u], 0};
    small(c, TRACKER_VIEW_NUM_COL, r + 1u, num,
          field ? GRID_ATTR(th->bg, th->hot) : GRID_ATTR(on ? th->hot : th->label, bg));
    if (!cell) {
        return;
    }
    char t[9];
    tracker_view_cell(cell, t);
    /* Grey rests and blanks (lighter on the band, where dark grey would vanish). */
    uint8_t quiet = on ? th->label : th->dim;
    uint8_t note = t[0] == '-' ? quiet : t[0] == '=' || t[0] == '+' ? th->label
                   : on                                              ? th->hot
                                                                     : th->value;
    uint8_t ins = !cell[1] ? quiet : th->voice[voice];
    uint8_t cmd = !cell[2] ? quiet : th->label;
    uint8_t na = GRID_ATTR(note, field == TRACKER_VIEW_FIELD_NOTE ? th->hot : bg);
    uint8_t ia = GRID_ATTR(ins, field == TRACKER_VIEW_FIELD_INSTRUMENT ? th->hot : bg);
    uint8_t ca = GRID_ATTR(cmd, field == TRACKER_VIEW_FIELD_COMMAND ? th->hot : bg);
    if (field) { /* the highlighted field's text in the band's colour, on the highlight */
        na = field == TRACKER_VIEW_FIELD_NOTE ? GRID_ATTR(th->bg, th->hot) : na;
        ia = field == TRACKER_VIEW_FIELD_INSTRUMENT ? GRID_ATTR(th->bg, th->hot) : ia;
        ca = field == TRACKER_VIEW_FIELD_COMMAND ? GRID_ATTR(th->bg, th->hot) : ca;
    }
    char note3[4] = {t[0], t[1], t[2], 0};
    char ins2[3] = {t[3], t[4], 0};
    big(c, TRACKER_VIEW_NOTE_COL, r, note3, na);
    char cmd3[4] = {t[5], t[6], t[7], 0};
    big(c, TRACKER_VIEW_INS_COL, r, ins2, ia);
    if (field == TRACKER_VIEW_FIELD_COMMAND) {
        big(c, TRACKER_VIEW_CMD_COL, r, cmd3, ca);
    } else {
        small(c, TRACKER_VIEW_CMD_COL, r + 1u, cmd3, ca);
    }
}

/* Voice v's pattern as a strip reads it: its rows (not counting the end row) and its cells. */
typedef struct {
    uint32_t rows;
    const uint8_t* data; /* NULL: none */
} strip_data;

static strip_data strip_of(const gt2_song* s, const lane* ln) {
    strip_data d = {rows_of(s, ln->pattern), NULL};
    if (d.rows) {
        d.data = gt2_pattern_data(s, ln->pattern);
    }
    return d;
}

/* Whether a note sounds on row u of a voice's pattern: its strip's bar is tall. */
static int sounds(const strip_data* d, uint32_t u) {
    if (u >= d->rows) {
        return 0;
    }
    uint8_t n = d->data ? d->data[u * GT2_ROW_BYTES] : (uint8_t)GT2_REST;
    return n >= GT2_FIRSTNOTE && n <= GT2_LASTNOTE;
}

/* One strip bar, bottom-aligned in the 16 pixels of a row: 12 high where a note sounds, 2 where
 * none does; nothing past the end of the voice's pattern. */
static void draw_strip(canvas* c, const theme* th, uint32_t r, uint32_t col, int exists, int tall,
                       uint8_t color) {
    if (!exists) {
        return;
    }
    c->ch[r + 1u][col] = tall ? FONT_V8 : FONT_V2;
    c->attr[r + 1u][col] = GRID_ATTR(color, th->bg);
    if (tall) {
        c->ch[r][col] = FONT_V4;
        c->attr[r][col] = GRID_ATTR(color, th->bg);
    }
}

/* Window row j (the pattern row `top` + j, its two cell rows) and its strips, into the canvas. */
static void draw_window_row(song_view* sv, const theme* th, const lane lanes[SONG_T_VOICES],
                            const strip_data strips[SONG_T_VOICES], int32_t top, uint32_t j) {
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    uint32_t shown_v = sv->voice;
    lane bar = lanes[shown_v];
    uint32_t bar_rows = strips[shown_v].rows;
    uint32_t shown = s ? bar_rows : GT2_MAX_ROWS; /* with no song, the numbers alone */
    int32_t pr = top + (int32_t)j;
    uint32_t r = TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * j;
    if (pr < 0) {
        blank(c, 0, r, GRID_COLS, TRACKER_VIEW_ROW_CELLS, THEME_ATTR(th, value));
        return;
    }
    uint32_t u = (uint32_t)pr;
    int on = u == bar.row;
    const uint8_t* cell = NULL;
    static const uint8_t EMPTY[4] = {GT2_REST, 0, 0, 0};
    if (s && u < bar_rows) {
        const uint8_t* d = strips[shown_v].data;
        cell = d ? d + u * GT2_ROW_BYTES : EMPTY;
    }
    if (u < shown) {
        draw_row(c, th, r, shown_v, u, cell, on,
                 on && sv->rec ? sv->field : TRACKER_VIEW_FIELD_NONE);
    } else {
        blank(c, 0, r, GRID_COLS, TRACKER_VIEW_ROW_CELLS, THEME_ATTR(th, value));
    }
    /* The other two voices as strips. */
    uint32_t col = TRACKER_VIEW_STRIP_COL;
    for (uint32_t w = 0; w < SONG_T_VOICES; w++) {
        if (w == shown_v) {
            continue;
        }
        draw_strip(c, th, r, col, s && u < strips[w].rows, !lanes[w].silent && sounds(&strips[w], u),
                   lanes[w].silent ? th->dim : th->voice[w]);
        col += TRACKER_VIEW_STRIP_STEP;
    }
}

/* Rows 27-29: the shown voice's orderlist from its entry, the current row's first command spelled
 * out, the hint and the position. */
static void build_footer(song_view* sv, const theme* th, const lane lanes[SONG_T_VOICES]) {
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    uint32_t shown_v = sv->voice;
    lane bar = lanes[shown_v];
    blank(c, 0, TRACKER_VIEW_ORDER_ROW, GRID_COLS, GRID_ROWS - TRACKER_VIEW_ORDER_ROW,
          THEME_ATTR(th, value));
    const uint8_t dim = THEME_ATTR(th, dim);
    const uint8_t grey = THEME_ATTR(th, label);
    small(c, 0, TRACKER_VIEW_ORDER_ROW, "OL", dim);
    uint32_t sub = subtune_of(sv, shown_v);
    const uint8_t* o = s ? gt2_order_data(s, sub, shown_v) : NULL;
    uint32_t n = s ? gt2_order_entries(s, sub, shown_v) : 0u;
    if (o && bar.pattern != 0xffu) {
        uint32_t col = 3u;
        for (uint32_t t = 0, i = bar.entry; t < 3u; t++, i++) {
            char e[6];
            if (i >= n) {
                if (t < 2u) { /* RST and the restart position: five cells */
                    e[0] = 'R', e[1] = 'S', e[2] = 'T';
                    e[3] = HEX[o[n + 1u] >> 4];
                    e[4] = HEX[o[n + 1u] & 15u];
                    e[5] = 0;
                    small(c, col, TRACKER_VIEW_ORDER_ROW, e, grey);
                }
                break;
            }
            tracker_view_order_entry(o[i], e);
            small(c, col, TRACKER_VIEW_ORDER_ROW, e,
                  t == 0u ? GRID_ATTR(th->value, th->cursor) : grey);
            col += e[2] ? 4u : 3u;
        }
    }
    /* the current row's first command, of the voices that sound, spelled out */
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
        uint8_t a = THEME_ATTR(th, hot);
        small(c, 0, TRACKER_VIEW_SAY_ROW, say, a);
        small(c, 8, TRACKER_VIEW_SAY_ROW, tracker_view_command_name(cell[2]), a);
        break;
    }
    small(c, 0, TRACKER_VIEW_HINT_ROW, "SELECT: ROW", dim);
    /* the position as the song views have always said it: the orderlist entry and the row, hex */
    uint32_t pos, row;
    song_view_position(sv, shown_v, &pos, &row);
    if (sv->rec) {
        pos = sv->order_pos;
        row = sv->row;
    }
    char at[14] = "ORD 00 ROW 00";
    at[4] = HEX[pos >> 4 & 15u];
    at[5] = HEX[pos & 15u];
    at[11] = HEX[row >> 4 & 15u];
    at[12] = HEX[row & 15u];
    small(c, GRID_COLS - 13u, TRACKER_VIEW_HINT_ROW, at, grey);
}

/* Window rows [from, from + n) into the canvas (marked dirty), the lanes as they are now. */
static void build_window(song_view* sv, uint32_t from, uint32_t n) {
    const theme* th = theme_get(sv->l->theme);
    lane lanes[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lanes[v] = lane_of(sv, v);
    }
    strip_data strips[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        strips[v] = strip_of(sv->song, &lanes[v]);
    }
    int32_t top = tracker_view_top(lanes[sv->voice].row, sv->scroll);
    for (uint32_t j = from; j < from + n && j < TRACKER_VIEW_ROWS; j++) {
        draw_window_row(sv, th, lanes, strips, top, j);
        sv->dirty |= 3u << (TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * j);
        sv->unbuilt &= (uint16_t)~(1u << j);
    }
}

/* The footer, whole, and the window to be built a few rows a frame (see compose). */
static void build_rows(song_view* sv) {
    const theme* th = theme_get(sv->l->theme);
    lane lanes[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lanes[v] = lane_of(sv, v);
    }
    build_footer(sv, th, lanes);
    sv->dirty |= ((1u << (GRID_ROWS - TRACKER_VIEW_ORDER_ROW)) - 1u) << TRACKER_VIEW_ORDER_ROW;
    sv->unbuilt = (uint16_t)((1u << TRACKER_VIEW_ROWS) - 1u);
}

/* What a step of the bar changes when nothing else did (the window stands, in BAR): the row it
 * leaves and the row it reaches, and the footer. Returns the canvas rows touched. */
static uint32_t build_step(song_view* sv, uint32_t from) {
    const theme* th = theme_get(sv->l->theme);
    lane lanes[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lanes[v] = lane_of(sv, v);
    }
    strip_data strips[SONG_T_VOICES];
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        strips[v] = strip_of(sv->song, &lanes[v]);
    }
    int32_t top = tracker_view_top(lanes[sv->voice].row, sv->scroll);
    uint32_t touched = ((1u << (GRID_ROWS - TRACKER_VIEW_ORDER_ROW)) - 1u) << TRACKER_VIEW_ORDER_ROW;
    uint32_t rows[2] = {from, lanes[sv->voice].row};
    for (uint32_t k = 0; k < 2u; k++) {
        int32_t j = (int32_t)rows[k] - top;
        if (j >= 0 && j < (int32_t)TRACKER_VIEW_ROWS) {
            draw_window_row(sv, th, lanes, strips, top, (uint32_t)j);
            touched |= 3u << (TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * (uint32_t)j);
        }
    }
    build_footer(sv, th, lanes);
    return touched;
}

/* Built anew at its first compose: what changed while it was away. */
static void enter(screen* s, void* state) {
    song_view* sv = state;
    sv->built = 0;
    sv->edit_screen = s;
}

/* Left: no rows wait to be built; the next visit builds all of them. */
static void leave(screen* s, void* state) {
    (void)s;
    song_view* sv = state;
    sv->unbuilt = 0;
    sv->built = 0;
}

__attribute__((noinline, unused)) static uint8_t clamp(int32_t value, uint32_t max) {
    return (uint8_t)(value < 0 ? 0 : (uint32_t)value > max ? max : (uint32_t)value);
}

__attribute__((noinline, unused)) static void feedback(screen* s, const song_view* sv) {
    const char* msg = sv->editor && sv->song ? song_edit_message(sv->editor) : "NO SONG";
    if (msg && *msg) {
        screen_toast(s, msg, SCREEN_TOAST_MS);
        s->popup_ask = 0;
    }
}

__attribute__((noinline, unused)) static int read_cell(const song_view* sv, lane* ln, uint8_t cell[4]) {
    *ln = lane_of(sv, sv->voice);
    return sv->editor && song_edit_read_row(sv->editor, ln->pattern, sv->row, cell);
}

__attribute__((noinline, unused)) static int write_cell(screen* s, song_view* sv, const lane* ln,
                                                    const uint8_t cell[4]) {
    int ok = song_edit_row(sv->editor, ln->pattern, sv->row, cell);
    sv->edit_pending = sv->editor->pending;
    if (!ok || sv->editor->status == SONG_EDIT_TIDYING) {
        feedback(s, sv);
    }
    return ok;
}

__attribute__((noinline, unused)) static void finish_ready_edit(song_view* sv) {
    if (sv->edit_pending && sv->editor->sent) {
        (void)song_edit_poll(sv->editor);
        sv->edit_pending = sv->editor->pending;
    }
}

void tracker_view_note(screen* s, song_view* sv, uint32_t note) {
    finish_ready_edit(sv);
    if (note > GT2_LASTNOTE - GT2_FIRSTNOTE) {
        screen_toast(s, "NOTE OUT OF RANGE", SCREEN_TOAST_MS);
        return;
    }
    lane ln;
    uint8_t cell[4];
    if (!read_cell(sv, &ln, cell)) {
        feedback(s, sv);
        return;
    }
    cell[0] = (uint8_t)(GT2_FIRSTNOTE + note);
    cell[1] = sv->instr;
    sv->field = TRACKER_VIEW_FIELD_NOTE;
    sv->clear_armed = 0;
    if (write_cell(s, sv, &ln, cell)) {
        uint32_t n = rows_of(sv->song, ln.pattern);
        sv->row = (uint8_t)((sv->row + sv->step) % n);
    }
}

int tracker_view_popup(const song_view* sv, uint32_t encoder, int edit, char* name, char* value,
                       uint32_t* fill, uint32_t* max) {
    uint32_t v;
    const char* label;
    if (encoder == 6u && !edit) {
        v = sv->instr;
        label = "INSTRUMENT";
    } else if (encoder == PANEL_ENC_ALGORITHM && edit) {
        v = sv->step;
        label = "EDIT STEP";
    } else if ((encoder == PANEL_ENC_K1 || encoder == PANEL_ENC_K1 + 1u) && !edit &&
               song_view_rec_playing(sv)) {
        /* REC while the song plays: K1 and K2 record the cutoff and the resonance */
        uint32_t k = encoder - PANEL_ENC_K1;
        v = song_view_rec_value(sv, k);
        label = k == 0u ? "CUTOFF" : "RESONANCE";
        char digits[4];
        uint32_t n = 0;
        for (uint32_t d = 1000u; d; d /= 10u) {
            if (n || v / d || d == 1u) {
                digits[n++] = (char)('0' + v / d % 10u);
            }
        }
        do {
            *name++ = *label;
        } while (*label++);
        for (uint32_t i = 0; i < n; i++) {
            value[i] = digits[i];
        }
        value[n] = 0;
        *fill = v;
        *max = k == 0u ? 2047u : 15u;
        return 1;
    } else if (encoder >= PANEL_ENC_K1 && encoder <= PANEL_ENC_K4) {
        lane ln;
        uint8_t cell[4];
        if (!read_cell(sv, &ln, cell)) {
            return 0;
        }
        uint32_t k = encoder - PANEL_ENC_K1;
        label = k == 0u ? "INSTRUMENT" : k == 1u ? "COMMAND" : "DATA";
        v = k == 0u ? cell[1] : k == 1u ? cell[2] : cell[3];
    } else {
        return 0;
    }
    do {
        *name++ = *label;
    } while (*label++);
    value[0] = HEX[v >> 4 & 15u];
    value[1] = HEX[v & 15u];
    value[2] = 0;
    *fill = v;
    *max = encoder == PANEL_ENC_ALGORITHM ? 16u : encoder == PANEL_ENC_K1 || encoder == 6u
                         ? GT2_MAX_INSTR : encoder == PANEL_ENC_K1 + 1u ? 15u : 255u;
    return 1;
}

static int input(screen* s, void* state, const screen_event* e) {
    song_view* sv = state;
    int edit = (int)(e->held >> PANEL_BTN_EDIT & 1u);
    if (sv->rec) {
        finish_ready_edit(sv);
    }
    if (e->kind == SCREEN_PRESS && edit && e->id == PANEL_BTN_LFO) {
        finish_ready_edit(sv);
        if (sv->editor) {
            (void)song_edit_undo(sv->editor);
            sv->edit_pending = sv->editor->pending;
        }
        sv->clear_armed = 0;
        feedback(s, sv);
        return 1;
    }
    if (e->kind == SCREEN_PRESS && sv->rec &&
        (e->id == PANEL_BTN_ARP || e->id == PANEL_BTN_LFO)) {
        lane ln;
        uint8_t cell[4];
        if (read_cell(sv, &ln, cell)) {
            if (e->id == PANEL_BTN_ARP) {
                cell[0] = GT2_KEYOFF;
                sv->clear_armed = 0;
            } else if (sv->clear_armed && sv->clear_pattern == ln.pattern && sv->clear_row == sv->row) {
                cell[0] = GT2_REST;
                cell[1] = cell[2] = cell[3] = 0;
            } else {
                cell[0] = GT2_REST;
            }
            if (write_cell(s, sv, &ln, cell) && e->id == PANEL_BTN_LFO) {
                sv->clear_armed = 1;
                sv->clear_pattern = ln.pattern;
                sv->clear_row = sv->row;
            }
            sv->field = TRACKER_VIEW_FIELD_NOTE;
        } else {
            feedback(s, sv);
        }
        return 1;
    }
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint32_t n = rows_of(sv->song, lane_of(sv, sv->voice).pattern);
        int32_t step = edit ? TRACKER_VIEW_PAGE : 1u;
        int32_t row = (int32_t)sv->row + e->steps * step;
        sv->row = (uint8_t)(row < 0 || !n ? 0 : (uint32_t)row >= n ? n - 1u : (uint32_t)row);
        sv->clear_armed = 0;
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        if (edit) {
            sv->step = clamp((int32_t)sv->step + e->steps, 16u);
            return 1;
        }
        int32_t v = ((int32_t)sv->voice + e->steps) % (int32_t)SONG_T_VOICES;
        sv->voice = (uint8_t)(v < 0 ? v + (int32_t)SONG_T_VOICES : v);
        uint32_t n = rows_of(sv->song, lane_of(sv, sv->voice).pattern);
        if (n && sv->row >= n) {
            sv->row = (uint8_t)(n - 1u);
        }
        sv->clear_armed = 0;
        return 1;
    }
    if (!sv->rec) {
        return 0;
    }
    if (!edit && (e->id == PANEL_ENC_K1 || e->id == PANEL_ENC_K1 + 1u) &&
        song_view_rec_playing(sv)) {
        /* REC while the song plays: the filter knobs record locks on the row they are turned in */
        (void)song_view_record_knob(sv, s, e->id - PANEL_ENC_K1, e->steps);
        return 1;
    }
    if (e->id == 6u && !edit) {
        sv->instr = clamp((int32_t)sv->instr + e->steps, GT2_MAX_INSTR);
        if (!sv->instr) {
            sv->instr = 1;
        }
        return 1;
    }
    lane ln;
    uint8_t cell[4];
    if (e->id < PANEL_ENC_K1 || e->id > 6u) {
        return 0;
    }
    if (!read_cell(sv, &ln, cell)) {
        feedback(s, sv);
        return 1;
    }
    sv->clear_armed = 0;
    if (e->id == 6u) {
        int ok = song_edit_shift(sv->editor, ln.pattern, sv->row, e->steps > 0);
        sv->edit_pending = sv->editor->pending;
        if (!ok || sv->editor->status == SONG_EDIT_TIDYING) {
            feedback(s, sv);
        }
    } else {
        uint32_t k = e->id - PANEL_ENC_K1;
        if (k == 0u) {
            cell[1] = clamp((int32_t)cell[1] + e->steps, GT2_MAX_INSTR);
            sv->field = TRACKER_VIEW_FIELD_INSTRUMENT;
        } else if (k == 1u) {
            cell[2] = clamp((int32_t)cell[2] + e->steps, 15u);
            sv->field = TRACKER_VIEW_FIELD_COMMAND;
        } else {
            uint32_t shift = k == 2u ? 4u : 0u;
            uint32_t nibble = clamp((int32_t)(cell[3] >> shift & 15u) + e->steps, 15u);
            cell[3] = (uint8_t)((cell[3] & ~(15u << shift)) | nibble << shift);
            sv->field = TRACKER_VIEW_FIELD_COMMAND;
        }
        (void)write_cell(s, sv, &ln, cell);
    }
    return 1;
}

static int looks_changed(const song_view* sv) {
    uint8_t head[HEAD_LOOK], rows[ROWS_LOOK];
    head_look(sv, head);
    rows_look(sv, rows);
    for (uint32_t i = 0; i < HEAD_LOOK; i++) {
        if (head[i] != sv->drawn[i]) {
            return 1;
        }
    }
    for (uint32_t i = 0; i < ROWS_LOOK; i++) {
        if (rows[i] != sv->drawn[HEAD_LOOK + i]) {
            return 1;
        }
    }
    return 0;
}

static int model_loading(const song_view* sv) {
    return sv->io && (sv->io->state == SONG_IO_STOP || sv->io->state == SONG_IO_READ ||
                      sv->io->state == SONG_IO_VERIFY);
}

static int changed(screen* s, void* state) {
    (void)s;
    const song_view* sv = state;
    if (model_loading(sv)) {
        return sv->which == SONG_VIEW_TRACKER && sv->dirty != 0;
    }
    return !sv->built || sv->dirty || sv->unbuilt || looks_changed(sv) ||
           sv->which != SONG_VIEW_TRACKER;
}

static void compose(screen* s, void* state, grid* g) {
    song_view* sv = state;
    if (model_loading(sv)) {
        if (sv->which == SONG_VIEW_TRACKER) {
            song_view_copy(sv, s, g);
        }
        return;
    }
    song_view_acquire_canvas(sv, SONG_VIEW_TRACKER);
    uint8_t head[HEAD_LOOK], rows[ROWS_LOOK];
    head_look(sv, head);
    rows_look(sv, rows);
    int fresh = !sv->built || sv->which != SONG_VIEW_TRACKER;
    int head_new = fresh, rows_new = fresh;
    for (uint32_t i = 0; i < HEAD_LOOK; i++) {
        head_new |= head[i] != sv->drawn[i];
    }
    for (uint32_t i = 0; i < ROWS_LOOK; i++) {
        rows_new |= rows[i] != sv->drawn[HEAD_LOOK + i];
    }
    if (head_new) {
        build_head(sv);
        sv->dirty |= (1u << TRACKER_VIEW_TOP) - 1u;
        for (uint32_t i = 0; i < HEAD_LOOK; i++) {
            sv->drawn[i] = head[i];
        }
    }
    int animating = screen_animating(s);
    if (rows_new) {
        /* Only the bar moved (and with it the position): the two rows and the footer. */
        uint32_t changed = 0;
        for (uint32_t i = 0; i < ROWS_LOOK; i++) {
            changed |= (uint32_t)(rows[i] != sv->drawn[HEAD_LOOK + i]) << i;
        }
        if (!fresh && !head_new && !sv->unbuilt && !(changed & ~(ROWS_BAR_BIT | ROWS_POS_BIT))) {
            sv->dirty |= build_step(sv, sv->drawn[HEAD_LOOK + ROWS_BAR]);
        } else {
            /* The window is built a few rows a frame, as the copy takes their cells (below). */
            build_rows(sv);
        }
        for (uint32_t i = 0; i < ROWS_LOOK; i++) {
            sv->drawn[HEAD_LOOK + i] = rows[i];
        }
    }
    if (sv->unbuilt) {
        uint32_t first = 0;
        while (!(sv->unbuilt >> first & 1u)) {
            first++;
        }
        /* A wipe down reveals the rows from the top, a few a frame: only those are built (the rows
         * past them are held, and are built before they show); else a few rows a frame. */
        uint32_t reach = screen_reach(s);
        uint32_t upto = animating ? (reach > TRACKER_VIEW_TOP ? (reach - TRACKER_VIEW_TOP + 1u) / 2u : 0u)
                                  : first + TRACKER_VIEW_BUILD_ROWS;
        if (upto > first) {
            build_window(sv, first, upto - first);
        }
    }
    sv->built = 1;
    song_view_copy(sv, s, g);
}

const view tracker_view_view = {"TRACKER", enter, leave, input, compose, changed};
