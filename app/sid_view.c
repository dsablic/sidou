// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "sid_io.h"
#include "song_io.h"

typedef song_view_canvas canvas;

/* NOW PLAYING's zones (sid_view.zones), each built when what it shows changed: the header (the
 * title, author and released, rows 0-7), the song, its time and state and the chips (rows 8-16),
 * the voice lanes (rows 17-24) and the counters (rows 25-26). */
#define ZONE_HEAD 1u
#define ZONE_PLAY 2u
#define ZONE_LANES 4u
#define ZONE_COUNT 8u
#define ZONES_ALL 15u
#define ZONE_REST 16u /* the rows no zone owns, blanked */
/* Their rows. */
#define HEAD_ROWS 0x000000ffu
#define PLAY_ROWS 0x0001ff00u
#define LANE_ROWS 0x01fe0000u
#define COUNT_ROWS 0x06000000u
/* Where each zone's bytes are in the look (sid_view.drawn), and how many. */
#define LOOK_HEAD 0u
#define LOOK_HEAD_N 4u
#define LOOK_PLAY 4u
#define LOOK_PLAY_N 12u
#define LOOK_LANES 16u
#define LOOK_LANES_N 5u
#define LOOK_COUNT 21u
#define LOOK_COUNT_N 4u
#define LOOK_THEME 25u
_Static_assert(LOOK_THEME < SID_VIEW_LOOK, "NOW PLAYING's look");
/* The rows of NOW PLAYING's parts. */
#define TITLE_ROW 1u
#define AUTHOR_ROW 6u
#define SONG_ROW 9u
#define TIME_ROW 12u
#define CHIPS_ROW 15u
#define LANE_ROW 18u
#define BAR_COL 14u
#define COUNT_ROW 25u
/* The browser's: the files a line every two rows, the status under them, the legends. */
#define FILE_ROW 6u
#define BROWSE_STATUS_ROW 22u
#define LEGEND_ROW 24u
#define BACK_ROW 26u
/* The largest number a counter shows. */
#define COUNT_MAX 9999u

/* ---- small helpers ---- */

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

static void copy_name(char* out, const char* in) {
    uint32_t n = 0;
    while (n < STORE_NAME_MAX && in[n]) {
        out[n] = in[n];
        n++;
    }
    out[n] = 0;
}

/* n (any) in decimal into buf (11 characters). */
static uint32_t decimal(char* buf, uint32_t n) {
    char d[10];
    uint32_t k = 0;
    do {
        d[k++] = (char)('0' + n % 10u);
        n /= 10u;
    } while (n);
    for (uint32_t i = 0; i < k; i++) {
        buf[i] = d[k - 1u - i];
    }
    buf[k] = 0;
    return k;
}

static void small(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = a;
    }
}

static void big(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    (void)grid_canvas_text_big(c, col, row, 2, s, a);
}

static void blank(canvas* c, uint32_t rows, uint8_t a) {
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (rows >> r & 1u) {
            memset(c->ch[r], ' ', GRID_COLS);
            memset(c->attr[r], a, GRID_COLS);
        }
    }
}

/* A name at 2x from column 2, padded to SID_VIEW_NAME_CELLS characters (the selection is a whole
 * bar); a longer one's last is an arrow. */
static void name_line(canvas* c, uint32_t row, const char* name, uint8_t a) {
    char line[SID_VIEW_NAME_CELLS + 1u];
    uint32_t n = 0;
    while (n < SID_VIEW_NAME_CELLS && name[n]) {
        line[n] = name[n];
        n++;
    }
    if (n == SID_VIEW_NAME_CELLS && name[n]) {
        line[n - 1u] = (char)FONT_ARROW_RIGHT;
    }
    while (n < SID_VIEW_NAME_CELLS) {
        line[n++] = ' ';
    }
    line[n] = 0;
    big(c, 2, row, line, a);
}

/* A header string's character as the font shows it: ASCII as itself (the font draws lower case as
 * upper case), anything else a '?'. */
static char shown(char ch) {
    return (uint8_t)ch >= 0x20u && (uint8_t)ch < 0x7fu ? ch : '?';
}

/* The title on two lines of SID_VIEW_NAME_CELLS: the first broken at its last space within them
 * when the title is longer, the second the rest, its last character an arrow when it is longer
 * still. */
