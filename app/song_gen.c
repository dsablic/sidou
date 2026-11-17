// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_gen.h"

#include <string.h>

#include "font.h"
#include "lab_song.h"
#include "song/gt2_kit.h"
#include "song/gt2_song.h"
#include "song_pages.h"
#include "song_swing.h"
#include "song_tempo.h"
#include "song_view.h"
#include "theme.h"

static const char* const DENSITY_NAMES[SONG_DICE_DENSITIES] = {"SPARSE", "LIGHT", "MEDIUM", "BUSY",
                                                               "FULL"};
static const char* const SCALE_NAMES[SONG_DICE_SCALES] = {"MIN PENT", "MAJ PENT", "MINOR",
                                                          "MAJOR",    "DORIAN",   "CHROMATIC"};
static const char* const RANGE_NAMES[SONG_DICE_RANGES] = {"1 OCT", "2 OCT", "3 OCT", "BASS"};
static const char* const RHYTHM_NAMES[SONG_DICE_RHYTHMS] = {"STRAIGHT", "OFFBEAT", "3-3-2", "FOUR",
                                                            "BROKEN"};
static const char* const DICE_LABELS[4] = {"DENS", "RANGE", "SCALE", "RHYTHM"};
static const char* const DICE_NAMES[4] = {"DENSITY", "RANGE", "SCALE", "RHYTHM"};
static const uint8_t DICE_TOPS[4] = {SONG_DICE_DENSITIES - 1u, SONG_DICE_RANGES - 1u,
                                     SONG_DICE_SCALES - 1u, SONG_DICE_RHYTHMS - 1u};
static const uint8_t ROLE_ORDER[SONG_ROLES] = {SONG_ROLE_AUTO,       SONG_ROLE_DRUMS, SONG_ROLE_BASS,
                                               SONG_ROLE_BASS_DRUMS, SONG_ROLE_ACID,
                                               SONG_ROLE_ARP,        SONG_ROLE_LEAD,  SONG_ROLE_NONE};
static const char* const ROLE_NAMES[SONG_ROLES] = {"AUTO", "DRUMS", "BASS",       "ARP",
                                                   "LEAD", "NONE",  "BASS+DRUMS", "ACID"};
static const uint8_t CHORD_TONES[SONG_GEN_CHORDS][SONG_GEN_CHORD_ROWS - 2u] = {
    {4, 7}, {3, 7}, {5, 7}, {4, 7, 10}};
static const char CHORD_NAMES[SONG_GEN_CHORDS][5] = {"MAJ", "MIN", "SUS4", "7"};
static const uint16_t DRUM_KICK[SONG_DICE_RHYTHMS] = {0x0101, 0x0101, 0x0909, 0x1111, 0x0481};
static const uint16_t DRUM_SNARE[SONG_DICE_RHYTHMS] = {0x1010, 0x1010, 0x4040, 0x0000, 0x1010};
#define CMD_WAVEPTR 0x08u

static const uint8_t SCALES[SONG_DICE_SCALES][13] = {
    {5, 0, 3, 5, 7, 10},          {5, 0, 2, 4, 7, 9},         {7, 0, 2, 3, 5, 7, 8, 10},
    {7, 0, 2, 4, 5, 7, 9, 11},    {7, 0, 2, 3, 5, 7, 9, 10},  {12, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11}};
static const uint8_t GRID_MELODIC[SONG_DICE_DENSITIES][4] = {
    {60, 0, 0, 0}, {85, 0, 25, 0}, {85, 0, 55, 0}, {90, 35, 75, 35}, {100, 100, 100, 100}};
static const uint8_t GRID_DRUMS[SONG_DICE_DENSITIES][4] = {
    {80, 0, 15, 0}, {90, 5, 30, 10}, {90, 15, 50, 25}, {95, 40, 75, 55}, {100, 100, 100, 100}};
static const uint8_t RHYTHMS[SONG_DICE_RHYTHMS - 1u][8] = {
    {10, 0, 90, 0, 10, 0, 90, 20},
    {95, 0, 0, 85, 0, 0, 85, 0},
    {100, 0, 20, 0, 100, 0, 20, 0},
    {90, 0, 30, 65, 0, 45, 75, 20}};
static const uint8_t RHYTHM_MUL[SONG_DICE_DENSITIES] = {50, 75, 100, 100, 100};
static const uint8_t RHYTHM_ADD[SONG_DICE_DENSITIES] = {0, 0, 0, 20, 100};
#define GEN_PHI 0x9e3779b9u
#define ACID_SLIDE_PERCENT 30u
#define ACID_ACCENT_PERCENT 35u
#define ACID_LOCK_ON 1u
#define CMD_CUTOFF 0x0Cu
#define CMD_FUNKTEMPO 0x0Eu
#define CMD_TEMPO 0x0Fu
#define TEMPO_PER_VOICE 0x80u
#define SWING_NEW_SPEED 6u
#define SWING_PAIRS 2u
static const char* const SWING_NAMES[SONG_SWING_LEVELS] = {"STRAIGHT", "LIGHT", "SHUFFLE", "HARD"};
static const char* const SWING_REFUSED[SONG_SWING_LEVELS] = {"", "NO LIGHT", "NO SHUFFLE", "NO HARD"};

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
    return song_gen_role(sv, voice) == SONG_ROLE_DRUMS;
}

