// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_gen.h"

#include <string.h>

#include "font.h"
#include "song/gt2_song.h"
#include "song_view.h"
#include "theme.h"

/* DICE's settings by name (the frame and the popup). */
static const char* const DENSITY_NAMES[SONG_DICE_DENSITIES] = {"SPARSE", "LIGHT", "MEDIUM", "BUSY",
                                                               "FULL"};
static const char* const SCALE_NAMES[SONG_DICE_SCALES] = {"MIN PENT", "MAJ PENT", "MINOR",
                                                          "MAJOR",    "DORIAN",   "CHROMATIC"};
static const char* const RANGE_NAMES[SONG_DICE_RANGES] = {"1 OCT", "2 OCT", "3 OCT", "BASS"};
static const char* const RHYTHM_NAMES[SONG_DICE_RHYTHMS] = {"STRAIGHT", "OFFBEAT", "3-3-2", "FOUR",
                                                            "BROKEN"};
/* The frame's lines, K1-K4: their labels, the popup's names, their tops. */
static const char* const DICE_LABELS[4] = {"DENS", "RANGE", "SCALE", "RHYTHM"};
static const char* const DICE_NAMES[4] = {"DENSITY", "RANGE", "SCALE", "RHYTHM"};
static const uint8_t DICE_TOPS[4] = {SONG_DICE_DENSITIES - 1u, SONG_DICE_RANGES - 1u,
                                     SONG_DICE_SCALES - 1u, SONG_DICE_RHYTHMS - 1u};
static const char NOTE_NAMES[12][3] = {"C-", "C#", "D-", "D#", "E-", "F-",
                                       "F#", "G-", "G#", "A-", "A#", "B-"};

/* DICE's scales (0 is the minor pentatonic it always rolled): the length, then the semitones
 * above the root. */
static const uint8_t SCALES[SONG_DICE_SCALES][13] = {
    {5, 0, 3, 5, 7, 10},          {5, 0, 2, 4, 7, 9},         {7, 0, 2, 3, 5, 7, 8, 10},
    {7, 0, 2, 4, 5, 7, 9, 11},    {7, 0, 2, 3, 5, 7, 9, 10},  {12, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11}};
/* STRAIGHT's grid by density (0 sparse .. 4 every step; 2 is what it always rolled): the chance in
 * percent of a hit on a row's place in its beat. A melodic lane hits on the eighths, a drum lane
 * on the sixteenths with a heavy first one. */
static const uint8_t GRID_MELODIC[SONG_DICE_DENSITIES][4] = {
    {60, 0, 0, 0}, {85, 0, 25, 0}, {85, 0, 55, 0}, {90, 35, 75, 35}, {100, 100, 100, 100}};
static const uint8_t GRID_DRUMS[SONG_DICE_DENSITIES][4] = {
    {80, 0, 15, 0}, {90, 5, 30, 10}, {90, 15, 50, 25}, {95, 40, 75, 55}, {100, 100, 100, 100}};
/* The other rhythms (the chunk's 1-4): the chance in percent of a hit on each place of two beats
 * of sixteenths as MEDIUM rolls it, melodic and drum lanes alike. The density scales them: a
 * weight times RHYTHM_MUL in percent, plus RHYTHM_ADD (FULL: every step). */
static const uint8_t RHYTHMS[SONG_DICE_RHYTHMS - 1u][8] = {
    {10, 0, 90, 0, 10, 0, 90, 20},   /* OFFBEAT: the eighths between the beats */
    {95, 0, 0, 85, 0, 0, 85, 0},     /* 3-3-2: the tresillo */
    {100, 0, 20, 0, 100, 0, 20, 0},  /* FOUR: on every beat */
    {90, 0, 30, 65, 0, 45, 75, 20}}; /* BROKEN: a breakbeat's pushes */
static const uint8_t RHYTHM_MUL[SONG_DICE_DENSITIES] = {50, 75, 100, 100, 100};
static const uint8_t RHYTHM_ADD[SONG_DICE_DENSITIES] = {0, 0, 0, 20, 100};
#define GEN_PHI 0x9e3779b9u
/* GoatTracker's FILTER CUTOFF command, the sweep's. */
#define CMD_CUTOFF 0x0Cu