static void title_lines(const char* t, char l1[SID_VIEW_NAME_CELLS + 1u],
                        char l2[SID_VIEW_NAME_CELLS + 1u]) {
    uint32_t n = length(t), cut = n;
    if (n > SID_VIEW_NAME_CELLS) {
        cut = SID_VIEW_NAME_CELLS;
        for (uint32_t k = SID_VIEW_NAME_CELLS; k > 0u; k--) {
            if (t[k] == ' ') {
                cut = k;
                break;
            }
        }
    }
    uint32_t i = 0;
    for (; i < cut && i < SID_VIEW_NAME_CELLS; i++) {
        l1[i] = shown(t[i]);
    }
    l1[i] = 0;
    const char* rest = t + cut;
    while (*rest == ' ') {
        rest++;
    }
    uint32_t m = length(rest);
    for (i = 0; i < m && i < SID_VIEW_NAME_CELLS; i++) {
        l2[i] = shown(rest[i]);
    }
    if (m > SID_VIEW_NAME_CELLS) {
        l2[SID_VIEW_NAME_CELLS - 1u] = (char)FONT_ARROW_RIGHT;
    }
    l2[i] = 0;
}

/* A header string at 1x from column 2, within the border: 27 characters. */
static void line_1x(canvas* c, uint32_t row, const char* s, uint8_t a) {
    char t[GRID_COLS - 2u];
    uint32_t i = 0;
    for (; s[i] && i + 1u < sizeof t; i++) {
        t[i] = shown(s[i]);
    }
    t[i] = 0;
    small(c, 2, row, t, a);
}

/* ---- the player ---- */

static void status_of(const sid_view* sd, sid_status* st) {
    memset(st, 0, sizeof *st);
    if (sd->t) {
        sd->t->status(sd->t->ctx, st);
    }
}

void sid_view_init(sid_view* sd, song_view* sv) {
    memset(sd, 0, sizeof *sd);
    sd->sv = sv;
}

void sid_view_set_transport(sid_view* sd, const sid_transport* t) {
    sd->t = t;
    sd->sv->tune = t;
}

uint32_t sid_view_state(const sid_view* sd) {
    sid_status st;
    status_of(sd, &st);
    return st.state;
}

int sid_view_is(const view* v) {
    return v == &sid_view_browser || v == &sid_view_playing;
}

int sid_view_play(sid_view* sd, int edit, int here) {
    sid_status st;
    status_of(sd, &st);
    const sid_transport* t = sd->t;
    switch (st.state) {
    case SID_T_NONE: return 0;
    case SID_T_PLAYING: (void)(edit ? t->pause(t->ctx) : t->stop(t->ctx)); break;
    case SID_T_PAUSED: (void)(edit ? t->resume(t->ctx) : t->stop(t->ctx)); break;
    case SID_T_STOPPED:
    case SID_T_FAILED:
        /* Here, or EDIT held: the song from init; in another view the tune gives way. */
        (void)(here || edit ? t->start(t->ctx, st.song) : t->end(t->ctx));
        break;
    case SID_T_LOADING:
        if (!edit) {
            (void)t->stop(t->ctx); /* it stops as it starts */
        }
        break;
    default: break; /* ending: the song comes back */
    }
    return 1;
}

/* ---- SIDS: the browser ---- */

/* Page `page` read again from the store's first file, the pages before it skipped: one store
 * entry a frame (scan_step). */
static void browse(sid_view* sd, uint8_t page) {
    sd->page = page;
    sd->skip = (uint8_t)(page * SID_VIEW_FILES);
    sd->count = 0;
    sd->after[0] = 0;
    sd->scanning = 1;
    sd->built = 0;
}

/* The scan's next .sid file: the page's own after the ones before it (one a frame). */
static void scan_step(sid_view* sd) {
    store_file f;
    memset(&f, 0, sizeof f);
    copy_name(f.name, sd->after);
    if (!sd->st || !sid_io_next(sd->st, &f)) {
        sd->scanning = 0;
        return;
    }
    copy_name(sd->after, f.name);
    if (sd->skip) {
        sd->skip--;
        return;
    }
    copy_name(sd->files[sd->count++], f.name);
    if (sd->count == SID_VIEW_FILES) {
        sd->scanning = 0;
    }
}

void sid_view_open(sid_view* sd, screen* s) {
    sd->asking = sd->saving = 0;
    browse(sd, sd->page);
    screen_push(s, &sid_view_browser, sd, SCREEN_WIPE_DOWN);
}