uint32_t song_gen_role(const song_view* sv, uint32_t voice) {
    song_scene scene;
    return sv->song && song_ext_get_scene(sv->song, &scene) && voice < scene.count &&
                   scene.role[voice] < SONG_ROLES
               ? scene.role[voice]
               : SONG_ROLE_AUTO;
}

static uint32_t chord_len(uint32_t c) {
    return c == SONG_GEN_7 ? 4u : 3u;
}

uint32_t song_gen_chord_rows(uint32_t c, uint32_t first, uint8_t left[SONG_GEN_CHORD_ROWS],
                             uint8_t right[SONG_GEN_CHORD_ROWS]) {
    uint32_t n = chord_len(c);
    memset(left, 0, SONG_GEN_CHORD_ROWS);
    right[0] = 0u;
    memcpy(right + 1, CHORD_TONES[c], n - 1u);
    left[n] = 0xffu;
    right[n] = (uint8_t)first;
    return n + 1u;
}

static int chord_is(const gt2_song* s, uint32_t ptr, uint32_t c) {
    uint8_t left[SONG_GEN_CHORD_ROWS], right[SONG_GEN_CHORD_ROWS];
    uint32_t n = song_gen_chord_rows(c, ptr, left, right);
    if (ptr < 1u || ptr - 1u + n > GT2_TABLE_ROWS) {
        return 0;
    }
    return !memcmp(&s->ltable[GT2_WTBL][ptr - 1u], left, n) &&
           !memcmp(&s->rtable[GT2_WTBL][ptr - 1u], right, n);
}

uint32_t song_gen_chord_at(const gt2_song* s, uint32_t ptr) {
    uint32_t c = 0u;
    while (c < SONG_GEN_CHORDS && !chord_is(s, ptr, c)) {
        c++;
    }
    return c;
}

uint32_t song_gen_chord_find(const gt2_song* s, uint32_t c) {
    for (uint32_t p = 1u; p <= GT2_TABLE_ROWS; p++) {
        if (chord_is(s, p, c)) {
            return p;
        }
    }
    return 0u;
}

const char* song_gen_chord_name(uint32_t c) {
    return CHORD_NAMES[c];
}

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

static uint32_t speed_row(const gt2_song* song, uint32_t left, uint32_t right) {
    for (uint32_t r = 0; r < GT2_TABLE_ROWS; r++) {
        if (song->ltable[GT2_STBL][r] == left && song->rtable[GT2_STBL][r] == right) {
            return r;
        }
    }
    return GT2_TABLE_ROWS;
}

static int read_first_row(const song_view* sv, uint32_t pattern, uint8_t cell[4]) {
    return sv->editor && song_edit_read_row(sv->editor, pattern, 0u, cell);
}

static int read_cell(const song_view* sv, uint32_t row, uint8_t cell[4]) {
    return sv->editor && song_edit_read_row(sv->editor, sv->gen.pattern, row, cell);
}

static int write_cell_at(screen* s, song_view* sv, uint32_t pattern, uint32_t row,
                         const uint8_t cell[4]) {
    int ok = song_edit_row(sv->editor, pattern, row, cell);
    sv->edit_pending = sv->editor->pending;
    if (!ok || sv->editor->status == SONG_EDIT_TIDYING) {
        feedback(s, sv);
    }
    return ok;
}

static int write_cell(screen* s, song_view* sv, uint32_t row, const uint8_t cell[4]) {
    return write_cell_at(s, sv, sv->gen.pattern, row, cell);
}

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

static uint32_t hit_kind(const song_gen* g, uint32_t r) {
    if (g->role == SONG_ROLE_BASS_DRUMS) {
        uint32_t bit = 1u << (r & 15u);
        if (DRUM_KICK[g->rhythm] & bit) {
            return 2u;
        }
        if (DRUM_SNARE[g->rhythm] & bit) {
            return 3u;
        }
    }
    return (uint32_t)song_gen_hit(g, r);
}

static uint32_t chord_pick(const song_gen* g, uint32_t h, uint32_t* note) {
    uint32_t root = g->root[g->voice], fits = 0u, n = 0u, count = 0u;
    for (uint32_t k = 0; k < 12u && !fits; k++) {
        n = song_gen_note(g, h + k);
        for (uint32_t c = 0; c < SONG_GEN_CHORDS; c++) {
            uint32_t ok = 1u;
            for (uint32_t i = 0; i + 1u < chord_len(c); i++) {
                ok &= (uint32_t)in_scale(g, root, n + CHORD_TONES[c][i]);
            }
            fits |= ok << c;
            count += ok;
        }
    }
    uint32_t pick = count ? (h >> 16) % count : 0u, c = 0u;
    for (; c < SONG_GEN_CHORDS - 1u; c++) {
        if (fits >> c & 1u && !pick--) {
            break;
        }
    }
    while (n + CHORD_TONES[c][chord_len(c) - 2u] > GT2_NOTE_MAX) {
        n -= 12u;
    }
    *note = n;
    return c;
}