void song_gen_init(song_gen* g, const uint8_t* gate) {
    memset(g, 0, sizeof *g);
    for (uint32_t v = 0; v < SONG_GEN_LANES; v++) {
        g->root[v] = 36u;
    }
    g->gate = gate;
}

void song_gen_dice(const song_view* sv, song_dice* d) {
    static const song_dice NONE = {SONG_DICE_DENSITY_DEFAULT, {0u, 0u, 0u}, 0u,
                                   SONG_DICE_RANGE_DEFAULT};
    if (sv->song) {
        (void)song_ext_get_dice(sv->song, d);
    } else {
        *d = NONE;
    }
}

int song_gen_drums(const song_view* sv, uint32_t voice) {
    song_scene scene;
    return sv->song && song_ext_get_scene(sv->song, &scene) && voice < scene.count &&
           scene.role[voice] == SONG_ROLE_DRUMS;
}

/* Knob k's setting of d (K1 density, K2 range, K3 the lane's scale, K4 rhythm), and its name. */
static uint8_t* dice_field(song_dice* d, uint32_t k, uint32_t voice) {
    return k == 0u ? &d->density : k == 1u ? &d->range : k == 2u ? &d->scale[voice] : &d->rhythm;
}

static const char* dice_text(const song_dice* d, uint32_t k, uint32_t voice) {
    return k == 0u ? DENSITY_NAMES[d->density] : k == 1u ? RANGE_NAMES[d->range]
           : k == 2u ? SCALE_NAMES[d->scale[voice]] : RHYTHM_NAMES[d->rhythm];
}

static int is_note(uint32_t n) {
    return n >= GT2_FIRSTNOTE && n <= GT2_LASTNOTE;
}

static int in_scale(const song_gen* g, uint32_t root, uint32_t note) {
    const uint8_t* sc = SCALES[g->scale];
    for (uint32_t i = 1; i <= sc[0]; i++) {
        if ((note + 120u - root) % 12u == sc[i]) {
            return 1;
        }
    }
    return 0;
}

/* The note of a random value h on the lane: its own for a drum lane, else a degree of the
 * scale in the range's octaves from the lane's note (1 OCT its own, 2 OCT and the next, 3 OCT
 * and the one after, BASS and the one below). */
uint32_t song_gen_note(const song_gen* g, uint32_t h) {
    uint32_t root = g->root[g->voice];
    if (g->drums) {
        return root;
    }
    const uint8_t* sc = SCALES[g->scale];
    uint32_t n = root + sc[1u + h % sc[0]];
    uint32_t octave = g->range == 2u ? (h >> 8) % 3u : g->range ? h >> 8 & 1u : 0u;
    if (g->range != 3u) {
        n += 12u * octave;
    } else if (octave && n >= 12u) {
        n -= 12u;
    }
    while (n > GT2_NOTE_MAX) {
        n -= 12u;
    }
    return n;
}

int song_gen_hit(const song_gen* g, uint32_t row) {
    uint32_t h = song_mix(g->seed + row * GEN_PHI);
    uint32_t chance = (g->drums ? GRID_DRUMS : GRID_MELODIC)[g->dens][row & 3u];
    if (g->rhythm) {
        chance = RHYTHMS[g->rhythm - 1u][row & 7u] * RHYTHM_MUL[g->dens] / 100u +
                 RHYTHM_ADD[g->dens];
    }
    return (h >> 8) % 100u < chance;
}

/* ---- the run ---- */

__attribute__((noinline)) static void feedback(screen* s, const song_view* sv) {
    const char* msg = sv->editor && sv->song ? song_edit_message(sv->editor) : "NO SONG";
    if (msg && *msg) {
        screen_toast(s, msg, SCREEN_TOAST_MS);
        s->popup_ask = 0;
    }
}

__attribute__((noinline)) static void refuse(screen* s, const char* msg) {
    screen_toast(s, msg, SCREEN_TOAST_MS);
    s->popup_ask = 0;
}

static int read_cell(const song_view* sv, uint32_t row, uint8_t cell[4]) {
    return sv->editor && song_edit_read_row(sv->editor, sv->gen.pattern, row, cell);
}