/* The store's tune `name` to the player, and NOW PLAYING on top if it took it (its refusals are
 * the binding's toasts). */
static void load(sid_view* sd, screen* s, const char* name) {
    sd->asking = sd->saving = 0;
    sd->built = 0;
    if (sd->t && sd->t->play(sd->t->ctx, name)) {
        screen_push(s, &sid_view_playing, sd, SCREEN_WIPE_DOWN);
    }
}

/* The highlighted tune chosen (ALGORITHM clockwise, SAVE): the loaded one opens NOW PLAYING as it
 * is; another loads, after the question when the song has unsaved edits of its own (a sound edit
 * is not one: the tune leaves the keys as they are) and has its memory. */
static void choose(sid_view* sd, screen* s) {
    if (sd->asking) {
        load(sd, s, sd->chosen); /* ALGORITHM again: the song's edits go */
        return;
    }
    if (sd->scanning || !sd->count) {
        return;
    }
    const char* name = sd->files[sd->cursor];
    sid_status st;
    status_of(sd, &st);
    if (st.state != SID_T_NONE && strcmp(st.name, name) == 0) {
        screen_push(s, &sid_view_playing, sd, SCREEN_WIPE_DOWN);
        return;
    }
    const song_io* io = sd->sv->io;
    if (st.state == SID_T_NONE && io && song_io_edited(io)) {
        copy_name(sd->chosen, name);
        sd->asking = 1;
        sd->saving = 0;
        sd->built = 0;
        return;
    }
    load(sd, s, name);
}

static int browser_input(screen* s, void* state, const screen_event* e) {
    sid_view* sd = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        if (sd->asking) {
            return 0; /* nothing to move on the question */
        }
        int32_t next = (int32_t)sd->cursor + e->steps;
        if (!sd->scanning && next >= (int32_t)SID_VIEW_FILES && sd->count == SID_VIEW_FILES &&
            (uint32_t)(sd->page + 1u) * SID_VIEW_FILES < STORE_KEYS_MAX) {
            browse(sd, (uint8_t)(sd->page + 1u));
            sd->cursor = 0;
        } else if (next < 0 && sd->page) {
            browse(sd, (uint8_t)(sd->page - 1u));
            sd->cursor = SID_VIEW_FILES - 1u;
        } else {
            int32_t last = sd->count ? (int32_t)sd->count - 1 : 0;
            sd->cursor = (uint8_t)(next < 0 ? 0 : next > last ? last : next);
            sd->built = 0;
        }
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        if (e->steps > 0) {
            choose(sd, s);
        }
        return 1;
    }
    return 0;
}

int sid_view_home(sid_view* sd) {
    if (sd->asking) {
        sd->asking = sd->saving = 0;
        sd->built = 0;
        return 1;
    }
    uint32_t st = sid_view_state(sd);
    if (st == SID_T_STOPPED || st == SID_T_FAILED) {
        (void)sd->t->end(sd->t->ctx); /* out of the browser: the song back */
    }
    return 0;
}

int sid_view_save(sid_view* sd, screen* s) {
    if (!sd->asking) {
        choose(sd, s);
        return 1;
    }
    song_io* io = sd->sv->io;
    if (!io) {
        return 1;
    }
    sd->built = 0;
    if (!io->name[0]) {
        sd->saving = 1; /* SAVE AS first (the views open it), then the tune */
        return SID_VIEW_SAVE_AS;
    }
    if (song_io_save(io, io->name, s->now)) {
        sd->saving = 1;
    }
    return 1;
}

void sid_view_frame(sid_view* sd, screen* s) {
    const song_io* io = sd->sv->io;
    if (!sd->asking || !sd->saving || !io || song_io_busy(io) ||
        screen_top(s) != &sid_view_browser) {
        return;
    }
    if (!song_io_edited(io)) {
        load(sd, s, sd->chosen); /* saved: the tune */
    } else {
        sd->saving = 0; /* not saved (refused, failed, SAVE AS left): the question again */
        sd->built = 0;
    }
}

/* The loaded tune's mark in the browser: its line on this page and its state; 0xff none. */
static uint8_t mark_of(const sid_view* sd, const sid_status* st) {
    if (st->state == SID_T_NONE || sd->scanning) {
        return 0xffu;
    }
    for (uint32_t i = 0; i < sd->count; i++) {
        if (strcmp(sd->files[i], st->name) == 0) {
            return (uint8_t)(i << 4 | (st->state == SID_T_PLAYING || st->state == SID_T_LOADING));
        }
    }
    return 0xfeu;
}