static int acid_lock(screen* s, song_view* sv, uint32_t r, uint32_t param, int on) {
    gt2_song* song = (gt2_song*)sv->song;
    uint32_t value;
    int was = song_lock_get(song, sv->gen.pattern, r, param, &value);
    if (on == was) {
        return 0;
    }
    int ok = on ? song_lock_put(song, sv->gen.pattern, r, param, ACID_LOCK_ON)
                : song_lock_remove(song, sv->gen.pattern, r, param);
    if (ok < 0) {
        refuse(s, song_view_lock_refusal(ok));
        sv->gen.run = 0u;
    }
    return 1;
}

static int acid_locks(screen* s, song_view* sv, uint32_t r, uint32_t kind, uint32_t h) {
    const song_gen* g = &sv->gen;
    uint32_t roll = song_mix(h ^ GEN_PHI);
    int next = r + 1u < g->rows && hit_kind(g, r + 1u);
    int slide = kind && next && roll % 100u < ACID_SLIDE_PERCENT;
    int accent = kind && (roll >> 8) % 100u < ACID_ACCENT_PERCENT;
    int changed = acid_lock(s, sv, r, SONG_LOCK_SLIDE, slide);
    if (sv->gen.run) {
        changed |= acid_lock(s, sv, r, SONG_LOCK_ACCENT, accent);
    }
    return changed;
}

static int dice_row(screen* s, song_view* sv, uint32_t r) {
    song_gen* g = &sv->gen;
    uint8_t cell[4], was[4];
    if (!read_cell(sv, r, cell)) {
        feedback(s, sv);
        g->run = 0u;
        return 1;
    }
    memcpy(was, cell, 4u);
    uint32_t kind = hit_kind(g, r), h = song_mix(g->seed ^ r), n = song_gen_note(g, h);
    int arp = g->role == SONG_ROLE_ARP;
    if (kind > 1u) {
        n = g->root[g->voice];
    } else if (kind && arp) {
        uint32_t c = chord_pick(g, h, &n);
        cell[2] = CMD_WAVEPTR;
        cell[3] = g->chord[c];
    } else if (!kind && arp && cell[2] == CMD_WAVEPTR) {
        cell[2] = cell[3] = 0u;
    }
    cell[0] = kind ? (uint8_t)(GT2_FIRSTNOTE + n)
              : r && !g->legato && hit_kind(g, r - 1u) ? GT2_KEYOFF
                                                       : GT2_REST;
    cell[1] = kind == 1u ? g->instr : kind == 2u ? g->kick : kind == 3u ? g->snare : 0u;
    int locked = g->role == SONG_ROLE_ACID && acid_locks(s, sv, r, kind, h);
    if (!g->run) {
        return 1;
    }
    if (!memcmp(cell, was, 4u)) {
        return locked;
    }
    if (!write_cell(s, sv, r, cell)) {
        g->run = 0u;
    }
    return 1;
}

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

void song_gen_bjorklund(uint32_t hits, uint32_t steps, uint8_t out[SONG_GEN_EUCLID_ROWS]) {
    uint8_t a[SONG_GEN_EUCLID_ROWS] = {1u}, b[SONG_GEN_EUCLID_ROWS] = {0u};
    uint32_t la = 1u, lb = 1u, na = hits, nb = steps - hits;
    while (nb > 1u && na) {
        uint8_t was[SONG_GEN_EUCLID_ROWS];
        memcpy(was, a, la);
        memcpy(a + la, b, lb);
        uint32_t lwas = la;
        la += lb;
        if (na <= nb) {
            nb -= na;
        } else {
            memcpy(b, was, lwas);
            lb = lwas;
            uint32_t pairs = nb;
            nb = na - nb;
            na = pairs;
        }
    }
    uint32_t n = 0u;
    for (uint32_t k = 0; k < na; k++, n += la) {
        memcpy(out + n, a, la);
    }
    for (uint32_t k = 0; k < nb; k++, n += lb) {
        memcpy(out + n, b, lb);
    }
}

static int euclid_hit(const song_gen* g, uint32_t row) {
    return g->euclid_mask[row / 8u] >> (row % 8u) & 1u;
}

static int euclid_row(screen* s, song_view* sv, uint32_t r) {
    uint8_t cell[4];
    if (!read_cell(sv, r, cell)) {
        feedback(s, sv);
        sv->gen.run = 0u;
        return 1;
    }
    if (euclid_hit(&sv->gen, r) == is_note(cell[0])) {
        return 0;
    }
    toggle(s, sv, r, sv->gen.root[sv->gen.voice]);
    return 1;
}