static int write_cell(screen* s, song_view* sv, uint32_t row, const uint8_t cell[4]) {
    int ok = song_edit_row(sv->editor, sv->gen.pattern, row, cell);
    sv->edit_pending = sv->editor->pending;
    if (!ok || sv->editor->status == SONG_EDIT_TIDYING) {
        feedback(s, sv);
    }
    return ok;
}

/* A note on or off at row (STEP's toggle): on, the note with the lane's instrument; off, a rest
 * or the key off the note before it still wants. A staccato lane's key off follows on the next
 * row if it is empty (taking a note away takes its key off), at the next turn. */
static void toggle(screen* s, song_view* sv, uint32_t row, uint32_t note) {
    song_gen* g = &sv->gen;
    uint8_t cell[4], other[4];
    if (!read_cell(sv, row, cell)) {
        feedback(s, sv);
        return;
    }
    int staccato = !g->legato;
    int note_on = !is_note(cell[0]);
    if (note_on) {
        cell[0] = (uint8_t)(GT2_FIRSTNOTE + note);
        cell[1] = g->instr;
    } else {
        int after_note = row && read_cell(sv, row - 1u, other) && is_note(other[0]);
        cell[0] = staccato && after_note ? GT2_KEYOFF : GT2_REST;
        cell[1] = 0u;
    }
    int has_next = row + 1u < g->rows && read_cell(sv, row + 1u, other);
    if (!write_cell(s, sv, row, cell) || !staccato || !has_next) {
        return;
    }
    if (note_on && other[0] == GT2_REST) {
        other[0] = GT2_KEYOFF;
        g->follow = 1u;
    } else if (!note_on && other[0] == GT2_KEYOFF && !other[1] && !other[2]) {
        other[0] = GT2_REST;
        g->follow = 1u;
    }
    if (g->follow) {
        g->follow_row = (uint8_t)(row + 1u);
        memcpy(g->follow_cell, other, 4u);
    }
}

/* DICE's row r: its note, or what follows one (a key off on a staccato lane), or a rest; only a
 * row that differs is written (its command and data stay). 1: written, or the run is over. */
static int dice_row(screen* s, song_view* sv, uint32_t r) {
    song_gen* g = &sv->gen;
    uint8_t cell[4];
    if (!read_cell(sv, r, cell)) {
        feedback(s, sv);
        g->run = 0u;
        return 1;
    }
    int hit = song_gen_hit(g, r);
    uint8_t want = hit ? (uint8_t)(GT2_FIRSTNOTE + song_gen_note(g, song_mix(g->seed ^ r)))
                   : r && !g->legato && song_gen_hit(g, r - 1u) ? GT2_KEYOFF
                                                                : GT2_REST;
    uint8_t instr = hit ? g->instr : 0u;
    if (cell[0] == want && cell[1] == instr) {
        return 0;
    }
    cell[0] = want;
    cell[1] = instr;
    if (!write_cell(s, sv, r, cell)) {
        g->run = 0u;
    }
    return 1;
}

/* One of MUTATE's two changes at a random row: a note is nudged to the next note of the scale
 * up or down, or goes; a rest gets a note of the scale (a ghost step). */
static void mutate_now(screen* s, song_view* sv) {
    song_gen* g = &sv->gen;
    uint8_t cell[4];
    uint32_t h = song_mix(g->seed + g->row * GEN_PHI);
    uint32_t row = h % g->rows;
    if (!read_cell(sv, row, cell)) {
        feedback(s, sv);
        g->run = 0u;
        return;
    }
    h = song_mix(h);
    if (!is_note(cell[0]) || g->drums || h >> 9 & 1u) {
        toggle(s, sv, row, song_gen_note(g, h));
        return;
    }
    uint32_t root = g->root[g->voice], n = cell[0] - GT2_FIRSTNOTE;
    int up = (int)(h >> 10 & 1u);
    for (uint32_t i = 0; i < 12u; i++) {
        n = up ? (n < GT2_NOTE_MAX ? n + 1u : n) : (n ? n - 1u : n);
        if (in_scale(g, root, n)) {
            break;
        }
    }
    cell[0] = (uint8_t)(GT2_FIRSTNOTE + n);
    (void)write_cell(s, sv, row, cell);
}

