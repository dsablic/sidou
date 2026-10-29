// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "step_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "live_view.h"
#include "song/gt2_song.h"
#include "song/song_ext.h"

/* What the view shows, as bytes (step_view.drawn), in three zones that are built apart: the
 * header with the page selector and the ruler (rows 0-3); the lanes (rows 4-15), which the
 * playing step changes; the HELD STEP frame and the pads (rows 16-29), which it does not. */
#define HEAD_AT 0u
#define HEAD_N 11u
#define LANES_AT (HEAD_AT + HEAD_N)
#define LANES_N 24u
#define LOWER_AT (LANES_AT + LANES_N)
#define LOWER_N 14u
_Static_assert(LOWER_AT + LOWER_N <= STEP_VIEW_LOOK, "the looks fit");
#define HEAD_ROWS 0x0000001Fu
#define LANES_ROWS 0x0001FFE0u
#define LOWER_ROWS 0x3FFE0000u

/* What a knob steps a lock by, its value when the step has none, its range's top. */
static const uint16_t LOCK_STEP[SONG_LOCK_PARAMS] = {32, 1, 1, 1, 64, 2, 2, 2, 2, 5};
static const uint16_t LOCK_START[SONG_LOCK_PARAMS] = {1024, 8, 0, 0, 2048, 64, 64, 64, 64, 100};
/* SET's choices: none, then triangle, sawtooth, pulse and noise, gate on. */
static const uint8_t WAVES[STEP_VIEW_WAVES] = {0u, 0x11u, 0x21u, 0x41u, 0x81u};
static const char* const WAVE_NAMES[STEP_VIEW_WAVES] = {"OFF", "TRI", "SAW", "PULSE", "NOISE"};
static const char* const WAVE_SHORT[STEP_VIEW_WAVES] = {"OFF", "TRI", "SAW", "PUL", "NOI"};
static const char* const KNOB_NAMES[4] = {"CUTOFF", "RESONANCE", "PULSE WIDTH", "WAVEFORM"};
static const char* const KNOB_SHORT[4] = {"CUTOFF", "RESO", "PULSE", "WAVE"};
/* DICE's settings by name (the frame and the popup). */
static const char* const DENSITY_NAMES[SONG_DICE_DENSITIES] = {"SPARSE", "LIGHT", "MEDIUM", "BUSY",
                                                               "FULL"};
static const char* const SCALE_NAMES[SONG_DICE_SCALES] = {"MIN PENT", "MAJ PENT", "MINOR",
                                                          "MAJOR",    "DORIAN",   "CHROMATIC"};
static const char* const CHAR_NAMES[4] = {"AGE", "DAC", "THUMP", "BLEED"};
static const char HEX[] = "0123456789ABCDEF";
static const char NOTE_NAMES[12][3] = {"C-", "C#", "D-", "D#", "E-", "F-",
                                       "F#", "G-", "G#", "A-", "A#", "B-"};

void step_view_init(step_view* st, song_view* sv) {
    memset(st, 0, sizeof *st);
    st->sv = sv;
    st->held = STEP_VIEW_NONE;
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        st->note[v] = 36u;
        st->instr[v] = 1u;
    }
}

/* ---- what each lane shows ---- */

typedef struct {
    uint8_t pattern; /* 0xFF: none */
    uint8_t rows;    /* its rows, the end row not counted */
    uint8_t play;    /* the row the song plays in it, 0xFF: none */
    uint8_t silent;
} lane;

/* Voice v's pattern: the first one at or after the orderlist entry the views show (the
 * player's while the song plays and its row events said one, the cursor's otherwise). */
static lane lane_of(const song_view* sv, uint32_t v) {
    lane ln = {0xffu, 0u, 0xffu, (uint8_t)(song_view_mute_mask(sv) >> v & 1u)};
    const gt2_song* s = sv->song;
    if (!s) {
        return ln;
    }
    uint32_t entry = sv->order_pos, sub = sv->subtune;
    if (song_view_playing(sv) && sv->st.row[v] != SONG_T_NOWHERE) {
        entry = sv->st.order_pos[v];
        sub = sv->st.subtune;
        ln.play = sv->st.row[v];
    }
    const uint8_t* o = gt2_order_data(s, sub, v);
    uint32_t n = gt2_order_entries(s, sub, v);
    for (uint32_t i = entry; o && i < n; i++) {
        if (o[i] < GT2_REPEAT) {
            ln.pattern = o[i];
            ln.rows = (uint8_t)(gt2_pattern_rows(s, o[i]) - 1u);
            break;
        }
    }
    return ln;
}

static uint32_t pages_of(uint32_t rows) {
    return rows ? (rows + STEP_VIEW_STEPS - 1u) / STEP_VIEW_STEPS : 1u;
}

/* The page shown: the selected lane's playing page while the song plays, else ALGORITHM's. */
static uint32_t page_of(const step_view* st, const lane lanes[SONG_T_VOICES]) {
    const lane* ln = &lanes[st->sv->voice];
    uint32_t page = ln->play != 0xffu ? ln->play / STEP_VIEW_STEPS : st->page;
    uint32_t pages = pages_of(ln->rows);
    return page >= pages ? pages - 1u : page;
}

static void lanes_of(const song_view* sv, lane lanes[SONG_T_VOICES]) {
    for (uint32_t v = 0; v < SONG_T_VOICES; v++) {
        lanes[v] = lane_of(sv, v);
    }
}

static int is_note(uint32_t n) {
    return n >= GT2_FIRSTNOTE && n <= GT2_LASTNOTE;
}

/* Row `row` of a pattern whose data is d (NULL: an empty pattern, all rests). */
static const uint8_t* cell_in(const uint8_t* d, uint32_t row) {
    static const uint8_t REST[4] = {GT2_REST, 0, 0, 0};
    return d ? d + row * GT2_ROW_BYTES : REST;
}

/* Row `row` of a lane's pattern as the model holds it. */
static const uint8_t* cell_of(const gt2_song* s, const lane* ln, uint32_t row) {
    return cell_in(gt2_pattern_data(s, ln->pattern), row);
}

/* The LOCK chunk's records (song_ext.h: a version, a 16-bit count, then SONG_LOCK_BYTES a
 * record sorted by pattern, row, parameter), NULL and 0 with none: a lane reads them once. */
static const uint8_t* lock_records(const gt2_song* s, uint32_t* n) {
    uint32_t len = 0u;
    const uint8_t* d = song_ext_find(s, SONG_TAG_LOCK, &len);
    *n = 0u;
    if (!d || len < 3u || d[0] != 1u) {
        return NULL;
    }
    uint32_t count = (uint32_t)d[1] | (uint32_t)d[2] << 8;
    if (3u + count * SONG_LOCK_BYTES > len) {
        return NULL;
    }
    *n = count;
    return d + 3;
}

/* A record's pattern and row. */
static uint32_t lock_at(const uint8_t* recs, uint32_t i) {
    return (uint32_t)recs[i * SONG_LOCK_BYTES] << 8 | recs[i * SONG_LOCK_BYTES + 1u];
}