static void run_now(screen* s, song_view* sv) {
    song_gen* g = &sv->gen;
    uint32_t rows = sv->song ? gt2_pattern_rows(sv->song, g->pattern) - 1u : 0u;
    g->rows = (uint8_t)rows;
    if (!rows) {
        g->run = 0u;
    } else if (g->run == SONG_GEN_MUTATE) {
        mutate_now(s, sv);
        g->run = ++g->row < 2u ? g->run : 0u;
    } else if (g->wait) {
        const song_pages* p = g->pages;
        if (p->pending || song_pages_batch_busy(p)) {
            return;
        }
        g->wait = 0u;
        for (uint32_t c = 0; c < SONG_GEN_CHORDS; c++) {
            g->chord[c] = (uint8_t)song_gen_chord_find(sv->song, c);
            if (!g->chord[c]) {
                g->run = 0u;
                refuse(s, *song_pages_message(p) ? song_pages_message(p) : "TABLE FULL");
            }
        }
    } else {
        uint32_t every = g->run == SONG_GEN_SWEEP ? SONG_GEN_SWEEP_EVERY : 1u;
        while (g->run && g->row < rows) {
            uint32_t r = g->row;
            g->row = (uint8_t)(r + every);
            if (g->run == SONG_GEN_SWEEP    ? sweep_row(s, sv, r)
                : g->run == SONG_GEN_EUCLID ? euclid_row(s, sv, r)
                                            : dice_row(s, sv, r)) {
                return;
            }
        }
        g->run = 0u;
    }
}

static int chords_ready(song_view* sv, screen* s) {
    song_gen* g = &sv->gen;
    song_pages* p = g->pages;
    const gt2_song* song = sv->song;
    uint32_t missing = 0u;
    for (uint32_t c = 0; c < SONG_GEN_CHORDS; c++) {
        g->chord[c] = (uint8_t)song_gen_chord_find(song, c);
        missing |= (uint32_t)!g->chord[c] << c;
    }
    if (!missing) {
        return 1;
    }
    if (!p || p->pending || !song_pages_batch_begin(p)) {
        refuse(s, "WAITING");
        return -1;
    }
    const char* why = NULL;
    uint32_t at = lab_song_table_end(song, GT2_WTBL);
    for (uint32_t c = 0; !why && c < SONG_GEN_CHORDS; c++) {
        uint8_t left[SONG_GEN_CHORD_ROWS], right[SONG_GEN_CHORD_ROWS];
        if (missing >> c & 1u) {
            uint32_t n = song_gen_chord_rows(c, at + 1u, left, right);
            why = song_pages_batch_rows(p, GT2_WTBL, at, n, 0u, left, right);
            at += n;
        }
    }
    if (why) {
        p->batch_runs = p->batch_end = p->batch_used = 0u;
    } else if (!song_pages_batch_go(p)) {
        why = song_pages_message(p);
    }
    if (why) {
        refuse(s, why);
        return -1;
    }
    return 0;
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
    g->role = (uint8_t)song_gen_role(sv, voice);
    g->drums = g->role == SONG_ROLE_DRUMS;
    g->wait = 0u;
    if (kind == SONG_GEN_DICE && g->role == SONG_ROLE_BASS_DRUMS) {
        g->kick = (uint8_t)gt2_kit_find(sv->song, GT2_KIT_KICK);
        g->snare = (uint8_t)gt2_kit_find(sv->song, GT2_KIT_SNARE);
        if (!g->kick || !g->snare) {
            refuse(s, "NO DRUM KIT");
            return;
        }
    }
    if (kind == SONG_GEN_DICE && g->role == SONG_ROLE_ARP) {
        int ready = chords_ready(sv, s);
        if (ready < 0) {
            return;
        }
        g->wait = (uint8_t)!ready;
    }
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
              : kind == SONG_GEN_DICE ? (g->role == SONG_ROLE_ARP ? "NEW CHORDS" : "NEW PATTERN")
              : g->up                 ? "SWEEP UP"
                                      : "SWEEP DOWN");
}

void song_gen_euclid(song_view* sv, screen* s, uint32_t knob, int32_t turn, uint32_t pattern,
                     uint32_t rows, uint32_t instr) {
    song_gen* g = &sv->gen;
    uint32_t v = sv->voice;
    if (!sv->song || !sv->editor || pattern == 0xffu || !rows || rows > SONG_GEN_EUCLID_ROWS) {
        feedback(s, sv);
        return;
    }
    if (g->run && g->run != SONG_GEN_EUCLID) {
        refuse(s, "WAITING");
        return;
    }
    int32_t hits = g->euclid_hits[v], rotate = g->euclid_rotate[v];
    if (knob == SONG_GEN_EUCLID_HITS) {
        hits += turn;
        hits = hits < 0 ? 0 : hits > (int32_t)rows ? (int32_t)rows : hits;
    } else {
        rotate = (int32_t)(((uint32_t)rotate + rows + (uint32_t)(turn % (int32_t)rows)) % rows);
    }
    g->euclid_hits[v] = (uint8_t)hits;
    g->euclid_rotate[v] = (uint8_t)(rotate % (int32_t)rows);
    uint8_t steps[SONG_GEN_EUCLID_ROWS];
    song_gen_bjorklund((uint32_t)hits, rows, steps);
    memset(g->euclid_mask, 0, sizeof g->euclid_mask);
    for (uint32_t r = 0; r < rows; r++) {
        uint32_t at = (r + rows - g->euclid_rotate[v]) % rows;
        g->euclid_mask[r / 8u] |= (uint8_t)(steps[at] << (r % 8u));
    }
    g->pattern = (uint8_t)pattern;
    g->voice = (uint8_t)v;
    g->instr = (uint8_t)instr;
    g->legato = (uint8_t)(g->gate ? *g->gate >> v & 1u : 0u);
    g->row = 0u;
    g->run = SONG_GEN_EUCLID;
}