/* The sweep's row r (every SONG_GEN_SWEEP_EVERY): a FILTER CUTOFF command, low to high (up) or
 * back, where the row has no command. 1: written, or the run is over; 0: the row keeps its own. */
static int sweep_row(screen* s, song_view* sv, uint32_t r) {
    song_gen* g = &sv->gen;
    uint8_t cell[4];
    if (!read_cell(sv, r, cell)) {
        feedback(s, sv);
        g->run = 0u;
        return 1;
    }
    if (cell[2]) {
        return 0;
    }
    uint32_t last = (g->rows - 1u) / SONG_GEN_SWEEP_EVERY;
    uint32_t i = g->up ? r / SONG_GEN_SWEEP_EVERY : last - r / SONG_GEN_SWEEP_EVERY;
    cell[2] = CMD_CUTOFF;
    cell[3] = (uint8_t)(SONG_GEN_SWEEP_LOW +
                        (last ? (SONG_GEN_SWEEP_HIGH - SONG_GEN_SWEEP_LOW) * i / last : 0u));
    if (!write_cell(s, sv, r, cell)) {
        g->run = 0u;
    }
    return 1;
}

/* The next write of the run: one a turn. */
static void run_now(screen* s, song_view* sv) {
    song_gen* g = &sv->gen;
    uint32_t rows = sv->song ? gt2_pattern_rows(sv->song, g->pattern) - 1u : 0u;
    g->rows = (uint8_t)rows;
    if (!rows) {
        g->run = 0u;
    } else if (g->run == SONG_GEN_MUTATE) {
        mutate_now(s, sv);
        g->run = ++g->row < 2u ? g->run : 0u;
    } else {
        uint32_t every = g->run == SONG_GEN_SWEEP ? SONG_GEN_SWEEP_EVERY : 1u;
        while (g->run && g->row < rows) {
            uint32_t r = g->row;
            g->row = (uint8_t)(r + every);
            if (g->run == SONG_GEN_SWEEP ? sweep_row(s, sv, r) : dice_row(s, sv, r)) {
                return;
            }
        }
        g->run = 0u;
    }
}

void song_gen_start(song_view* sv, screen* s, uint32_t kind, uint32_t pattern, uint32_t voice,
                    uint32_t instr) {
    song_gen* g = &sv->gen;
    if (!sv->song || !sv->editor || pattern == 0xffu) {
        feedback(s, sv);
        return;
    }
    if (g->run) {
        refuse(s, "WAITING");
        return;
    }
    song_dice d;
    song_gen_dice(sv, &d);
    g->seed = song_mix(g->seed ^ s->now ^ s->frame);
    g->drums = (uint8_t)song_gen_drums(sv, voice);
    g->pattern = (uint8_t)pattern;
    g->voice = (uint8_t)voice;
    g->row = 0u;
    g->instr = (uint8_t)instr;
    g->legato = (uint8_t)(g->gate ? *g->gate >> voice & 1u : 0u);
    g->dens = d.density;
    g->scale = d.scale[voice];
    g->rhythm = d.rhythm;
    g->range = d.range;
    g->up = (uint8_t)(g->seed >> 9 & 1u);
    g->run = (uint8_t)kind;
    refuse(s, kind == SONG_GEN_MUTATE ? "MUTATED"
              : kind == SONG_GEN_DICE ? "NEW PATTERN"
              : g->up                 ? "SWEEP UP"
                                      : "SWEEP DOWN");
}

int song_gen_busy(const song_view* sv) {
    return sv->gen.run | sv->gen.follow;
}

void song_gen_drain(song_view* sv, screen* s) {
    song_gen* g = &sv->gen;
    if (!sv->editor) {
        return;
    }
    if (sv->edit_pending && sv->editor->sent) {
        (void)song_edit_poll(sv->editor);
        sv->edit_pending = sv->editor->pending;
    }
    if (sv->editor->pending) {
        return;
    }
    if (g->follow) {
        g->follow = 0u;
        int ok = song_edit_row(sv->editor, g->pattern, g->follow_row, g->follow_cell);
        sv->edit_pending = sv->editor->pending;
        if (!ok) {
            feedback(s, sv);
        }
        return;
    }
    if (g->run) {
        run_now(s, sv);
    }
}