/* What else the browser shows: the question, its save, the song files' state. */
static uint8_t browse_flags(const sid_view* sd) {
    const song_io* io = sd->sv->io;
    return (uint8_t)(sd->asking | sd->saving << 1 | (io && song_io_busy(io)) << 2 |
                     (io && song_io_saving(io)) << 3);
}

static void build_browser(sid_view* sd, uint8_t mark) {
    song_view* sv = sd->sv;
    canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    const uint8_t text = THEME_ATTR(th, menu_item);
    const uint8_t label = THEME_ATTR(th, label);
    const uint8_t legend = THEME_ATTR(th, hint);
    const uint8_t selected = GRID_ATTR(th->sel_fg, th->sel_bg);
    blank(c, (1u << GRID_ROWS) - 1u, text);
    big(c, 2, 2, "SIDS", GRID_ATTR(th->title_fg, th->title_bg));
    const song_io* io = sv->io;
    if (io && song_io_busy(io)) {
        big(c, 2, 4, song_io_saving(io) ? "SAVING" : "BUSY", THEME_ATTR(th, hot));
    }
    if (sd->asking) {
        /* The song would lose its edits: its name, and what each answer does. */
        big(c, 2, FILE_ROW, "SONG NOT SAVED", THEME_ATTR(th, alert));
        name_line(c, FILE_ROW + 2u, io && io->name[0] ? io->name : "NEW SONG", THEME_ATTR(th, value));
        big(c, 2, FILE_ROW + 6u, "SAVE   SAVE IT", legend);
        big(c, 2, FILE_ROW + 8u, "ALG    DISCARD", legend);
        big(c, 2, FILE_ROW + 10u, "HOME    CANCEL", legend);
    } else {
        for (uint32_t i = 0; i < sd->count; i++) {
            name_line(c, FILE_ROW + 2u * i, sd->files[i], i == sd->cursor ? selected : text);
        }
        if (mark < 0xfeu) {
            /* the loaded tune: a play mark while it plays, a square while it waits */
            const char m[2] = {(char)(mark & 1u ? FONT_PLAY : FONT_SQUARE), 0};
            big(c, 0, FILE_ROW + 2u * (mark >> 4), m, THEME_ATTR(th, hot));
        }
        if (!sd->scanning && !sd->count && !sd->page) {
            big(c, 2, FILE_ROW, "NO TUNES:", label);
            big(c, 2, FILE_ROW + 2u, "SEE MANUAL", label);
        }
        char page[] = {'P', 'A', 'G', 'E', ' ', (char)('1' + sd->page), ' ',
                       sd->page ? (char)FONT_ARROW_UP : ' ',
                       sd->count == SID_VIEW_FILES ? (char)FONT_ARROW_DOWN : ' ', 0};
        big(c, 2, BROWSE_STATUS_ROW,
            sd->scanning ? "READING" : sd->page || sd->count == SID_VIEW_FILES ? page : "", label);
        big(c, 2, LEGEND_ROW, "SAVE CHOOSE", legend);
    }
    big(c, 2, BACK_ROW, "HOME BACK", legend);
    sv->dirty = (1u << GRID_ROWS) - 1u;
}

static int browser_changed(screen* s, void* state) {
    (void)s;
    const sid_view* sd = state;
    const song_view* sv = sd->sv;
    sid_status st;
    status_of(sd, &st);
    return !sd->built || sd->scanning || sv->dirty || sv->which != SONG_VIEW_CANVAS_SIDS ||
           sd->drawn_theme != sv->l->theme || sd->mark != mark_of(sd, &st) ||
           sd->flags != browse_flags(sd);
}

static void browser_compose(screen* s, void* state, grid* g) {
    sid_view* sd = state;
    song_view* sv = sd->sv;
    if (song_view_acquire_canvas(sv, SONG_VIEW_CANVAS_SIDS)) {
        sd->built = 0;
    }
    if (sd->scanning && !screen_animating(s)) {
        /* (not under the wipe: a line that changes in what it has revealed would be sent
         * with the rows it reveals) */
        scan_step(sd);
        sd->built = 0;
        if (!sd->scanning && sd->cursor >= sd->count) {
            sd->cursor = (uint8_t)(sd->count ? sd->count - 1u : 0u);
        }
    }
    sid_status st;
    status_of(sd, &st);
    uint8_t mark = mark_of(sd, &st);
    uint8_t flags = browse_flags(sd);
    if (!sd->built || sv->which != SONG_VIEW_CANVAS_SIDS || sd->drawn_theme != sv->l->theme ||
        mark != sd->mark || flags != sd->flags) {
        build_browser(sd, mark);
        sd->built = 1;
        sd->mark = mark;
        sd->flags = flags;
        sd->drawn_theme = sv->l->theme;
    }
    song_view_copy(sv, s, g);
}