int song_gen_busy(const song_view* sv) {
    const song_gen* g = &sv->gen;
    return g->run | g->follow | g->swing_due | g->swing_wait | g->swing_writing | g->swing_poke;
}

static void swing_apply(song_view* sv, screen* s);
static void swing_start(song_view* sv, screen* s);
static void swing_run(screen* s, song_view* sv);
static void swing_live(song_view* sv);

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
    if (g->swing_wait) {
        if (g->pages->pending || song_pages_batch_busy(g->pages)) {
            return;
        }
        g->swing_wait = 0u;
        swing_start(sv, s);
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
    if (g->swing_due) {
        g->swing_due = 0u;
        swing_apply(sv, s);
        return;
    }
    if (g->run) {
        run_now(s, sv);
    }
    if (g->swing_writing && !sv->editor->pending) {
        swing_run(s, sv);
    }
    if (g->swing_poke) {
        swing_live(sv);
    }
}

void song_gen_stop(song_view* sv) {
    sv->gen.run = 0u;
    sv->gen.scl_down = sv->gen.scl_spoiled = 0u;
    sv->gen.lone.waiting = SCREEN_LONE_NONE;
}

uint32_t song_gen_lone_chord(song_view* sv, const screen_event* e) {
    uint32_t chord = screen_lone_chord(&sv->gen.lone, e);
    if (chord == PANEL_BTN_SCL) {
        sv->gen.scl_down = 1u;
        sv->gen.scl_spoiled = 0u;
    }
    return chord;
}

int song_gen_press(song_view* sv, uint32_t id, int edit, int free) {
    song_gen* g = &sv->gen;
    if (g->scl_down && id != PANEL_BTN_SCL) {
        g->scl_spoiled = 1u;
        return 1;
    }
    if (free && id == PANEL_BTN_SCL) {
        g->scl_down = (uint8_t)edit;
        g->scl_spoiled = 0u;
        if (!edit) {
            screen_lone_wait(&g->lone, id, SCREEN_LONE_HELD_BACK);
        }
        return 1;
    }
    return 0;
}

int song_gen_release(song_view* sv, uint32_t held) {
    song_gen* g = &sv->gen;
    if (!g->scl_down || held >> PANEL_BTN_SCL & 1u) {
        return 0;
    }
    int roll = !g->scl_spoiled;
    g->scl_down = g->scl_spoiled = 0u;
    return roll;
}

__attribute__((noinline)) static void ext_refused(screen* s, int r) {
    refuse(s, r == -(int)GT2_ERR_SAVING ? "SAVING" : r == -(int)GT2_ERR_PENDING ? "WAITING"
                                                                                : "SONG FULL");
}

static uint32_t role_place(const song_view* sv) {
    uint32_t role = song_gen_role(sv, sv->voice), k = 0u;
    while (ROLE_ORDER[k] != role) {
        k++;
    }
    return k;
}

static void role_turn(song_view* sv, screen* s, int32_t steps) {
    song_scene scene;
    if (!sv->song) {
        return;
    }
    (void)song_ext_get_scene(sv->song, &scene);
    if (scene.count < SONG_GEN_LANES) {
        scene.count = SONG_GEN_LANES;
    }
    int32_t k = (int32_t)role_place(sv) + steps;
    k = k < 0 ? 0 : k >= (int32_t)SONG_ROLES ? (int32_t)SONG_ROLES - 1 : k;
    scene.role[sv->voice] = ROLE_ORDER[k];
    int r = song_ext_set_scene((gt2_song*)sv->song, &scene);
    if (r < 0) {
        ext_refused(s, r);
    }
}

static uint32_t swing_subtune(const song_view* sv) {
    return song_view_playing(sv) ? sv->st.subtune : sv->subtune;
}

static uint32_t swing_pattern(const song_view* sv) {
    const gt2_song* song = sv->song;
    uint32_t sub = swing_subtune(sv);
    const uint8_t* o = song ? gt2_order_data(song, sub, 0u) : NULL;
    uint32_t n = song ? gt2_order_entries(song, sub, 0u) : 0u;
    for (uint32_t i = 0; o && i < n; i++) {
        if (o[i] < GT2_REPEAT) {
            return o[i];
        }
    }
    return 0xffu;
}