void song_gen_stop(song_view* sv) {
    sv->gen.run = 0u;
    sv->gen.scl_down = sv->gen.scl_spoiled = sv->gen.arp_alone = sv->gen.lfo_alone = 0u;
}

/* ---- the chord ---- */

void song_gen_scl_alone(song_view* sv) {
    if (sv->gen.scl_down == SONG_GEN_SCL_ALONE) {
        sv->gen.scl_down = 0u;
        live_fx_select_next(sv->l);
    }
}

int song_gen_press(song_view* sv, uint32_t id, int edit, int free) {
    song_gen* g = &sv->gen;
    if (g->scl_down == SONG_GEN_SCL_ALONE) {
        if (id == PANEL_BTN_EDIT) {
            g->scl_down = SONG_GEN_SCL_CHORD; /* EDIT after SCL: the same chord */
            g->scl_spoiled = 0u;
            return 1;
        }
        song_gen_scl_alone(sv); /* another button: SCL was alone */
    }
    if (g->scl_down && id != PANEL_BTN_SCL) {
        g->scl_spoiled = 1u; /* another button: a combination of its own, which does nothing */
        return 1;
    }
    if (free && id == PANEL_BTN_SCL) {
        /* With EDIT: DICE rolls at SCL's release, unless a knob is turned first. Alone: it waits
         * for EDIT a while (a hand's two fingers rarely land together). */
        g->scl_down = edit ? SONG_GEN_SCL_CHORD : SONG_GEN_SCL_ALONE;
        g->scl_spoiled = 0u;
        return 1;
    }
    return 0;
}

int song_gen_release(song_view* sv, uint32_t held) {
    song_gen* g = &sv->gen;
    if (!g->scl_down || held >> PANEL_BTN_SCL & 1u) {
        return 0;
    }
    if (g->scl_down == SONG_GEN_SCL_ALONE) {
        song_gen_scl_alone(sv);
        return 0;
    }
    int roll = !g->scl_spoiled;
    g->scl_down = g->scl_spoiled = 0u;
    return roll;
}

int song_gen_turn(song_view* sv, screen* s, uint32_t enc, int32_t steps) {
    if (sv->gen.scl_down != SONG_GEN_SCL_CHORD || enc < PANEL_ENC_K1 || enc > PANEL_ENC_K4) {
        return 0;
    }
    /* One detent a step, stopping at the ends; a turn spoils the roll at SCL's release. */
    uint32_t k = enc - PANEL_ENC_K1;
    song_dice d;
    song_gen_dice(sv, &d);
    uint8_t* f = dice_field(&d, k, sv->voice);
    int32_t v = (int32_t)*f + steps;
    *f = (uint8_t)(v < 0 ? 0 : v > (int32_t)DICE_TOPS[k] ? (int32_t)DICE_TOPS[k] : v);
    sv->gen.scl_spoiled = 1u;
    int r = sv->song ? song_ext_set_dice((gt2_song*)sv->song, &d) : -1;
    if (r < 0) {
        refuse(s, r == -(int)GT2_ERR_SAVING ? "SAVING" : "SONG FULL");
    }
    return 1;
}

static void copy_text(char out[SCREEN_POPUP_TEXT], const char* text) {
    uint32_t n = 0u;
    while (n < SCREEN_POPUP_TEXT - 1u && text[n]) {
        n++;
    }
    memcpy(out, text, n);
    out[n] = 0;
}

int song_gen_popup(const song_view* sv, uint32_t enc, char name[SCREEN_POPUP_TEXT],
                   char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max) {
    if (sv->gen.scl_down != SONG_GEN_SCL_CHORD || enc < PANEL_ENC_K1 || enc > PANEL_ENC_K4) {
        return 0;
    }
    uint32_t k = enc - PANEL_ENC_K1;
    song_dice d;
    song_gen_dice(sv, &d);
    copy_text(name, DICE_NAMES[k]);
    copy_text(value, dice_text(&d, k, sv->voice));
    *fill = *dice_field(&d, k, sv->voice);
    *max = DICE_TOPS[k];
    return 1;
}