static void browser_enter(screen* s, void* state) {
    (void)s;
    sid_view* sd = state;
    sd->built = 0;
}

const view sid_view_browser = {"SIDS", browser_enter, NULL, browser_input, browser_compose,
                               browser_changed};

/* ---- NOW PLAYING ---- */

/* The song shown: the one playing or asked for, else the header's start song. */
static uint32_t song_of(const sid_status* st) {
    return st->song ? st->song : st->h ? st->h->start : 0u;
}

static uint32_t capped(uint32_t n) {
    return n > COUNT_MAX ? COUNT_MAX : n;
}

/* What NOW PLAYING shows, as bytes: each zone's (the header's as a hash of the file's name and
 * whether its header is read), and the theme. */
static void playing_look(const sid_view* sd, const sid_status* st, uint8_t out[SID_VIEW_LOOK]) {
    memset(out, 0, SID_VIEW_LOOK);
    uint32_t hash = 2166136261u;
    for (uint32_t i = 0; st->name[i]; i++) {
        hash = (hash ^ (uint8_t)st->name[i]) * 16777619u;
    }
    hash ^= st->h ? 1u : 0u;
    hash ^= st->state == SID_T_NONE ? 2u : 0u;
    memcpy(out + LOOK_HEAD, &hash, LOOK_HEAD_N);
    uint8_t* p = out + LOOK_PLAY;
    const psid_header* h = st->h;
    uint32_t song = song_of(st), songs = h ? h->songs : 0u;
    p[0] = st->state;
    p[1] = (uint8_t)song;
    p[2] = (uint8_t)(song >> 8);
    p[3] = (uint8_t)songs;
    p[4] = (uint8_t)(songs >> 8);
    p[5] = (uint8_t)st->seconds;
    p[6] = (uint8_t)(st->seconds >> 8);
    p[7] = (uint8_t)(st->seconds >> 16);
    p[8] = (uint8_t)(h && h->clock == 2u);
    p[9] = (uint8_t)(st->model[0] | st->model[1] << 1 | st->chips << 2);
    p[10] = (uint8_t)(h && h->sid3);
    uint8_t* l = out + LOOK_LANES;
    l[0] = sd->sv->mute;
    l[1] = sd->sv->solo;
    for (uint32_t v = 0; v < 3u; v++) {
        l[2u + v] = (uint8_t)((uint32_t)st->env[v] * (SID_VIEW_BAR_CHARS * 8u) / 255u);
    }
    uint8_t* k = out + LOOK_COUNT;
    uint32_t over = capped(st->overruns), late = capped(st->late);
    k[0] = (uint8_t)over;
    k[1] = (uint8_t)(over >> 8);
    k[2] = (uint8_t)late;
    k[3] = (uint8_t)(late >> 8);
    out[LOOK_THEME] = sd->sv->l->theme;
}

static void build_head(sid_view* sd, const sid_status* st, const theme* th) {
    canvas* c = &sd->sv->cv;
    blank(c, HEAD_ROWS, THEME_ATTR(th, value));
    char l1[SID_VIEW_NAME_CELLS + 1u], l2[SID_VIEW_NAME_CELLS + 1u];
    if (st->h) {
        title_lines(st->h->name, l1, l2);
        line_1x(c, AUTHOR_ROW, st->h->author, THEME_ATTR(th, label));
        line_1x(c, AUTHOR_ROW + 1u, st->h->released, THEME_ATTR(th, label));
    } else {
        title_lines(st->state == SID_T_NONE ? "NO TUNE" : st->name, l1, l2);
    }
    big(c, 2, TITLE_ROW, l1, THEME_ATTR(th, value));
    big(c, 2, TITLE_ROW + 2u, l2, THEME_ATTR(th, value));
}