static int swing_sum(const gt2_song* song, const uint8_t cell[4], uint32_t* sum, uint32_t* late) {
    uint32_t data = cell[3];
    if (!cell[2]) {
        *sum = 2u * SWING_NEW_SPEED;
        *late = SWING_NEW_SPEED;
    } else if (cell[2] == CMD_FUNKTEMPO && data) {
        uint32_t left = song->ltable[GT2_STBL][data - 1u], right = song->rtable[GT2_STBL][data - 1u];
        *sum = left + right;
        *late = left > right ? left : right;
    } else if (cell[2] == CMD_TEMPO && data >= SONG_SWING_SPEED_MIN && data < TEMPO_PER_VOICE) {
        *sum = 2u * data;
        *late = data;
    } else {
        return 0;
    }
    return 1;
}

static uint32_t swing_level(uint32_t sum, uint32_t late) {
    uint32_t level = SONG_SWING_STRAIGHT;
    for (uint32_t k = SONG_SWING_LIGHT; k < SONG_SWING_LEVELS; k++) {
        if (song_swing_gives(sum, k) && song_swing_long(sum, k) <= late) {
            level = k;
        }
    }
    return level;
}

uint32_t song_gen_swing(const song_view* sv) {
    uint32_t p = swing_pattern(sv), sum, late;
    const uint8_t* cell = p != 0xffu ? gt2_pattern_data(sv->song, p) : NULL;
    return cell && swing_sum(sv->song, cell, &sum, &late) ? swing_level(sum, late)
                                                          : SONG_SWING_STRAIGHT;
}

static const char* swing_first(const song_view* sv, uint32_t* pattern, uint8_t cell[4],
                               uint32_t* sum, uint32_t* late) {
    *pattern = swing_pattern(sv);
    if (*pattern == 0xffu || !read_first_row(sv, *pattern, cell)) {
        return sv->editor && sv->song ? song_edit_message(sv->editor) : "NO SONG";
    }
    return swing_sum(sv->song, cell, sum, late) ? NULL : "ROW HAS A COMMAND";
}

uint32_t song_gen_row_frames(const song_view* sv) {
    uint32_t p, sum = 2u * SWING_NEW_SPEED, late = SWING_NEW_SPEED;
    uint8_t cell[4];
    (void)swing_first(sv, &p, cell, &sum, &late);
    return sum - late < late ? sum - late : late;
}

static int swing_straight(const song_gen* g) {
    return g->swing_given == SONG_SWING_STRAIGHT;
}

static int swing_needs(const song_gen* g, uint32_t k) {
    return k ? g->swing_eighths
             : !swing_straight(g) || g->swing_short != SWING_NEW_SPEED ||
                   g->swing_long != SWING_NEW_SPEED;
}

static void swing_pair(const song_gen* g, uint32_t k, uint8_t pair[2]) {
    int long_first = !k && !swing_straight(g);
    pair[0] = long_first ? g->swing_long : g->swing_short;
    pair[1] = long_first ? g->swing_short : g->swing_long;
}

static uint32_t swing_row(const song_view* sv, uint32_t k) {
    uint8_t pair[2];
    swing_pair(&sv->gen, k, pair);
    return speed_row(sv->song, pair[0], pair[1]);
}

static void swing_apply(song_view* sv, screen* s) {
    song_gen* g = &sv->gen;
    uint32_t p, sum = 0u, late = 0u;
    uint8_t cell[4], left[SWING_PAIRS], right[SWING_PAIRS], pair[2];
    if (swing_first(sv, &p, cell, &sum, &late)) {
        return;
    }
    g->swing_long = (uint8_t)song_swing_long(sum, g->swing_given);
    g->swing_short = (uint8_t)(sum - g->swing_long);
    g->swing_sub = (uint8_t)swing_subtune(sv);
    g->swing_eighths = (uint8_t)(g->swing_given != SONG_SWING_STRAIGHT &&
                                 song_swing_eighths(sv->song, g->swing_sub));
    g->swing_first = (uint8_t)p;
    g->swing_sum = (uint16_t)sum;
    uint32_t order_pos = 0u, row = 0u;
    if (song_view_playing(sv)) {
        song_view_position(sv, 0u, &order_pos, &row);
    }
    g->swing_from = (uint8_t)order_pos;
    g->swing_from_row = (uint8_t)row;
    uint32_t n = 0u;
    for (uint32_t k = 0; k < SWING_PAIRS; k++) {
        if (swing_needs(g, k) && swing_row(sv, k) == GT2_TABLE_ROWS) {
            swing_pair(g, k, pair);
            left[n] = pair[0];
            right[n++] = pair[1];
        }
    }
    if (!n) {
        swing_start(sv, s);
        return;
    }
    song_pages* pg = g->pages;
    const char* why = !pg || pg->pending || !song_pages_batch_begin(pg)
                          ? "WAITING"
                          : song_pages_batch_rows(pg, GT2_STBL, lab_song_table_end(sv->song, GT2_STBL),
                                                  n, 0u, left, right);
    if (!why && !song_pages_batch_go(pg)) {
        why = song_pages_message(pg);
    }
    if (why) {
        refuse(s, why);
        return;
    }
    g->swing_wait = 1u;
}