/* The first record at or after (pattern, row). */
static uint32_t lock_first(const uint8_t* recs, uint32_t n, uint32_t pattern, uint32_t row) {
    uint32_t lo = 0u, hi = n, key = pattern << 8 | row;
    while (lo < hi) {
        uint32_t mid = (lo + hi) / 2u;
        if (lock_at(recs, mid) < key) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    return lo;
}

static int lock_get(const gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param,
                    uint32_t* value) {
    uint32_t n = 0u;
    const uint8_t* r = song_locks_row(s, pattern, row, &n);
    for (uint32_t i = 0; r && i < n; i++, r += SONG_LOCK_BYTES) {
        if (r[2] == param) {
            *value = (uint32_t)r[3] | (uint32_t)r[4] << 8;
            return 1;
        }
    }
    return 0;
}

/* ---- the look ---- */

static void put32(uint8_t* out, uint32_t v) {
    out[0] = (uint8_t)v;
    out[1] = (uint8_t)(v >> 8);
    out[2] = (uint8_t)(v >> 16);
    out[3] = (uint8_t)(v >> 24);
}

/* The song's DICE density and scale (the defaults with no song or no chunk). */
static void dice_get(const song_view* sv, uint8_t* d, uint8_t* sc) {
    if (sv->song) {
        (void)song_ext_get_dice(sv->song, d, sc);
    } else {
        *d = SONG_DICE_DENSITY_DEFAULT;
        *sc = 0u;
    }
}

static void look(const step_view* st, uint8_t out[STEP_VIEW_LOOK]) {
    const song_view* sv = st->sv;
    lane lanes[SONG_T_VOICES];
    lanes_of(sv, lanes);
    memset(out, 0, STEP_VIEW_LOOK);
    uint32_t v = sv->voice, page = page_of(st, lanes);
    uint32_t bpm = song_view_shown_bpm(sv);
    uint8_t* h = out + HEAD_AT;
    h[0] = (uint8_t)song_view_playing(sv);
    h[1] = (uint8_t)bpm;
    h[2] = (uint8_t)(bpm >> 8);
    h[3] = (uint8_t)(sv->rec | (sv->io && song_io_saving(sv->io) ? 2u : 0u));
    h[4] = (uint8_t)v;
    h[5] = (uint8_t)(sv->mute | sv->solo << 3);
    h[6] = lanes[v].pattern;
    h[7] = sv->l->theme;
    h[8] = (uint8_t)page;
    h[9] = (uint8_t)pages_of(lanes[v].rows);
    h[10] = lanes[v].rows;
    uint8_t* a = out + LANES_AT;
    put32(a, sv->song_edits);
    a[4] = sv->editor ? (uint8_t)sv->editor->revision : 0u;
    a[5] = st->rev;
    a[6] = (uint8_t)page;
    a[7] = (uint8_t)v;
    for (uint32_t k = 0; k < SONG_T_VOICES; k++) {
        a[8 + k] = lanes[k].pattern;
        a[11 + k] = lanes[k].play != 0xffu && lanes[k].play / STEP_VIEW_STEPS == page
                        ? (uint8_t)(lanes[k].play % STEP_VIEW_STEPS) : 0xffu;
        a[14] = (uint8_t)(a[14] | lanes[k].silent << k);
        a[16 + k] = st->note[k];
        a[19 + k] = st->instr[k];
    }
    a[15] = st->held;
    a[22] = st->gate;
    a[23] = sv->l->theme;
    uint8_t* l = out + LOWER_AT;
    put32(l, sv->song_edits);
    l[4] = a[4];
    l[5] = st->rev;
    l[6] = (uint8_t)page;
    l[7] = (uint8_t)v;
    l[8] = lanes[v].pattern;
    l[9] = st->held;
    l[10] = sv->l->theme;
    if (st->scl_down) {
        uint8_t d, sc;
        dice_get(sv, &d, &sc);
        l[11] = 1u;
        l[12] = d;
        l[13] = sc;
    }
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

static void blank(canvas* c, uint32_t row, uint32_t rows, uint8_t a) {
    for (uint32_t r = row; r < row + rows; r++) {
        memset(c->ch[r], ' ', GRID_COLS);
        memset(c->attr[r], a, GRID_COLS);
    }
}

static void put(canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t a) {
    c->ch[row][col] = ch;
    c->attr[row][col] = a;
}

/* n in decimal, at most 4 digits, into out; returns its length. */
static uint32_t decimal(uint32_t n, char out[5]) {
    uint32_t len = 0;
    if (n >= 1000u) {
        out[len++] = (char)('0' + n / 1000u % 10u);
    }
    if (n >= 100u) {
        out[len++] = (char)('0' + n / 100u % 10u);
    }
    if (n >= 10u) {
        out[len++] = (char)('0' + n / 10u % 10u);
    }
    out[len++] = (char)('0' + n % 10u);
    out[len] = 0;
    return len;
}

static void note_name(uint32_t note, char out[4]) {
    out[0] = NOTE_NAMES[note % 12u][0];
    out[1] = NOTE_NAMES[note % 12u][1];
    out[2] = (char)('0' + note / 12u % 10u);
    out[3] = 0;
}

static uint32_t step_col(uint32_t i) {
    return STEP_VIEW_STEP_COL + i + i / 4u;
}

/* The header, the page selector and the ruler: rows 0-4. */
static void build_head(step_view* st, const lane lanes[SONG_T_VOICES], uint32_t page) {
    song_view* sv = st->sv;
    canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    const uint8_t base = THEME_ATTR(th, value);
    uint32_t v = sv->voice;
    blank(c, 0, 5u, base);
    char vn[3] = {'V', (char)('1' + v), 0};
    big(c, 0, 0, vn, GRID_ATTR(lanes[v].silent ? th->dim : th->voice[v], th->bg));
    char pat[7] = "PAT --";
    if (lanes[v].pattern != 0xffu) {
        pat[4] = HEX[lanes[v].pattern >> 4 & 15u];
        pat[5] = HEX[lanes[v].pattern & 15u];
    }
    big(c, 5, 0, sv->io && song_io_saving(sv->io) ? "SAVING" : pat, base);
    if (sv->solo >> v & 1u) {
        small(c, 18, 0, "SOL", base);
    } else if (sv->mute >> v & 1u) {
        small(c, 18, 0, "MUT", THEME_ATTR(th, alert));
    }
    put(c, 22, 0, song_view_playing(sv) ? FONT_PLAY : FONT_SQUARE, base);
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
    small(c, 26, 1, "BPM", THEME_ATTR(th, hint));
    if (sv->rec) {
        small(c, 22, 1, "REC", THEME_ATTR(th, alert));
    }
    /* Row 2: the pages, the shown one reversed, and the pattern's length. */
    small(c, 0, STEP_VIEW_PAGE_ROW, "PAGE", THEME_ATTR(th, hint));
    uint32_t pages = pages_of(lanes[v].rows);
    for (uint32_t p = 0; p < pages && p < 8u; p++) {
        put(c, 5u + 2u * p, STEP_VIEW_PAGE_ROW, (uint8_t)('1' + p),
            p == page ? GRID_ATTR(th->bg, th->hot) : THEME_ATTR(th, label));
    }
    char len[5];
    uint32_t n = decimal(lanes[v].rows, len);
    small(c, 21, STEP_VIEW_PAGE_ROW, len, THEME_ATTR(th, value));
    small(c, 21u + n, STEP_VIEW_PAGE_ROW, " STEPS", THEME_ATTR(th, hint));
    /* Rows 3-4, at 2x: the number of each group's first step. */
    for (uint32_t g = 0; g < 4u; g++) {
        decimal(page * STEP_VIEW_STEPS + g * 4u + 1u, len);
        big(c, step_col(g * 4u), STEP_VIEW_RULER_ROW, len, THEME_ATTR(th, label));
    }
    sv->dirty |= HEAD_ROWS;
}

/* Lane l's four rows from its first: the pips, their notches, and at 2x the lane's note, its
 * instrument and its gate. */
static void build_lane(step_view* st, const lane lanes[SONG_T_VOICES], uint32_t l, uint32_t page) {
    song_view* sv = st->sv;
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    const theme* th = theme_get(sv->l->theme);
    uint32_t r = STEP_VIEW_LANES_ROW + STEP_VIEW_LANE_ROWS * l;
    blank(c, r, STEP_VIEW_LANE_ROWS, THEME_ATTR(th, value));
    const lane* ln = &lanes[l];
    int selected = l == sv->voice;
    char vn[3] = {'V', (char)('1' + l), 0};
    big(c, 0, r, vn, GRID_ATTR(ln->silent ? th->dim : th->voice[l], th->bg));
    if (selected) {
        put(c, 4, r, FONT_ARROW_RIGHT, THEME_ATTR(th, hot));
    }
    const uint8_t* d = s && ln->pattern != 0xffu ? gt2_pattern_data(s, ln->pattern) : NULL;
    uint32_t nlocks = 0u, next = 0u;
    const uint8_t* recs = s ? lock_records(s, &nlocks) : NULL;
    if (recs) {
        next = lock_first(recs, nlocks, ln->pattern, page * STEP_VIEW_STEPS);
    }
    for (uint32_t i = 0; s && ln->pattern != 0xffu && i < STEP_VIEW_STEPS; i++) {
        uint32_t row = page * STEP_VIEW_STEPS + i;
        if (row >= ln->rows) {
            break;
        }
        const uint8_t* cell = cell_in(d, row);
        int note = is_note(cell[0]);
        uint32_t col = step_col(i);
        int held = selected && st->held == i;
        int playing = ln->play == row;
        uint8_t pip = note ? FONT_BALL : FONT_PIP;
        uint8_t color = playing ? th->value : note ? th->voice[l] : th->dim;
        if (held) {
            put(c, col, r, pip, GRID_ATTR(th->bg, th->hot));
            put(c, col, r + 1u, ' ', GRID_ATTR(th->bg, th->hot));
        } else {
            put(c, col, r, pip, GRID_ATTR(color, th->bg));
        }
        /* The notch: yellow for a LOCK, cyan for a SET (the waveform command). */
        uint32_t locks = 0u;
        if (recs) {
            while (next < nlocks && lock_at(recs, next) < (ln->pattern << 8 | row)) {
                next++;
            }
            locks = next < nlocks && lock_at(recs, next) == (ln->pattern << 8 | row);
        }
        if (!held && (locks || cell[2] == 7u)) {
            put(c, col, r + 1u, locks ? FONT_V4 : FONT_V2,
                GRID_ATTR(locks ? th->hot : th->knob[1], th->bg));
        }
    }
    char text[24];
    uint32_t n = 0;
    note_name(st->note[l], text);
    n = 3u;
    text[n++] = ' ';
    text[n++] = 'I';
    text[n++] = HEX[st->instr[l] >> 4];
    text[n++] = HEX[st->instr[l] & 15u];
    text[n++] = ' ';
    const char* gate = st->gate >> l & 1u ? "LEG" : "STAC";
    while (*gate) {
        text[n++] = *gate++;
    }
    text[n] = 0;
    big(c, STEP_VIEW_STEP_COL, r + 2u, text,
        selected ? THEME_ATTR(th, value) : THEME_ATTR(th, hint));
}

static void build_lanes(step_view* st, const lane lanes[SONG_T_VOICES], uint32_t page) {
    for (uint32_t l = 0; l < STEP_VIEW_LANES; l++) {
        build_lane(st, lanes, l, page);
    }
    st->sv->dirty |= LANES_ROWS;
}

/* A bar of 8 cells with `fill` of `max`, in the knob's colour on a dark track. */
static void bar(canvas* c, uint32_t col, uint32_t row, uint32_t fill, uint32_t max,
                uint8_t color, uint8_t track) {
    uint32_t px = max ? fill * 64u / max : 0u;
    for (uint32_t k = 0; k < 8u; k++) {
        uint32_t here = px > 8u * k ? px - 8u * k : 0u;
        put(c, col + k, row, (uint8_t)(FONT_BAR0 + (here > 8u ? 8u : here)),
            GRID_ATTR(color, track));
    }
}

/* One knob of the HELD STEP frame: K1 and K3 on the frame's left, K2 and K4 on its right (the
 * knob's number from col, its name after it, LOCK or SET after the name, its bar and its
 * value under them). */
static void knob_cell(step_view* st, const lane* ln, uint32_t at, int held, uint32_t k,
                      uint32_t row) {
    song_view* sv = st->sv;
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    const theme* th = theme_get(sv->l->theme);
    uint32_t col = k & 1u ? 17u : 2u;
    char tag[3] = {'K', (char)('1' + k), 0};
    /* The legend reads at 1x with nothing held too: hint, not dim (2.16:1). */
    small(c, col, row, tag, THEME_ATTR(th, hint));
    small(c, col + 3u, row, KNOB_SHORT[k], GRID_ATTR(held ? th->knob_text[k] : th->hint, th->bg));
    uint32_t value = 0u, max = 0u;
    int tagged = 0;
    char text[8] = "---";
    if (held) {
        if (k < 3u) {
            uint32_t p = k == 0u ? SONG_LOCK_CUTOFF : k == 1u ? SONG_LOCK_RESONANCE : SONG_LOCK_PULSE;
            tagged = lock_get(s, ln->pattern, at, p, &value);
            max = song_lock_max(p);
            if (tagged) {
                decimal(k == 1u ? value : (value * 100u + max / 2u) / max, text);
            }
        } else {
            const uint8_t* cell = cell_of(s, ln, at);
            uint32_t choice = 0u;
            for (uint32_t w = 1; cell[2] == 7u && w < STEP_VIEW_WAVES; w++) {
                choice = cell[3] == WAVES[w] ? w : choice;
            }
            tagged = cell[2] == 7u;
            value = choice;
            max = STEP_VIEW_WAVES - 1u;
            if (tagged) {
                memcpy(text, choice ? WAVE_SHORT[choice] : "7XY", 4u);
            }
        }
    }
    bar(c, col, row + 1u, value, max, th->knob[k], th->dim);
    small(c, col + 9u, row + 1u, text, GRID_ATTR(tagged ? th->value : th->dim, th->bg));
    if (tagged) {
        small(c, col + (k & 1u ? 8u : 10u), row, k == 3u ? "SET" : "LOCK",
              GRID_ATTR(k == 3u ? th->knob[1] : th->hot, th->bg));
    }
}

/* The pads: two rows of eight, a beat (four steps) apart, each numbered by its place in its
 * beat at 2x (1 2 3 4  1 2 3 4); the second and fourth beats shaded (the cursor's dark grey),
 * a beat's first step in the value role and the others in the label role, a step's pad in the
 * lane's colour when it has a note, the held one yellow. The held step's own number is the
 * frame's (HELD STEP 22). */
static void build_pads(step_view* st, const lane* ln, uint32_t page) {
    song_view* sv = st->sv;
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    const theme* th = theme_get(sv->l->theme);
    uint32_t v = sv->voice;
    const uint8_t* pd = s && ln->pattern != 0xffu ? gt2_pattern_data(s, ln->pattern) : NULL;
    for (uint32_t i = 0; i < STEP_VIEW_STEPS; i++) {
        uint32_t row = page * STEP_VIEW_STEPS + i;
        uint32_t j = i & 7u;
        uint32_t r = STEP_VIEW_PADS_ROW + (i >> 3) * 3u, col = STEP_VIEW_PAD_COL + 3u * j + (j >> 2) * 2u;
        if (row >= ln->rows || !s || ln->pattern == 0xffu) {
            continue;
        }
        const uint8_t* cell = cell_in(pd, row);
        uint8_t a = GRID_ATTR(i & 3u ? th->label : th->value, i & 4u ? th->cursor : th->bg);
        if (is_note(cell[0])) {
            a = GRID_ATTR(th->bg, th->voice[v]);
        }
        if (st->held == i) {
            a = GRID_ATTR(th->bg, th->hot);
        }
        char num[2] = {(char)('1' + (i & 3u)), 0};
        big(c, col, r, num, a);
    }
    sv->dirty |= LOWER_ROWS;
}

/* The frame while EDIT + SCL is held: DICE's two settings at 2x, K1 and K3 beside them (what a
 * turn changes is what the frame says; a release rolls). */
static void dice_frame(step_view* st) {
    canvas* c = &st->sv->cv;
    const theme* th = theme_get(st->sv->l->theme);
    uint8_t d, sc;
    dice_get(st->sv, &d, &sc);
    small(c, 2, 17u, " DICE - LET GO TO ROLL ", THEME_ATTR(th, hot));
    small(c, 2, 18u, "K1", THEME_ATTR(th, hint));
    small(c, 2, 19u, "DENS", GRID_ATTR(th->knob_text[0], th->bg));
    big(c, 11, 18u, DENSITY_NAMES[d], GRID_ATTR(th->knob_text[0], th->bg));
    small(c, 2, 20u, "K3", THEME_ATTR(th, hint));
    small(c, 2, 21u, "SCALE", GRID_ATTR(th->knob_text[2], th->bg));
    big(c, 11, 20u, SCALE_NAMES[sc], GRID_ATTR(th->knob_text[2], th->bg));
}

/* Rows 16-29: the HELD STEP frame and the pads. */
static void build_lower(step_view* st, const lane lanes[SONG_T_VOICES], uint32_t page) {
    song_view* sv = st->sv;
    canvas* c = &sv->cv;
    const gt2_song* s = sv->song;
    const theme* th = theme_get(sv->l->theme);
    uint32_t v = sv->voice;
    const lane* ln = &lanes[v];
    const uint8_t frame = THEME_ATTR(th, dim);
    blank(c, 17u, GRID_ROWS - 17u, THEME_ATTR(th, value));
    /* The frame, rows 17-22. */
    put(c, 0, 17u, FONT_ROUND_TL, frame);
    put(c, GRID_COLS - 1u, 17u, FONT_ROUND_TR, frame);
    put(c, 0, 22u, FONT_ROUND_BL, frame);
    put(c, GRID_COLS - 1u, 22u, FONT_ROUND_BR, frame);
    memset(&c->ch[17][1], FONT_BOX_H, GRID_COLS - 2u);
    memset(&c->attr[17][1], frame, GRID_COLS - 2u);
    memset(&c->ch[22][1], FONT_BOX_H, GRID_COLS - 2u);
    memset(&c->attr[22][1], frame, GRID_COLS - 2u);
    for (uint32_t r = 18u; r < 22u; r++) {
        put(c, 0, r, FONT_BOX_V, frame);
        put(c, GRID_COLS - 1u, r, FONT_BOX_V, frame);
    }
    uint32_t at = page * STEP_VIEW_STEPS + st->held;
    int held = st->held != STEP_VIEW_NONE && s && ln->pattern != 0xffu && at < ln->rows;
    if (st->scl_down) {
        dice_frame(st);
        build_pads(st, ln, page);
        return;
    }
    char title[16] = " HELD STEP ";
    uint32_t n = 11u;
    if (held) {
        n += decimal(at + 1u, title + n);
    } else {
        title[n++] = '-';
        title[n++] = '-';
    }
    title[n++] = ' ';
    title[n] = 0;
    small(c, 2, 17u, title, held ? THEME_ATTR(th, hot) : THEME_ATTR(th, hint));
    if (held) {
        const uint8_t* cell = cell_of(s, ln, at);
        if (is_note(cell[0])) {
            char nn[4];
            note_name(cell[0] - GT2_FIRSTNOTE, nn);
            small(c, 17, 17u, nn, THEME_ATTR(th, value));
        }
        char pc[6];
        uint32_t chance = 100u;
        if (lock_get(s, ln->pattern, at, SONG_LOCK_CHANCE, &chance)) {
            pc[decimal(chance, pc)] = '%';
            small(c, 22, 17u, pc, THEME_ATTR(th, hot));
        }
    }
    for (uint32_t k = 0; k < 4u; k++) {
        knob_cell(st, ln, at, held, k, 18u + 2u * (k >> 1));
    }
    build_pads(st, ln, page);
}


/* ---- editing ---- */

__attribute__((noinline, unused)) static void feedback(screen* s, const song_view* sv) {
    const char* msg = sv->editor && sv->song ? song_edit_message(sv->editor) : "NO SONG";
    if (msg && *msg) {
        screen_toast(s, msg, SCREEN_TOAST_MS);
        s->popup_ask = 0;
    }
}

__attribute__((noinline, unused)) static void refuse(screen* s, const char* msg) {
    screen_toast(s, msg, SCREEN_TOAST_MS);
    s->popup_ask = 0;
}

__attribute__((noinline, unused)) static void finish_ready_edit(song_view* sv) {
    if (sv->edit_pending && sv->editor->sent) {
        (void)song_edit_poll(sv->editor);
        sv->edit_pending = sv->editor->pending;
    }
}

/* A step of a pattern: where an action writes. */
typedef struct {
    uint8_t pattern, rows, voice, row;
} target;

void step_view_leds(const step_view* st, uint32_t* set, uint32_t* rows, uint32_t* head) {
    const song_view* sv = st->sv;
    lane lanes[SONG_T_VOICES];
    lanes_of(sv, lanes);
    const lane* ln = &lanes[sv->voice];
    uint32_t page = page_of(st, lanes);
    const uint8_t* d = sv->song && ln->pattern != 0xffu ? gt2_pattern_data(sv->song, ln->pattern) : NULL;
    *set = *rows = 0u;
    *head = STEP_VIEW_NONE;
    for (uint32_t i = 0; ln->pattern != 0xffu && i < STEP_VIEW_STEPS; i++) {
        uint32_t row = page * STEP_VIEW_STEPS + i;
        if (row >= ln->rows) {
            break;
        }
        *rows |= 1u << i;
        if (is_note(cell_in(d, row)[0])) {
            *set |= 1u << i;
        }
    }
    if (ln->play != 0xffu && ln->play / STEP_VIEW_STEPS == page) {
        *head = ln->play % STEP_VIEW_STEPS;
    }
}

/* The selected lane's step i of the page shown. */
__attribute__((noinline, unused)) static int target_of(const step_view* st, uint32_t i, target* t) {
    const song_view* sv = st->sv;
    lane lanes[SONG_T_VOICES];
    lanes_of(sv, lanes);
    t->voice = sv->voice;
    t->pattern = lanes[t->voice].pattern;
    t->rows = lanes[t->voice].rows;
    t->row = (uint8_t)(page_of(st, lanes) * STEP_VIEW_STEPS + i);
    return sv->song && t->pattern != 0xffu && t->row < t->rows;
}

__attribute__((noinline, unused)) static int read_cell(const step_view* st, const target* t,
                                                   uint32_t row, uint8_t cell[4]) {
    return st->sv->editor && song_edit_read_row(st->sv->editor, t->pattern, row, cell);
}

__attribute__((noinline, unused)) static int write_cell(screen* s, step_view* st, const target* t,
                                                    uint32_t row, const uint8_t cell[4]) {
    song_view* sv = st->sv;
    int ok = song_edit_row(sv->editor, t->pattern, row, cell);
    sv->edit_pending = sv->editor->pending;
    if (!ok || sv->editor->status == SONG_EDIT_TIDYING) {
        feedback(s, sv);
    }
    return ok;
}

/* Step t's note on or off on its lane (on: note, the lane's default for a key). */
static void toggle_now(screen* s, step_view* st, const target* t, uint32_t note) {
    song_view* sv = st->sv;
    uint8_t cell[4], other[4];
    if (!read_cell(st, t, t->row, cell)) {
        feedback(s, sv);
        return;
    }
    int staccato = !(st->gate >> t->voice & 1u);
    int note_on = !is_note(cell[0]);
    int has_next;
    if (note_on) {
        cell[0] = (uint8_t)(GT2_FIRSTNOTE + note);
        cell[1] = st->instr[t->voice];
    } else {
        /* A rest, or the key off the note before it still wants. */
        int after_note = t->row && read_cell(st, t, t->row - 1u, other) && is_note(other[0]);
        cell[0] = staccato && after_note ? GT2_KEYOFF : GT2_REST;
        cell[1] = 0u;
    }
    has_next = t->row + 1u < t->rows && read_cell(st, t, t->row + 1u, other);
    if (!write_cell(s, st, t, t->row, cell)) {
        return;
    }
    if (staccato && has_next) {
        /* Staccato: a key off on the next empty row; taking a note away takes its key off. */
        if (note_on && other[0] == GT2_REST) {
            other[0] = GT2_KEYOFF;
            st->follow = 1u;
        } else if (!note_on && other[0] == GT2_KEYOFF && !other[1] && !other[2]) {
            other[0] = GT2_REST;
            st->follow = 1u;
        }
        if (st->follow) {
            st->follow_pattern = t->pattern;
            st->follow_row = (uint8_t)(t->row + 1u);
            memcpy(st->follow_cell, other, 4u);
        }
    }
}

/* SET: K4's waveform command on a step. */
static void wave_now(screen* s, step_view* st, const target* t, int32_t steps) {
    uint8_t cell[4];
    if (!read_cell(st, t, t->row, cell)) {
        feedback(s, st->sv);
        return;
    }
    if (cell[2] && cell[2] != 7u) {
        refuse(s, "ROW HAS A COMMAND");
        return;
    }
    int32_t choice = 0;
    for (uint32_t w = 1; cell[2] == 7u && w < STEP_VIEW_WAVES; w++) {
        choice = cell[3] == WAVES[w] ? (int32_t)w : choice;
    }
    choice += steps;
    choice = choice < 0 ? 0 : choice > (int32_t)(STEP_VIEW_WAVES - 1u) ? (int32_t)(STEP_VIEW_WAVES - 1u) : choice;
    cell[2] = choice ? 7u : 0u;
    cell[3] = WAVES[choice];
    (void)write_cell(s, st, t, t->row, cell);
}

/* PRESETS on a held step: its note (the lane's from then on). */
static void note_now(screen* s, step_view* st, const target* t, int32_t steps) {
    uint8_t cell[4];
    if (!read_cell(st, t, t->row, cell)) {
        feedback(s, st->sv);
        return;
    }
    int32_t base = is_note(cell[0]) ? (int32_t)cell[0] - (int32_t)GT2_FIRSTNOTE
                                    : (int32_t)st->note[t->voice];
    base += steps;
    base = base < 0 ? 0 : base > (int32_t)GT2_NOTE_MAX ? (int32_t)GT2_NOTE_MAX : base;
    cell[0] = (uint8_t)(GT2_FIRSTNOTE + (uint32_t)base);
    cell[1] = cell[1] ? cell[1] : st->instr[t->voice];
    st->note[t->voice] = (uint8_t)base;
    (void)write_cell(s, st, t, t->row, cell);
}

/* A step's waveform command goes (its locks went already). */
static void unset_now(screen* s, step_view* st, const target* t) {
    uint8_t cell[4];
    if (read_cell(st, t, t->row, cell) && cell[2] == 7u) {
        cell[2] = cell[3] = 0u;
        (void)write_cell(s, st, t, t->row, cell);
    }
}

/* ---- generative play: MUTATE and DICE (E+ARP, E+SCL) ----
 * Both are runs of ordinary cell writes through the editor, one a turn of drain, so each is an
 * edit as any other (the pattern stays plain GT2 data; undo steps back a cell at a time). The
 * notes come from a scale on the lane's own note (PRESETS with a step held): the song's DICE
 * scale (the minor pentatonic unless EDIT + SCL + K3 chose another);
 * a lane the SCNE chunk calls DRUMS keeps its one note and varies only where it hits. What
 * "random" means is song_mix of the seed and the row: the same seed makes the same pattern. */
enum { GEN_MUTATE = 1, GEN_DICE };
/* DICE's scales (the song's DICE chunk picks one; 0 is the minor pentatonic it always rolled):
 * the length, then the semitones above the root. */
static const uint8_t SCALES[SONG_DICE_SCALES][13] = {
    {5, 0, 3, 5, 7, 10},          {5, 0, 2, 4, 7, 9},         {7, 0, 2, 3, 5, 7, 8, 10},
    {7, 0, 2, 4, 5, 7, 9, 11},    {7, 0, 2, 3, 5, 7, 9, 10},  {12, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11}};
/* DICE's grid by density (0 sparse .. 4 every step; 2 is what it always rolled): the chance in
 * percent of a hit on a row's place in its beat. A melodic lane hits on the eighths, a drum lane
 * on the sixteenths with a heavy first one. */
static const uint8_t GRID_MELODIC[SONG_DICE_DENSITIES][4] = {
    {60, 0, 0, 0}, {85, 0, 25, 0}, {85, 0, 55, 0}, {90, 35, 75, 35}, {100, 100, 100, 100}};
static const uint8_t GRID_DRUMS[SONG_DICE_DENSITIES][4] = {
    {80, 0, 15, 0}, {90, 5, 30, 10}, {90, 15, 50, 25}, {95, 40, 75, 55}, {100, 100, 100, 100}};
#define GEN_PHI 0x9e3779b9u

static int in_scale(const step_view* st, uint32_t root, uint32_t note) {
    const uint8_t* sc = SCALES[st->gen_scale];
    for (uint32_t i = 1; i <= sc[0]; i++) {
        if ((note + 120u - root) % 12u == sc[i]) {
            return 1;
        }
    }
    return 0;
}

/* The note of a random value h on the lane: its own for a drum lane, else a degree of the
 * scale in the octave from the lane's note or the next. */
static uint32_t gen_note(const step_view* st, uint32_t voice, uint32_t h) {
    uint32_t root = st->note[voice];
    if (st->gen_drums) {
        return root;
    }
    const uint8_t* sc = SCALES[st->gen_scale];
    uint32_t n = root + sc[1u + h % sc[0]] + 12u * (h >> 8 & 1u);
    while (n > GT2_NOTE_MAX) {
        n -= 12u;
    }
    return n;
}

static int dice_hit(const step_view* st, uint32_t row) {
    uint32_t h = song_mix(st->seed + row * GEN_PHI);
    return (h >> 8) % 100u < (st->gen_drums ? GRID_DRUMS : GRID_MELODIC)[st->gen_dens][row & 3u];
}

/* DICE's row: its note, or what follows one (a key off on a staccato lane), or a rest; only a
 * row that differs is written (its command and data stay). 1: written, or the run is over. */
static int dice_row(screen* s, step_view* st, const target* t) {
    uint8_t cell[4];
    uint32_t r = t->row;
    if (!read_cell(st, t, r, cell)) {
        feedback(s, st->sv);
        st->run = 0u;
        return 1;
    }
    int hit = dice_hit(st, r);
    uint8_t want = hit ? (uint8_t)(GT2_FIRSTNOTE + gen_note(st, t->voice, song_mix(st->seed ^ r)))
                       : r && !(st->gate >> t->voice & 1u) && dice_hit(st, r - 1u) ? GT2_KEYOFF
                                                                                      : GT2_REST;
    uint8_t instr = hit ? st->instr[t->voice] : 0u;
    if (cell[0] == want && cell[1] == instr) {
        return 0;
    }
    cell[0] = want;
    cell[1] = instr;
    if (!write_cell(s, st, t, r, cell)) {
        st->run = 0u;
    }
    return 1;
}

/* One of MUTATE's two changes at a random row: a note is nudged to the next note of the scale
 * up or down, or goes; a rest gets a note of the scale (a ghost step). */
static void mutate_now(screen* s, step_view* st, target* t) {
    uint8_t cell[4];
    uint32_t h = song_mix(st->seed + st->gen_row * GEN_PHI);
    t->row = (uint8_t)(h % t->rows);
    if (!read_cell(st, t, t->row, cell)) {
        feedback(s, st->sv);
        st->run = 0u;
        return;
    }
    h = song_mix(h);
    if (!is_note(cell[0]) || st->gen_drums || h >> 9 & 1u) {
        toggle_now(s, st, t, gen_note(st, t->voice, h));
        return;
    }
    uint32_t root = st->note[t->voice], n = cell[0] - GT2_FIRSTNOTE;
    int up = (int)(h >> 10 & 1u);
    for (uint32_t i = 0; i < 12u; i++) {
        n = up ? (n < GT2_NOTE_MAX ? n + 1u : n) : (n ? n - 1u : n);
        if (in_scale(st, root, n)) {
            break;
        }
    }
    cell[0] = (uint8_t)(GT2_FIRSTNOTE + n);
    (void)write_cell(s, st, t, t->row, cell);
}

/* The next write of the run MUTATE or DICE is on: one a turn of drain. */
__attribute__((noinline, unused)) static void gen_now(screen* s, step_view* st) {
    song_view* sv = st->sv;
    target t = {st->gen_pattern, 0u, st->gen_voice, st->gen_row};
    t.rows = sv->song ? (uint8_t)(gt2_pattern_rows(sv->song, t.pattern) - 1u) : 0u;
    if (!t.rows) {
        st->run = 0u;
    } else if (st->run == GEN_MUTATE) {
        mutate_now(s, st, &t);
        st->run = ++st->gen_row < 2u ? st->run : 0u;
    } else {
        while (st->run && st->gen_row < t.rows) {
            t.row = st->gen_row++;
            if (dice_row(s, st, &t)) {
                return;
            }
        }
        st->run = 0u;
    }
}

/* What a cell write waits for: the editor takes one at a time, and the first after a key
 * starts the audition, which holds edits for a while. The actions wait here, in order. */
enum { ACT_TOGGLE = 1, ACT_WAVE, ACT_NOTE, ACT_UNSET };

/* Runs what waits: the second row of a step, or the first action, once the editor is free. */
__attribute__((noinline, unused)) static void drain(screen* s, step_view* st) {
    song_view* sv = st->sv;
    if (!sv->editor) {
        return;
    }
    finish_ready_edit(sv);
    if (sv->editor->pending) {
        return;
    }
    if (st->follow) {
        st->follow = 0u;
        int ok = song_edit_row(sv->editor, st->follow_pattern, st->follow_row, st->follow_cell);
        sv->edit_pending = sv->editor->pending;
        if (!ok) {
            feedback(s, sv);
        }
        return;
    }
    if (!st->qn) {
        if (st->run) {
            gen_now(s, st);
        }
        return;
    }
    step_act a = st->q[0];
    for (uint32_t i = 1; i < st->qn; i++) {
        st->q[i - 1u] = st->q[i];
    }
    st->qn--;
    target t = {a.pattern, 0u, a.voice, a.row};
    t.rows = sv->song ? (uint8_t)(gt2_pattern_rows(sv->song, a.pattern) - 1u) : 0u;
    if (a.kind == ACT_TOGGLE) {
        toggle_now(s, st, &t, st->note[t.voice]);
    } else if (a.kind == ACT_WAVE) {
        wave_now(s, st, &t, a.steps);
    } else if (a.kind == ACT_NOTE) {
        note_now(s, st, &t, a.steps);
    } else {
        unset_now(s, st, &t);
    }
}

/* An action for a step, waiting its turn; turns of the same knob on the same step join. */
static void act(screen* s, step_view* st, uint32_t kind, const target* t, int32_t steps) {
    step_act* last = st->qn ? &st->q[st->qn - 1u] : NULL;
    if (last && last->kind == kind && last->pattern == t->pattern && last->row == t->row &&
        last->voice == t->voice && (kind == ACT_WAVE || kind == ACT_NOTE)) {
        last->steps = (int16_t)(last->steps + steps);
    } else if (st->qn == STEP_VIEW_QUEUE) {
        refuse(s, "WAITING");
        return;
    } else {
        st->q[st->qn++] = (step_act){(uint8_t)kind, t->voice, t->pattern, t->row, (int16_t)steps};
    }
    drain(s, st);
}

/* The note a step's key sounds: the step's own, else the lane's default. */
static uint32_t audition_note(const step_view* st, const target* t, const uint8_t* cell) {
    return is_note(cell[0]) ? cell[0] - GT2_FIRSTNOTE : st->note[t->voice];
}

static void key_down(step_view* st, screen* s, uint32_t k) {
    song_view* sv = st->sv;
    st->held = (uint8_t)k;
    target t;
    if (sv->t && song_view_rec_playing(sv)) {
        /* REC while the song plays: the key is a pad that records the lane's note at the row it
         * is played in (a staccato lane's key off after it), and sounds it */
        uint32_t v = sv->voice;
        (void)song_view_record_note(sv, s, st->note[v], st->instr[v], !(st->gate >> v & 1u));
        if (sv->jam_off && sv->jam_voice != v) {
            (void)sv->t->jam(sv->t->ctx, sv->jam_voice, SONG_T_RELEASE, sv->instr);
        }
        sv->jam_off = 0;
        (void)sv->t->jam(sv->t->ctx, v, st->note[v], st->instr[v]);
        sv->jam_voice = (uint8_t)v;
        st->touched = (uint16_t)(st->touched | 1u << k); /* it toggles nothing at its release */
        return;
    }
    if (!sv->t || !target_of(st, k, &t)) {
        return;
    }
    if (sv->jam_off && sv->jam_voice != t.voice) {
        (void)sv->t->jam(sv->t->ctx, sv->jam_voice, SONG_T_RELEASE, sv->instr);
    }
    sv->jam_off = 0;
    const uint8_t* c = cell_in(gt2_pattern_data(sv->song, t.pattern), t.row);
    uint32_t instr = is_note(c[0]) && c[1] ? c[1] : st->instr[t.voice];
    (void)sv->t->jam(sv->t->ctx, t.voice, audition_note(st, &t, c), instr);
    sv->jam_voice = t.voice;
}

uint32_t step_view_keys(step_view* st, screen* s, uint32_t down, uint32_t pressed, int take) {
    song_view* sv = st->sv;
    uint32_t mask = (1u << STEP_VIEW_STEPS) - 1u;
    uint32_t now = take ? down & mask : 0u;
    uint32_t pressed16 = take ? pressed & mask : 0u;
    /* Let go (or let go and pressed again): a step with no knob turned on it toggles. */
    uint32_t gone = st->down & (~now | pressed16);
    for (uint32_t k = 0; gone >> k; k++) {
        if (!(gone >> k & 1u)) {
            continue;
        }
        target t;
        if (take && !(st->touched >> k & 1u)) {
            if (target_of(st, k, &t)) {
                act(s, st, ACT_TOGGLE, &t, 0);
            } else {
                feedback(s, sv);
            }
        }
        st->touched = (uint16_t)(st->touched & ~(1u << k));
        st->down = (uint16_t)(st->down & ~(1u << k));
        if (sv->t) {
            sv->jam_off = 1; /* the audition goes at the next frame */
        }
    }
    for (uint32_t k = 0; pressed16 >> k; k++) {
        if (pressed16 >> k & 1u) {
            st->down = (uint16_t)(st->down | 1u << k);
            key_down(st, s, k);
        }
    }
    if (!st->down || (take && song_view_rec_playing(sv))) {
        st->held = STEP_VIEW_NONE; /* (the pads of a recording hold no step) */
    } else if (st->held >= STEP_VIEW_STEPS || !(st->down >> st->held & 1u)) {
        st->held = 0u;
        while (!(st->down >> st->held & 1u)) {
            st->held++;
        }
    }
    return pressed16 | now;
}

/* LOCK: a knob's parameter on the held step, for that row only. */
static void set_lock(screen* s, step_view* st, const target* t, uint32_t param, int32_t steps) {
    uint32_t value;
    gt2_song* song = (gt2_song*)st->sv->song;
    if (!lock_get(song, t->pattern, t->row, param, &value)) {
        value = LOCK_START[param];
    }
    int32_t v = (int32_t)value + steps * (int32_t)LOCK_STEP[param];
    int32_t max = (int32_t)song_lock_max(param);
    v = v < 0 ? 0 : v > max ? max : v;
    int r = param == SONG_LOCK_CHANCE && v == 100 ? song_lock_remove(song, t->pattern, t->row, param)
                                                  : song_lock_put(song, t->pattern, t->row, param, (uint32_t)v);
    if (r >= 0) {
        st->rev++;
    } else {
        refuse(s, r == -(int)GT2_ERR_SAVING ? "SAVING" : r == -(int)GT2_ERR_PENDING ? "WAITING" :
                  r == -(int)GT2_ERR_SONG_FULL ? "SONG FULL" : "LOCKS FULL");
    }
}

/* LFO on a held step: its locks and its waveform command go. */
static void clear_step(screen* s, step_view* st, const target* t) {
    gt2_song* song = (gt2_song*)st->sv->song;
    for (uint32_t p = 0; p < SONG_LOCK_PARAMS; p++) {
        int r = song_lock_remove(song, t->pattern, t->row, p);
        if (r < 0) {
            refuse(s, r == -(int)GT2_ERR_SAVING ? "SAVING" : "WAITING");
            return;
        }
        st->rev = (uint8_t)(st->rev + (r > 0));
    }
    act(s, st, ACT_UNSET, t, 0);
}

/* MUTATE or DICE on the selected lane: the run starts, its writes follow in drain. */
static void gen_start(screen* s, step_view* st, uint32_t kind) {
    song_view* sv = st->sv;
    target t;
    song_scene scene;
    if (!target_of(st, 0u, &t) || !sv->editor) {
        feedback(s, sv);
        return;
    }
    if (st->run) {
        refuse(s, "WAITING");
        return;
    }
    st->seed = song_mix(st->seed ^ s->now ^ s->frame);
    st->gen_drums = (uint8_t)(song_ext_get_scene(sv->song, &scene) && t.voice < scene.count &&
                              scene.role[t.voice] == SONG_ROLE_DRUMS);
    st->gen_pattern = t.pattern;
    st->gen_voice = t.voice;
    st->gen_row = 0u;
    dice_get(sv, &st->gen_dens, &st->gen_scale);
    st->run = (uint8_t)kind;
    refuse(s, kind == GEN_MUTATE ? "MUTATED" : "NEW PATTERN");
    drain(s, st);
}

/* K1 or K3 with EDIT + SCL held: the song's DICE density or scale, one detent a step, stopping
 * at the ends; a turn spoils the roll at SCL's release. */
static void dice_turn(screen* s, step_view* st, int scale, int32_t steps) {
    uint8_t d, sc;
    dice_get(st->sv, &d, &sc);
    int32_t top = (int32_t)(scale ? SONG_DICE_SCALES : SONG_DICE_DENSITIES) - 1;
    int32_t v = (int32_t)(scale ? sc : d) + steps;
    v = v < 0 ? 0 : v > top ? top : v;
    st->scl_spoiled = 1u;
    int r = st->sv->song ? song_ext_set_dice((gt2_song*)st->sv->song, scale ? d : (uint32_t)v,
                                             scale ? (uint32_t)v : sc)
                         : -1;
    if (r < 0) {
        refuse(s, r == -(int)GT2_ERR_SAVING ? "SAVING" : "SONG FULL");
    }
}

void step_view_buttons(step_view* st, screen* s, uint32_t held) {
    if (st->scl_down && !(held >> PANEL_BTN_SCL & 1u)) {
        int roll = !st->scl_spoiled && screen_top(s) == &step_view_view;
        st->scl_down = st->scl_spoiled = 0u;
        if (roll) {
            gen_start(s, st, GEN_DICE);
        }
    }
}

static int input(screen* s, void* state, const screen_event* e) {
    step_view* st = state;
    song_view* sv = st->sv;
    if (sv->away) {
        return song_view_away_input(s, e);
    }
    int edit = (int)(e->held >> PANEL_BTN_EDIT & 1u);
    drain(s, st);
    target t;
    if (e->kind == SCREEN_PRESS) {
        if (st->scl_down && e->id != PANEL_BTN_SCL) {
            st->scl_spoiled = 1u; /* another button: a combination of its own, which does nothing */
            return 1;
        }
        if (edit && st->held == STEP_VIEW_NONE && !song_view_recording(sv) &&
            e->id == PANEL_BTN_SCL && sv->editor) {
            st->scl_down = 1u; /* DICE rolls at SCL's release, unless a knob is turned first */
            st->scl_spoiled = 0u;
            return 1;
        }
        if (e->id == PANEL_BTN_LFO && edit) {
            if (sv->editor) {
                (void)song_edit_undo(sv->editor);
                sv->edit_pending = sv->editor->pending;
            }
            feedback(s, sv);
            return 1;
        }
        if (edit && st->held == STEP_VIEW_NONE && !song_view_recording(sv) &&
            e->id == PANEL_BTN_ARP) {
            gen_start(s, st, GEN_MUTATE);
            return 1;
        }
        if (e->id == PANEL_BTN_LFO && st->held != STEP_VIEW_NONE) {
            st->touched = st->down;
            if (target_of(st, st->held, &t)) {
                clear_step(s, st, &t);
            }
            return 1;
        }
        if (e->id == PANEL_BTN_ARP) {
            st->gate ^= (uint8_t)(1u << sv->voice);
            return 1;
        }
        return 0;
    }
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        int32_t v = ((int32_t)sv->voice + e->steps) % (int32_t)SONG_T_VOICES;
        sv->voice = (uint8_t)(v < 0 ? v + (int32_t)SONG_T_VOICES : v);
        sv->instr = st->instr[sv->voice];
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM && st->held != STEP_VIEW_NONE && target_of(st, st->held, &t)) {
        st->touched = st->down; /* a step held: ALGORITHM is its chance, not the page */
        set_lock(s, st, &t, SONG_LOCK_CHANCE, e->steps);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        lane lanes[SONG_T_VOICES];
        lanes_of(sv, lanes);
        if (lanes[sv->voice].play == 0xffu) {
            int32_t p = (int32_t)page_of(st, lanes) + e->steps;
            int32_t last = (int32_t)pages_of(lanes[sv->voice].rows) - 1;
            st->page = (uint8_t)(p < 0 ? 0 : p > last ? last : p);
        }
        return 1;
    }
    if (st->scl_down && (e->id == PANEL_ENC_K1 || e->id == PANEL_ENC_K1 + 2u)) {
        dice_turn(s, st, e->id != PANEL_ENC_K1, e->steps);
        return 1;
    }
    int held = st->held != STEP_VIEW_NONE;
    if (e->id == 6u) { /* PRESETS */
        if (held && target_of(st, st->held, &t)) {
            st->touched = st->down;
            act(s, st, ACT_NOTE, &t, e->steps);
        } else if (!held) {
            int32_t i = (int32_t)st->instr[sv->voice] + e->steps;
            st->instr[sv->voice] = (uint8_t)(i < 1 ? 1 : i > (int32_t)GT2_MAX_INSTR ? (int32_t)GT2_MAX_INSTR : i);
            sv->instr = st->instr[sv->voice];
        }
        return 1;
    }
    if (!held && !edit && (e->id == PANEL_ENC_K1 || e->id == PANEL_ENC_K1 + 1u) &&
        song_view_recording(sv)) {
        /* REC while the song plays: K1 and K2 record the cutoff and the resonance as locks on
         * the row they are turned in (during a count-in they act as in LIVE) */
        st->rev = (uint8_t)(st->rev + song_view_record_knob(sv, s, e->id - PANEL_ENC_K1, e->steps));
        return 1;
    }
    if (e->id < PANEL_ENC_K1 || e->id > PANEL_ENC_K4 || !held) {
        return 0; /* the knobs act as in LIVE until a step is held */
    }
    uint32_t k = e->id - PANEL_ENC_K1;
    st->touched = st->down;
    if (!target_of(st, st->held, &t)) {
        feedback(s, sv);
        return 1;
    }
    if (edit) {
        set_lock(s, st, &t, SONG_LOCK_AGE + k, e->steps);
    } else if (k == 3u) {
        act(s, st, ACT_WAVE, &t, e->steps);
    } else {
        set_lock(s, st, &t, k == 0u ? SONG_LOCK_CUTOFF : k == 1u ? SONG_LOCK_RESONANCE :
                 SONG_LOCK_PULSE, e->steps);
    }
    return 1;
}

/* ---- the popup ---- */

static void copy_text(char out[SCREEN_POPUP_TEXT], const char* text) {
    uint32_t n = 0u;
    while (n < SCREEN_POPUP_TEXT - 1u && text[n]) {
        n++;
    }
    memcpy(out, text, n);
    out[n] = 0;
}

int step_view_popup(const step_view* st, uint32_t encoder, int edit, char name[SCREEN_POPUP_TEXT],
                    char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max) {
    const song_view* sv = st->sv;
    lane lanes[SONG_T_VOICES];
    lanes_of(sv, lanes);
    uint32_t v = sv->voice, page = page_of(st, lanes);
    const lane* ln = &lanes[v];
    char text[SCREEN_POPUP_TEXT] = {0};
    const char* label;
    uint32_t cur = 0u, top = 0u;
    int held = st->held != STEP_VIEW_NONE;
    uint32_t at = page * STEP_VIEW_STEPS + st->held;
    if (st->scl_down && (encoder == PANEL_ENC_K1 || encoder == PANEL_ENC_K1 + 2u)) {
        uint8_t d, sc;
        dice_get(sv, &d, &sc);
        int scale = encoder != PANEL_ENC_K1;
        label = scale ? "SCALE" : "DENSITY";
        cur = scale ? sc : d;
        top = (scale ? SONG_DICE_SCALES : SONG_DICE_DENSITIES) - 1u;
        copy_text(text, scale ? SCALE_NAMES[sc] : DENSITY_NAMES[d]);
    } else if (encoder == PANEL_ENC_ALGORITHM && held && sv->song && ln->pattern != 0xffu && at < ln->rows) {
        label = "CHANCE";
        cur = 100u;
        (void)lock_get(sv->song, ln->pattern, at, SONG_LOCK_CHANCE, &cur);
        top = 100u;
        text[decimal(cur, text)] = '%';
    } else if (encoder == PANEL_ENC_ALGORITHM) {
        label = "PAGE";
        uint32_t pages = pages_of(ln->rows);
        cur = page;
        top = pages - 1u;
        uint32_t n = decimal(page + 1u, text);
        text[n++] = '/';
        decimal(pages, text + n);
    } else if (encoder == 6u) {
        if (held) {
            label = "NOTE";
            const uint8_t* c = sv->song && ln->pattern != 0xffu && at < ln->rows ?
                               cell_of(sv->song, ln, at) : NULL;
            cur = c && is_note(c[0]) ? c[0] - GT2_FIRSTNOTE : st->note[v];
            top = 95u;
            note_name(cur, text);
        } else {
            label = "INSTRUMENT";
            cur = st->instr[v];
            top = GT2_MAX_INSTR;
            song_view_instr_text(sv, cur, text);
        }
    } else if ((encoder == PANEL_ENC_K1 || encoder == PANEL_ENC_K1 + 1u) && !held && !edit &&
               song_view_recording(sv)) {
        uint32_t k = encoder - PANEL_ENC_K1;
        label = k == 0u ? "CUTOFF" : "RESONANCE";
        cur = song_view_rec_value(sv, k);
        top = k == 0u ? 2047u : 15u;
        if (k == 0u) {
            live_view_fc_text(sv->l, cur, text); /* in Hz, as every view says the cutoff */
        } else {
            decimal(cur, text);
        }
    } else if (encoder >= PANEL_ENC_K1 && encoder <= PANEL_ENC_K4 && held && sv->song &&
               ln->pattern != 0xffu && at < ln->rows) {
        uint32_t k = encoder - PANEL_ENC_K1;
        if (edit) {
            label = CHAR_NAMES[k];
            uint32_t p = SONG_LOCK_AGE + k;
            if (!lock_get(sv->song, ln->pattern, at, p, &cur)) {
                cur = LOCK_START[p];
            }
            top = song_lock_max(p);
            decimal(cur, text);
        } else if (k == 3u) {
            label = KNOB_NAMES[3];
            const uint8_t* c = cell_of(sv->song, ln, at);
            for (uint32_t w = 1; c[2] == 7u && w < STEP_VIEW_WAVES; w++) {
                cur = c[3] == WAVES[w] ? w : cur;
            }
            top = STEP_VIEW_WAVES - 1u;
            copy_text(text, WAVE_NAMES[cur]);
        } else {
            label = KNOB_NAMES[k];
            uint32_t p = k == 0u ? SONG_LOCK_CUTOFF : k == 1u ? SONG_LOCK_RESONANCE : SONG_LOCK_PULSE;
            if (!lock_get(sv->song, ln->pattern, at, p, &cur)) {
                cur = LOCK_START[p];
            }
            top = song_lock_max(p);
            decimal(cur, text);
        }
    } else {
        return 0;
    }
    copy_text(name, label);
    copy_text(value, text);
    *fill = cur;
    *max = top;
    return 1;
}

/* ---- the view ---- */

static int model_loading(const song_view* sv) {
    return sv->io && (sv->io->state == SONG_IO_STOP || sv->io->state == SONG_IO_READ ||
                      sv->io->state == SONG_IO_VERIFY);
}

static void enter(screen* s, void* state) {
    step_view* st = state;
    st->sv->built = 0;
    st->sv->edit_screen = s;
    st->held = STEP_VIEW_NONE;
    st->zones = 0u;
    st->run = 0u;
    st->scl_down = st->scl_spoiled = 0u;
    st->down = st->touched = 0u;
    st->sv->instr = st->instr[st->sv->voice];
}

static void leave(screen* s, void* state) {
    step_view* st = state;
    if (st->follow | st->qn) {
        drain(s, st);
    }
    st->qn = st->run = 0u;
    st->scl_down = st->scl_spoiled = 0u;
    st->sv->built = 0;
    st->held = STEP_VIEW_NONE;
    st->down = st->touched = 0u;
}

/* The zones built into the canvas (step_view.zones). */
#define ZONE_HEAD 1u
#define ZONE_LANES 2u
#define ZONE_LOWER 4u
#define ZONES_ALL (ZONE_HEAD | ZONE_LANES | ZONE_LOWER)

static int changed(screen* s, void* state) {
    step_view* st = state;
    song_view* sv = st->sv;
    if (sv->away) {
        return song_view_away_changed(sv);
    }
    if (st->follow | st->qn | st->run) {
        drain(s, st);
    }
    if (model_loading(sv)) {
        return sv->which == SONG_VIEW_STEP && sv->dirty != 0;
    }
    uint8_t now[STEP_VIEW_LOOK];
    look(st, now);
    return !sv->built || sv->dirty || sv->which != SONG_VIEW_STEP || st->zones != ZONES_ALL ||
           memcmp(now, st->drawn, LOWER_AT + LOWER_N) != 0;
}

/* Builds a zone, if its rows are on screen (a wipe down shows a few rows a frame, and the view
 * draws what they reach: the whole canvas in one frame is the busiest pass of a SEQ). */
static void zone(step_view* st, uint32_t bit, uint32_t first_row, uint32_t reach,
                 const uint8_t* now, uint32_t at, uint32_t n, int force) {
    if (first_row >= reach ||
        (!force && (st->zones & bit) && memcmp(now + at, st->drawn + at, n) == 0)) {
        return;
    }
    lane lanes[SONG_T_VOICES];
    lanes_of(st->sv, lanes);
    uint32_t page = page_of(st, lanes);
    st->page = (uint8_t)page; /* stopping leaves the page where the playing step was */
    if (bit == ZONE_HEAD) {
        build_head(st, lanes, page);
    } else if (bit == ZONE_LANES) {
        build_lanes(st, lanes, page);
    } else {
        build_lower(st, lanes, page);
    }
    memcpy(st->drawn + at, now + at, n);
    st->zones |= (uint8_t)bit;
}

static void compose(screen* s, void* state, grid* g) {
    step_view* st = state;
    song_view* sv = st->sv;
    if (sv->away) {
        song_view_away_compose(sv, s, g);
        return;
    }
    if (model_loading(sv)) {
        if (sv->which == SONG_VIEW_STEP) {
            song_view_copy(sv, s, g);
        }
        return;
    }
    song_view_acquire_canvas(sv, SONG_VIEW_STEP);
    if (!sv->built) {
        st->zones = 0u;
    }
    uint8_t now[STEP_VIEW_LOOK];
    look(st, now);
    uint32_t reach = screen_animating(s) ? screen_reach(s) : GRID_ROWS;
    zone(st, ZONE_HEAD, 0u, reach, now, HEAD_AT, HEAD_N, 0);
    zone(st, ZONE_LANES, STEP_VIEW_LANES_ROW, reach, now, LANES_AT, LANES_N, 0);
    zone(st, ZONE_LOWER, STEP_VIEW_HELD_ROW, reach, now, LOWER_AT, LOWER_N, 0);
    sv->built = 1;
    song_view_copy(sv, s, g);
}

const view step_view_view = {"STEP", enter, leave, input, compose, changed};