static void build_play(sid_view* sd, const sid_status* st, const theme* th) {
    canvas* c = &sd->sv->cv;
    blank(c, PLAY_ROWS, THEME_ATTR(th, value));
    const psid_header* h = st->h;
    if (st->state == SID_T_NONE) {
        return;
    }
    if (h) {
        char t[24] = "SONG ";
        decimal(t + 5, song_of(st));
        uint32_t n = length(t);
        t[n++] = '/';
        decimal(t + n, h->songs);
        big(c, 2, SONG_ROW, t, THEME_ATTR(th, value));
    }
    /* The time since the song started, M:SS, and the state at the right. */
    char t[16];
    decimal(t, st->seconds / 60u);
    uint32_t n = length(t);
    t[n++] = ':';
    t[n++] = (char)('0' + st->seconds % 60u / 10u);
    t[n++] = (char)('0' + st->seconds % 10u);
    t[n] = 0;
    big(c, 2, TIME_ROW, t, THEME_ATTR(th, value));
    static const char* const WORD[] = {"", "LOADING", "PLAYING", "PAUSED", "STOPPED", "FAILED", "ENDING"};
    static const uint8_t COLOUR[] = {0, 1, 2, 0, 1, 3, 1}; /* value, label, hot, alert */
    uint32_t s = st->state < 7u ? st->state : 0u;
    const uint8_t roles[4] = {THEME_ATTR(th, value), THEME_ATTR(th, label), THEME_ATTR(th, hot),
                              THEME_ATTR(th, alert)};
    big(c, GRID_COLS - 2u * length(WORD[s]), TIME_ROW, WORD[s], roles[COLOUR[s]]);
    if (!h) {
        return;
    }
    /* The clock and each chip's model; a 2SID tune's mark, a third SID's. */
    const uint8_t label = THEME_ATTR(th, label);
    small(c, 2, CHIPS_ROW, h->clock == 2u ? "NTSC" : "PAL", label);
    small(c, 8, CHIPS_ROW, st->model[0] == SID_MODEL_8580 ? "8580" : "6581", THEME_ATTR(th, value));
    if (st->chips == 2u) {
        small(c, 13, CHIPS_ROW, st->model[1] == SID_MODEL_8580 ? "8580" : "6581",
              THEME_ATTR(th, value));
        small(c, 18, CHIPS_ROW, "2SID", THEME_ATTR(th, hot));
    }
    if (h->sid3) {
        small(c, 2, CHIPS_ROW + 1u, "3SID: 3RD SILENT", THEME_ATTR(th, alert));
    }
}

/* The voices: the number in the voice's colour (dark grey while GLO silences it, reversed while
 * soloed), MUT or SOL, and chip A's envelope as a bar. */
static void build_lanes(sid_view* sd, const sid_status* st, const theme* th) {
    const song_view* sv = sd->sv;
    canvas* c = &sd->sv->cv;
    blank(c, LANE_ROWS, THEME_ATTR(th, value));
    if (st->state == SID_T_NONE) {
        return;
    }
    uint32_t silent = song_view_mute_mask(sv);
    for (uint32_t v = 0; v < 3u; v++) {
        uint32_t row = LANE_ROW + 2u * v;
        int off = (int)(silent >> v & 1u);
        const char digit[2] = {(char)('1' + v), 0};
        big(c, 2, row, digit, sv->solo >> v & 1u ? GRID_ATTR(th->bg, th->hot)
                              : off              ? THEME_ATTR(th, dim)
                                                 : GRID_ATTR(th->voice[v], th->bg));
        if (sv->solo >> v & 1u) {
            big(c, 6, row, "SOL", THEME_ATTR(th, value));
        } else if (sv->mute >> v & 1u) {
            big(c, 6, row, "MUT", THEME_ATTR(th, alert));
        }
        char bar[SID_VIEW_BAR_CHARS + 1u];
        uint32_t px = (uint32_t)st->env[v] * (SID_VIEW_BAR_CHARS * 8u) / 255u;
        for (uint32_t i = 0; i < SID_VIEW_BAR_CHARS; i++) {
            uint32_t fill = px >= 8u ? 8u : px;
            bar[i] = (char)(FONT_BAR0 + fill);
            px -= fill;
        }
        bar[SID_VIEW_BAR_CHARS] = 0;
        big(c, BAR_COL, row, bar, GRID_ATTR(off ? th->dim : th->voice[v], th->bg));
    }
}