static void swing_start(song_view* sv, screen* s) {
    song_gen* g = &sv->gen;
    uint8_t data[SWING_PAIRS];
    for (uint32_t k = 0; k < SWING_PAIRS; k++) {
        uint32_t r = swing_needs(g, k) ? swing_row(sv, k) : GT2_TABLE_ROWS;
        if (r == GT2_TABLE_ROWS && swing_needs(g, k)) {
            refuse(s, g->pages && *song_pages_message(g->pages) ? song_pages_message(g->pages)
                                                                : "TABLE FULL");
            return;
        }
        data[k] = swing_needs(g, k) ? (uint8_t)(r + 1u) : 0u;
    }
    int straight = swing_straight(g);
    g->swing_first_data = data[0];
    g->swing_data[SONG_SWING_NONE] = 0u;
    g->swing_data[SONG_SWING_LONG_FIRST] = straight ? 0u : data[0];
    g->swing_data[SONG_SWING_SHORT_FIRST] = data[1];
    g->swing_poke = (uint8_t)straight;
    g->swing_step = 0u;
    g->swing_row = g->swing_from_row;
    g->swing_writing = 1u;
}

static int swing_cell(const song_view* sv, uint32_t r, const uint8_t cell[4], uint8_t to[4]) {
    const song_gen* g = &sv->gen;
    uint32_t data;
    if (g->swing_pattern == g->swing_first && r == 0u) {
        data = g->swing_first_data;
    } else if (song_swing_owns(sv->song, cell, g->swing_sum)) {
        data = g->swing_data[song_swing_pair_at(r, g->swing_eighths)];
    } else {
        return 0;
    }
    memcpy(to, cell, 4u);
    to[2] = data ? CMD_FUNKTEMPO : 0u;
    to[3] = (uint8_t)data;
    return (data || cell[2]) && memcmp(to, cell, 4u);
}

static uint32_t swing_rows(const song_view* sv, const uint8_t* order, uint32_t entries) {
    const song_gen* g = &sv->gen;
    uint32_t at = (g->swing_from + g->swing_step) % entries;
    uint32_t p = order[at];
    if (p >= GT2_MAX_PATTERNS || !gt2_pattern_data(sv->song, p)) {
        return 0u;
    }
    uint32_t rows = gt2_pattern_rows(sv->song, p) - 1u;
    return g->swing_step == entries && g->swing_from_row < rows ? g->swing_from_row : rows;
}

static void swing_run(screen* s, song_view* sv) {
    song_gen* g = &sv->gen;
    const uint8_t* order = gt2_order_data(sv->song, g->swing_sub, 0u);
    uint32_t entries = order ? gt2_order_entries(sv->song, g->swing_sub, 0u) : 0u;
    for (; entries && g->swing_step <= entries; g->swing_step++, g->swing_row = 0u) {
        uint32_t rows = swing_rows(sv, order, entries);
        g->swing_pattern = order[(g->swing_from + g->swing_step) % entries];
        while (g->swing_row < rows) {
            uint32_t r = g->swing_row;
            uint8_t cell[4], to[4];
            g->swing_row = (uint8_t)(r + 1u);
            if (!song_edit_read_row(sv->editor, g->swing_pattern, r, cell)) {
                feedback(s, sv);
                g->swing_writing = 0u;
                return;
            }
            if (swing_cell(sv, r, cell, to)) {
                g->swing_writing = (uint8_t)write_cell_at(s, sv, g->swing_pattern, r, to);
                return;
            }
        }
    }
    g->swing_writing = 0u;
    g->swing_poke = (uint8_t)swing_straight(g);
}

static void swing_live(song_view* sv) {
    const song_transport* t = sv->t;
    song_gen* g = &sv->gen;
    if (!song_view_playing(sv) || !t || !t->funktempo ||
        t->funktempo(t->ctx, g->swing_short, g->swing_long)) {
        g->swing_poke = 0u;
    }
}

void song_gen_swing_turn(song_view* sv, screen* s, int32_t steps) {
    song_gen* g = &sv->gen;
    uint32_t p, sum = 0u, late = 0u;
    uint8_t cell[4];
    const char* why = swing_first(sv, &p, cell, &sum, &late);
    if (why) {
        refuse(s, why);
        return;
    }
    int32_t want = (int32_t)(g->swing_turned ? g->swing : swing_level(sum, late)) + steps;
    want = want < 0 ? 0 : want >= (int32_t)SONG_SWING_LEVELS ? (int32_t)SONG_SWING_LEVELS - 1 : want;
    g->swing = (uint8_t)want;
    g->swing_turned = 1u;
    g->swing_refused = (uint8_t)!song_swing_gives(sum, (uint32_t)want);
    if (g->swing_refused) {
        return;
    }
    g->swing_given = (uint8_t)want;
    g->swing_due = 1u;
}