uint32_t song_gen_keys(song_view* sv, uint32_t down, uint32_t pressed) {
    if (sv->gen.scl_down != SONG_GEN_SCL_CHORD) {
        return 0u;
    }
    for (uint32_t k = 0; pressed >> k; k++) {
        if (!(pressed >> k & 1u)) {
            continue;
        }
        uint8_t n = live_key_note(sv->l, (uint8_t)(PANEL_BASE_NOTE + k));
        if (n != LIVE_NO_NOTE && n >= 12u && n - 12u <= GT2_NOTE_MAX) {
            sv->gen.root[sv->voice] = (uint8_t)(n - 12u); /* the jam's: key note 12 is note 0 */
        }
    }
    return down | pressed;
}

/* ---- the frame ---- */

typedef song_view_canvas canvas;

static void small(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t a) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = a;
    }
}

static void put(canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t a) {
    c->ch[row][col] = ch;
    c->attr[row][col] = a;
}

/* While EDIT + SCL is held: DICE's four settings at 2x, each beside its knob (K1-K4, what a turn
 * changes is what the frame says), and the lane's root under them; a release rolls. A drum lane's
 * range and scale are grey: it keeps its one note. */
void song_gen_frame(song_view* sv, uint32_t top) {
    canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    uint32_t v = sv->voice, bottom = top + SONG_GEN_FRAME_ROWS - 1u;
    const uint8_t frame = THEME_ATTR(th, dim);
    song_dice d;
    song_gen_dice(sv, &d);
    int drums = song_gen_drums(sv, v);
    for (uint32_t r = top; r <= bottom; r++) {
        memset(c->ch[r], ' ', GRID_COLS);
        memset(c->attr[r], THEME_ATTR(th, value), GRID_COLS);
        put(c, 0, r, FONT_BOX_V, frame);
        put(c, GRID_COLS - 1u, r, FONT_BOX_V, frame);
    }
    memset(&c->ch[top][1], FONT_BOX_H, GRID_COLS - 2u);
    memset(&c->attr[top][1], frame, GRID_COLS - 2u);
    memset(&c->ch[bottom][1], FONT_BOX_H, GRID_COLS - 2u);
    memset(&c->attr[bottom][1], frame, GRID_COLS - 2u);
    put(c, 0, top, FONT_ROUND_TL, frame);
    put(c, GRID_COLS - 1u, top, FONT_ROUND_TR, frame);
    put(c, 0, bottom, FONT_ROUND_BL, frame);
    put(c, GRID_COLS - 1u, bottom, FONT_ROUND_BR, frame);
    small(c, 2, top, " DICE - LET GO TO ROLL ", THEME_ATTR(th, hot));
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t r = top + 1u + 2u * k;
        uint8_t a = drums && (k == 1u || k == 2u) ? THEME_ATTR(th, dim)
                                                  : GRID_ATTR(th->knob_text[k], th->bg);
        char tag[3] = {'K', (char)('1' + k), 0};
        small(c, 2, r, tag, THEME_ATTR(th, hint));
        small(c, 2, r + 1u, DICE_LABELS[k], a);
        (void)grid_canvas_text_big(c, 11, r, 2, dice_text(&d, k, v), a);
    }
    uint32_t root = sv->gen.root[v];
    char nn[4] = {NOTE_NAMES[root % 12u][0], NOTE_NAMES[root % 12u][1],
                  (char)('0' + root / 12u % 10u), 0};
    small(c, 2, top + 10u, "ROOT", THEME_ATTR(th, label));
    (void)grid_canvas_text_big(c, 11, top + 9u, 2, nn, THEME_ATTR(th, value));
    sv->dirty |= ((1u << SONG_GEN_FRAME_ROWS) - 1u) << top;
}

int song_gen_look(const song_view* sv, uint8_t out[SONG_GEN_LOOK]) {
    memset(out, 0, SONG_GEN_LOOK);
    if (sv->gen.scl_down != SONG_GEN_SCL_CHORD) {
        return 0;
    }
    song_dice d;
    song_gen_dice(sv, &d);
    out[0] = 1u;
    out[1] = d.density;
    out[2] = d.scale[sv->voice];
    out[3] = d.rhythm;
    out[4] = d.range;
    out[5] = sv->gen.root[sv->voice];
    return 1;
}