static void build_count(sid_view* sd, const sid_status* st, const theme* th) {
    canvas* c = &sd->sv->cv;
    blank(c, COUNT_ROWS, THEME_ATTR(th, value));
    char t[24];
    if (st->overruns) {
        memcpy(t, "OVERRUNS ", 9);
        decimal(t + 9, capped(st->overruns));
        small(c, 2, COUNT_ROW, t, THEME_ATTR(th, alert));
    }
    if (st->late) {
        memcpy(t, "LATE ", 5);
        decimal(t + 5, capped(st->late));
        small(c, 17, COUNT_ROW, t, THEME_ATTR(th, alert));
    }
}

static int playing_changed(screen* s, void* state) {
    (void)s;
    const sid_view* sd = state;
    const song_view* sv = sd->sv;
    if (sv->which != SONG_VIEW_CANVAS_PLAYING || (sd->zones & ZONES_ALL) != ZONES_ALL || sv->dirty) {
        return 1;
    }
    sid_status st;
    status_of(sd, &st);
    uint8_t now[SID_VIEW_LOOK];
    playing_look(sd, &st, now);
    return memcmp(now, sd->drawn, SID_VIEW_LOOK) != 0;
}

static void playing_compose(screen* s, void* state, grid* g) {
    sid_view* sd = state;
    song_view* sv = sd->sv;
    if (song_view_acquire_canvas(sv, SONG_VIEW_CANVAS_PLAYING)) {
        sd->zones = 0;
    }
    sid_status st;
    status_of(sd, &st);
    uint8_t now[SID_VIEW_LOOK];
    playing_look(sd, &st, now);
    const theme* th = theme_get(sv->l->theme);
    if (now[LOOK_THEME] != sd->drawn[LOOK_THEME]) {
        sd->zones = 0;
    }
    /* Each zone when its bytes changed, rows on screen only while a wipe reveals them. */
    uint32_t reach = screen_animating(s) ? screen_reach(s) : GRID_ROWS;
    static const uint8_t AT[4] = {LOOK_HEAD, LOOK_PLAY, LOOK_LANES, LOOK_COUNT};
    static const uint8_t N[4] = {LOOK_HEAD_N, LOOK_PLAY_N, LOOK_LANES_N, LOOK_COUNT_N};
    static const uint8_t FIRST[4] = {0u, 8u, 17u, 25u};
    static const uint32_t ROWS[4] = {HEAD_ROWS, PLAY_ROWS, LANE_ROWS, COUNT_ROWS};
    for (uint32_t z = 0; z < 4u; z++) {
        uint32_t bit = 1u << z;
        /* Under a wipe only a zone not built yet: what changes in what it has revealed would go
         * with the rows it reveals. */
        if (FIRST[z] >= reach || ((sd->zones & bit) && (reach < GRID_ROWS ||
                                   memcmp(now + AT[z], sd->drawn + AT[z], N[z]) == 0))) {
            continue;
        }
        if (z == 0u) {
            build_head(sd, &st, th);
        } else if (z == 1u) {
            build_play(sd, &st, th);
        } else if (z == 2u) {
            build_lanes(sd, &st, th);
        } else {
            build_count(sd, &st, th);
        }
        memcpy(sd->drawn + AT[z], now + AT[z], N[z]);
        sd->zones |= (uint8_t)bit;
        sv->dirty |= ROWS[z];
    }
    /* The rows no zone owns: blank once. */
    if (!(sd->zones & ZONE_REST)) {
        uint32_t rest = ~(HEAD_ROWS | PLAY_ROWS | LANE_ROWS | COUNT_ROWS) & ((1u << GRID_ROWS) - 1u);
        blank(&sv->cv, rest, THEME_ATTR(th, value));
        sv->dirty |= rest;
        sd->zones |= ZONE_REST;
    }
    sd->drawn[LOOK_THEME] = now[LOOK_THEME];
    song_view_copy(sv, s, g);
}

/* ALGORITHM steps the song (restarting it from init); clamped at both ends. */
static int playing_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    sid_view* sd = state;
    if (e->kind != SCREEN_TURN || e->id != PANEL_ENC_ALGORITHM) {
        return 0;
    }
    sid_status st;
    status_of(sd, &st);
    if (!st.h || st.state == SID_T_NONE || st.state == SID_T_ENDING) {
        return 1;
    }
    int32_t to = (int32_t)song_of(&st) + e->steps;
    to = to < 1 ? 1 : to > (int32_t)st.h->songs ? (int32_t)st.h->songs : to;
    if ((uint32_t)to != song_of(&st)) {
        (void)sd->t->start(sd->t->ctx, (uint32_t)to);
    }
    return 1;
}