int song_gen_turn(song_view* sv, screen* s, uint32_t enc, int32_t steps) {
    if (!sv->gen.scl_down) {
        return 0;
    }
    if (enc == SONG_GEN_ENC_ROLE) {
        role_turn(sv, s, steps);
        sv->gen.scl_spoiled = 1u;
        return 1;
    }
    if (enc < PANEL_ENC_K1 || enc > PANEL_ENC_K4) {
        return 0;
    }
    uint32_t k = enc - PANEL_ENC_K1;
    song_dice d;
    song_gen_dice(sv, &d);
    uint8_t* f = dice_field(&d, k, sv->voice);
    int32_t v = (int32_t)*f + steps;
    *f = (uint8_t)(v < 0 ? 0 : v > (int32_t)DICE_TOPS[k] ? (int32_t)DICE_TOPS[k] : v);
    sv->gen.scl_spoiled = 1u;
    int r = sv->song ? song_ext_set_dice((gt2_song*)sv->song, &d) : -1;
    if (r < 0) {
        ext_refused(s, r);
    }
    return 1;
}

uint32_t song_gen_swing_popup(const song_view* sv, char value[SCREEN_POPUP_TEXT]) {
    const song_gen* g = &sv->gen;
    uint32_t level = g->swing_turned ? g->swing : song_gen_swing(sv);
    screen_popup_copy(value, g->swing_turned && g->swing_refused ? SWING_REFUSED[level]
                                                                 : SWING_NAMES[level]);
    return level;
}

int song_gen_popup(const song_view* sv, uint32_t enc, char name[SCREEN_POPUP_TEXT],
                   char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max) {
    if (!sv->gen.scl_down) {
        return 0;
    }
    if (enc == SONG_GEN_ENC_ROLE) {
        screen_popup_copy(name, "ROLE");
        screen_popup_copy(value, ROLE_NAMES[song_gen_role(sv, sv->voice)]);
        *fill = role_place(sv);
        *max = SONG_ROLES - 1u;
        return 1;
    }
    if (enc < PANEL_ENC_K1 || enc > PANEL_ENC_K4) {
        return 0;
    }
    uint32_t k = enc - PANEL_ENC_K1;
    song_dice d;
    song_gen_dice(sv, &d);
    screen_popup_copy(name, DICE_NAMES[k]);
    screen_popup_copy(value, dice_text(&d, k, sv->voice));
    *fill = *dice_field(&d, k, sv->voice);
    *max = DICE_TOPS[k];
    return 1;
}

uint32_t song_gen_keys(song_view* sv, uint32_t down, uint32_t pressed) {
    if (!sv->gen.scl_down) {
        return 0u;
    }
    for (uint32_t k = 0; pressed >> k; k++) {
        if (!(pressed >> k & 1u)) {
            continue;
        }
        uint8_t n = live_key_note(sv->l, (uint8_t)(PANEL_BASE_NOTE + k));
        if (n != LIVE_NO_NOTE && n >= 12u && n - 12u <= GT2_NOTE_MAX) {
            sv->gen.root[sv->voice] = (uint8_t)(n - 12u);
        }
    }
    return down | pressed;
}

typedef song_view_canvas canvas;

static void put(canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t a) {
    c->ch[row][col] = ch;
    c->attr[row][col] = a;
}

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
    grid_canvas_text(c, 2, top, " DICE - LET GO TO ROLL ", THEME_ATTR(th, hot));
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t r = top + 1u + 2u * k;
        uint8_t a = drums && (k == 1u || k == 2u) ? THEME_ATTR(th, dim)
                                                  : GRID_ATTR(th->knob_text[k], th->bg);
        char tag[3] = {'K', (char)('1' + k), 0};
        grid_canvas_text(c, 2, r, tag, THEME_ATTR(th, hint));
        grid_canvas_text(c, 2, r + 1u, DICE_LABELS[k], a);
        (void)grid_canvas_text_big(c, 11, r, 2, dice_text(&d, k, v), a);
    }
    uint32_t root = sv->gen.root[v];
    char nn[4];
    screen_note_text(root, nn);
    grid_canvas_text(c, 2, top + 10u, "ROOT", THEME_ATTR(th, label));
    (void)grid_canvas_text_big(c, 11, top + 9u, 2, nn, THEME_ATTR(th, value));
    grid_canvas_text(c, 19, top + 9u, "PRE", THEME_ATTR(th, hint));
    grid_canvas_text(c, 23, top + 9u, "ROLE", THEME_ATTR(th, label));
    grid_canvas_text(c, 19, top + 10u, ROLE_NAMES[song_gen_role(sv, v)], THEME_ATTR(th, value));
    sv->dirty |= ((1u << SONG_GEN_FRAME_ROWS) - 1u) << top;
}

int song_gen_look(const song_view* sv, uint8_t out[SONG_GEN_LOOK]) {
    memset(out, 0, SONG_GEN_LOOK);
    if (!sv->gen.scl_down) {
        return 0;
    }
    song_dice d;
    song_gen_dice(sv, &d);
    out[0] = (uint8_t)(1u + song_gen_role(sv, sv->voice));
    out[1] = d.density;
    out[2] = d.scale[sv->voice];
    out[3] = d.rhythm;
    out[4] = d.range;
    out[5] = sv->gen.root[sv->voice];
    return 1;
}