static void playing_enter(screen* s, void* state) {
    (void)s;
    sid_view* sd = state;
    sd->zones = 0;
}

const view sid_view_playing = {"NOW PLAYING", playing_enter, NULL, playing_input,
                               playing_compose, playing_changed};

/* ---- STATUS's rows ---- */

int sid_view_status_look(const sid_view* sd, uint8_t drawn[SID_VIEW_STATUS_LOOK], int update) {
    sid_status st;
    status_of(sd, &st);
    uint8_t now[SID_VIEW_STATUS_LOOK];
    uint32_t over = capped(st.overruns), late = capped(st.late), drop = capped(st.dropped);
    now[0] = st.state;
    now[1] = (uint8_t)st.cycles;
    now[2] = (uint8_t)(st.cycles >> 8);
    now[3] = (uint8_t)(st.cycles >> 16);
    now[4] = (uint8_t)over;
    now[5] = (uint8_t)(over >> 8);
    now[6] = (uint8_t)late;
    now[7] = (uint8_t)(late >> 8);
    now[8] = (uint8_t)drop;
    now[9] = (uint8_t)(drop >> 8);
    now[10] = st.shadow;
    now[11] = st.chips;
    now[12] = (uint8_t)st.load;
    now[13] = (uint8_t)(st.load >> 8);
    now[14] = (uint8_t)st.ticks_k;
    now[15] = (uint8_t)(st.ticks_k >> 8);
    int changed = memcmp(now, drawn, SID_VIEW_STATUS_LOOK) != 0;
    if (update) {
        memcpy(drawn, now, SID_VIEW_STATUS_LOOK);
    }
    return changed;
}

/* One of STATUS's lines at 2x: the label from column 2, the value ending at column 28. */
static void status_line(grid* g, const theme* th, uint32_t row, const char* label, const char* value) {
    uint32_t n = 0;
    while (value[n]) {
        n++;
    }
    grid_text_big(g, 2, row, 2, label, THEME_ATTR(th, label));
    grid_text_big(g, 28u - 2u * n, row, 2, value, THEME_ATTR(th, value));
}

void sid_view_status_draw(const sid_view* sd, grid* g, const theme* th, uint32_t row) {
    sid_status st;
    status_of(sd, &st);
    grid_fill(g, 1, row, GRID_COLS - 2u, STATUS_LINES * STATUS_LINE_ROWS - 1u, ' ', THEME_ATTR(th, value));
    static const char* const WORD[] = {"", "LOADING", "PLAYING", "PAUSED", "STOPPED", "FAILED", "ENDING"};
    status_line(g, th, row, "", WORD[st.state < 7u ? st.state : 0u]);
    grid_text_big(g, 2, row, 2, "6502", THEME_ATTR(th, hot));
    char t[12];
    char m[8];
    /* the main loop's share (tenths of a percent) and the ticks a thousand cycles took */
    if (st.load) {
        uint32_t k = decimal(m, st.load / 10u);
        m[k++] = '.';
        m[k++] = (char)('0' + st.load % 10u);
        m[k++] = '%';
        m[k] = 0;
        status_line(g, th, row + STATUS_LINE_ROWS, "CPU", m);
        decimal(t, st.ticks_k);
        status_line(g, th, row + 2u * STATUS_LINE_ROWS, "T/KC", t);
    } else {
        status_line(g, th, row + STATUS_LINE_ROWS, "CPU", "--");
        status_line(g, th, row + 2u * STATUS_LINE_ROWS, "T/KC", "--");
    }
    decimal(t, capped(st.late));
    status_line(g, th, row + 3u * STATUS_LINE_ROWS, "LATE", t);
    decimal(t, capped(st.overruns));
    status_line(g, th, row + 4u * STATUS_LINE_ROWS, "OVERRUNS", t);
    decimal(t, st.cycles);
    status_line(g, th, row + 5u * STATUS_LINE_ROWS, "CYC/S", t);
    status_line(g, th, row + 6u * STATUS_LINE_ROWS, "SHADOW", st.shadow ? "ON" : "OFF");
    decimal(t, capped(st.dropped));
    status_line(g, th, row + 7u * STATUS_LINE_ROWS, "3RD SID", t);
}
