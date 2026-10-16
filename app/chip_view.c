// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_view.h"

#include <stddef.h>

#include "font.h"
#include "fx/fx_slots.h"
#include "panel.h"
#include "sid/sid_character.h"
#include "views.h"

/* The page's colours: light blue on blue as the status screen, the focus white, what is off or
 * not there yet grey, and the A tab in reverse video. */
#define TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
#define FOCUS GRID_ATTR(GRID_WHITE, GRID_BLUE)
#define DIM GRID_ATTR(GRID_GREY, GRID_BLUE)
#define TAB GRID_ATTR(GRID_BLUE, GRID_LIGHT_BLUE)
/* Inside the frames: names from LABEL_COL, the character's bars from BAR_COL, values ending at
 * VALUE_END; a slot's second column of controls SLOT_HALF further on. */
#define LABEL_COL 4u
#define BAR_COL 10u
#define BAR_CELLS 12u
#define VALUE_END 26u
#define SLOT_HALF 12u
/* The tabs on row 2: " A " and "B M2". */
#define TITLE_ROW 2u
#define TAB_A_COL 20u
#define TAB_B_COL 24u
/* The character's frame: four rows of controls; CHIP's, two of the profile and model. */
#define CHIP_H 4u
#define CHARACTER_H 6u

_Static_assert(CHIP_VIEW_CHIP_ROW + CHIP_H == CHIP_VIEW_CHARACTER_ROW &&
                   CHIP_VIEW_CHARACTER_ROW + CHARACTER_H == CHIP_VIEW_SLOT_ROW &&
                   CHIP_VIEW_SLOT_ROW + LIVE_SLOTS * CHIP_VIEW_SLOT_ROWS == CHIP_VIEW_HELP_ROW,
               "the frames follow each other down to the help line");
_Static_assert(CHIP_VIEW_HELP_ROW < SCREEN_TOAST_ROW, "the help line is above the toast's");
_Static_assert(CHIP_VIEW_COL + CHIP_VIEW_W == GRID_COLS - 2u, "the frames end before the margin");
_Static_assert(BAR_COL + BAR_CELLS < VALUE_END - 3u, "a bar ends before its value");
_Static_assert(LIVE_SLOT_FIELDS == 5u && LIVE_SLOT_MIX == 4u,
               "Amount, A, B, Mix after the algorithm");

/* ---- the slots' controls by algorithm (core/fx/fx_slots.h) ---- */

/* Amount, A and B for each algorithm, in ENV order; NULL: not used (its generic name, grey). */
typedef struct {
    const char* name[3];
} controls;

static const controls DIST[] = {
    {{NULL, NULL, NULL}},         /* OFF */
    {{"DRIVE", NULL, NULL}},      /* TAPE */
    {{"DRIVE", "TONE", "BIAS"}},  /* OVERDRIVE */
    {{"DRIVE", "OCT", "BIAS"}},   /* FUZZ: octave-up blend, starved bias */
    {{"BITS", "RATE", NULL}},     /* CRUSH */
    {{"TONE", "HUM", "NOISE"}},   /* LO-FI: TV tone */
};
static const controls MOD[] = {
    {{NULL, NULL, NULL}},         /* OFF */
    {{"DEPTH", "RATE", "ENS"}},   /* CHORUS: ensemble */
    {{"DEPTH", "RATE", "FDBK"}},  /* FLANGER */
    {{"DEPTH", "RATE", "FDBK"}},  /* PHASER */
    {{"DEPTH", "RATE", "SHAPE"}}, /* TREMOLO */
    {{"WIDTH", "RATE", "SHAPE"}}, /* AUTO-PAN */
    {{"WOW", "FLUT", NULL}},      /* WOW: flutter */
};
static const controls FX[] = {
    {{NULL, NULL, NULL}},         /* OFF */
    {{"FDBK", "TIME", "DAMP"}},   /* DELAY */
    {{"FDBK", "TIME", "DAMP"}},   /* PING-PONG */
    {{"DECAY", "DAMP", "DIFF"}},  /* REVERB */
    {{"DECAY", "DAMP", "BAND"}},  /* LUSH: bandwidth */
};
_Static_assert(sizeof DIST / sizeof DIST[0] == SID_DIST_COUNT &&
                   sizeof MOD / sizeof MOD[0] == SID_MOD_COUNT &&
                   sizeof FX / sizeof FX[0] == SID_FXSLOT_COUNT,
               "a name row per algorithm");

static const char* const GENERIC[LIVE_SLOT_FIELDS] = {"ALG", "AMT", "A", "B", "MIX"};

int chip_view_label(uint32_t slot, uint32_t alg, uint32_t field, char* buf) {
    static const controls* const BY_SLOT[LIVE_SLOTS] = {DIST, MOD, FX};
    const char* name = NULL;
    int used = 0;
    if (slot < LIVE_SLOTS && alg < live_slot_algorithms(slot) && field < LIVE_SLOT_FIELDS) {
        if (field == LIVE_SLOT_MIX) {
            used = alg != 0;
        } else if (field != LIVE_SLOT_ALGORITHM) {
            name = BY_SLOT[slot][alg].name[field - LIVE_SLOT_AMOUNT];
            used = name != NULL;
        }
    }
    if (!name) {
        name = field < LIVE_SLOT_FIELDS ? GENERIC[field] : "";
    }
    uint32_t n = 0;
    while (name[n] && n + 1u < CHIP_VIEW_LABEL) {
        buf[n] = name[n];
        n++;
    }
    buf[n] = 0;
    return used;
}

/* ---- the canvas ---- */

static void put(chip_view_canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        c->ch[row][col] = ch;
        c->attr[row][col] = attr;
    }
}

static void fill(chip_view_canvas* c, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                 uint8_t ch, uint8_t attr) {
    for (uint32_t r = row; r < row + h; r++) {
        for (uint32_t k = col; k < col + w; k++) {
            put(c, k, r, ch, attr);
        }
    }
}

/* Upper case (the profiles' names have lower case, which the font shows as upper case
 * anyway). Returns the column after it. */
static uint32_t text(chip_view_canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++) {
        uint8_t ch = (uint8_t)*s;
        put(c, col++, row, ch >= 'a' && ch <= 'z' ? (uint8_t)(ch - 'a' + 'A') : ch, attr);
    }
    return col;
}

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* s right-aligned to end before column end; a text longer than end starts at column 0 (put
 * drops what falls past the grid). */
static void text_right(chip_view_canvas* c, uint32_t end, uint32_t row, const char* s,
                       uint8_t attr) {
    uint32_t n = length(s);
    text(c, n <= end ? end - n : 0u, row, s, attr);
}

/* value (0..999) right-aligned in the 3 cells before column end. */
static void number(chip_view_canvas* c, uint32_t end, uint32_t row, uint32_t value, uint8_t attr) {
    for (uint32_t i = 1; i <= 3u; i++) {
        put(c, end - i, row, i == 1u || value ? (uint8_t)('0' + value % 10u) : (uint8_t)' ', attr);
        value /= 10u;
    }
}

/* A meter of BAR_CELLS bar glyphs filled to value / SID_CHARACTER_MAX (as widget_bar). */
static void bar(chip_view_canvas* c, uint32_t col, uint32_t row, uint32_t value, uint8_t attr) {
    uint32_t px = value * (BAR_CELLS * 8u) / SID_CHARACTER_MAX;
    for (uint32_t i = 0; i < BAR_CELLS; i++) {
        uint32_t f = px >= 8u ? 8u : px;
        put(c, col + i, row, (uint8_t)(FONT_BAR0 + f), attr);
        px -= f;
    }
}

/* A frame of box glyphs (as widget_frame) around h rows of CHIP_VIEW_W cells from row, its
 * title a and b (NULL: none) set into its top edge; the inside cleared. */
static void frame(chip_view_canvas* c, uint32_t row, uint32_t h, const char* a, const char* b,
                  uint8_t attr) {
    const uint32_t col = CHIP_VIEW_COL;
    const uint32_t last = col + CHIP_VIEW_W - 1u;
    fill(c, col + 1u, row, CHIP_VIEW_W - 2u, 1, FONT_BOX_H, attr);
    fill(c, col + 1u, row + h - 1u, CHIP_VIEW_W - 2u, 1, FONT_BOX_H, attr);
    fill(c, col, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    fill(c, last, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    fill(c, col + 1u, row + 1u, CHIP_VIEW_W - 2u, h - 2u, ' ', attr);
    put(c, col, row, FONT_BOX_TL, attr);
    put(c, last, row, FONT_BOX_TR, attr);
    put(c, col, row + h - 1u, FONT_BOX_BL, attr);
    put(c, last, row + h - 1u, FONT_BOX_BR, attr);
    uint32_t k = text(c, col + 1u, row, " ", attr);
    k = text(c, k, row, a, attr);
    if (b) {
        k = text(c, k, row, " ", attr);
        k = text(c, k, row, b, attr);
    }
    put(c, k, row, ' ', attr);
}

/* ---- what it shows ---- */

static uint32_t profile_of(const live* l) {
    return live_param_get(l, LIVE_PARAM_PROFILE);
}

static void look(const chip_view* cv, uint8_t out[CHIP_VIEW_LOOK]) {
    const live* l = cv->l;
    uint32_t n = 0;
    out[n++] = (uint8_t)profile_of(l);
    out[n++] = (uint8_t)l->model_req;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        out[n++] = l->character[f];
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            out[n++] = l->slot[k][f];
        }
    }
    out[n++] = l->fx_select;
    out[n] = cv->chip;
}

static int looks_changed(const chip_view* cv) {
    uint8_t now[CHIP_VIEW_LOOK];
    look(cv, now);
    for (uint32_t i = 0; i < CHIP_VIEW_LOOK; i++) {
        if (now[i] != cv->drawn[i]) {
            return 1;
        }
    }
    return 0;
}

/* The focus's mark in the margin, on its frame's top row. */
static void mark(chip_view_canvas* c, uint32_t row, int on) {
    put(c, 1, row, on ? FONT_PLAY : (uint8_t)' ', on ? FOCUS : TEXT);
}

static void draw_chip(chip_view* cv, chip_view_canvas* c) {
    const live* l = cv->l;
    uint8_t a = cv->chip ? FOCUS : TEXT;
    const sid_chip_profile* p = sid_chip_profile_get(profile_of(l));
    frame(c, CHIP_VIEW_CHIP_ROW, CHIP_H, "CHIP", NULL, a);
    mark(c, CHIP_VIEW_CHIP_ROW, cv->chip);
    text(c, LABEL_COL, CHIP_VIEW_CHIP_ROW + 1u, "PROFILE", a);
    text_right(c, VALUE_END, CHIP_VIEW_CHIP_ROW + 1u, p ? p->name : "?", a);
    text(c, LABEL_COL, CHIP_VIEW_CHIP_ROW + 2u, "MODEL", a);
    /* The model asked for, as PROFILE shows the profile asked for: a step shows at once,
     * while the spare chip for it is prepared (app/reg_ring.h). */
    text_right(c, VALUE_END, CHIP_VIEW_CHIP_ROW + 2u, l->model_req == SID_MODEL_8580 ? "8580" : "6581",
               a);

    static const char* const NAMES[LIVE_CHAR_FIELDS - 1u] = {"AGE", "DAC", "THUMP", "BLEED"};
    frame(c, CHIP_VIEW_CHARACTER_ROW, CHARACTER_H, "CHARACTER", NULL, a);
    mark(c, CHIP_VIEW_CHARACTER_ROW, cv->chip);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        uint32_t row = CHIP_VIEW_CHARACTER_ROW + 1u + f;
        text(c, LABEL_COL, row, NAMES[f], a);
        bar(c, BAR_COL, row, l->character[f], a);
        number(c, VALUE_END, row, l->character[f], a);
    }
}

static void draw_slot(chip_view* cv, chip_view_canvas* c, uint32_t k) {
    const live* l = cv->l;
    const uint8_t* s = l->slot[k];
    uint32_t alg = s[LIVE_SLOT_ALGORITHM];
    uint32_t row = CHIP_VIEW_SLOT_ROW + CHIP_VIEW_SLOT_ROWS * k;
    int focus = !cv->chip && l->fx_select == k;
    uint8_t a = focus ? FOCUS : TEXT;
    frame(c, row, CHIP_VIEW_SLOT_ROWS, views_slot_name(k), views_algorithm_name(k, alg), a);
    mark(c, row, focus);
    for (uint32_t f = 0; f < 4u; f++) {
        char name[CHIP_VIEW_LABEL];
        uint32_t field = LIVE_SLOT_AMOUNT + f;
        uint8_t fa = chip_view_label(k, alg, field, name) ? a : DIM;
        uint32_t r = row + 1u + f / 2u;
        uint32_t col = LABEL_COL + SLOT_HALF * (f % 2u);
        text(c, col, r, name, fa);
        number(c, col + 9u, r, s[field], fa);
    }
}

/* The whole page into the canvas, and what it shows remembered. */
static void build(chip_view* cv) {
    chip_view_canvas* c = &cv->cv;
    look(cv, cv->drawn);
    /* The screen's frame, as widget_screen: the full block on the outer ring. */
    fill(c, 0, 0, GRID_COLS, GRID_ROWS, FONT_FULL, TEXT);
    fill(c, 1, 1, GRID_COLS - 2u, GRID_ROWS - 2u, ' ', TEXT);
    text(c, CHIP_VIEW_COL, TITLE_ROW, "CHIP", TEXT);
    text(c, TAB_A_COL, TITLE_ROW, " A ", TAB);
    text(c, TAB_B_COL, TITLE_ROW, "B M2", DIM);
    draw_chip(cv, c);
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        draw_slot(cv, c, k);
    }
    text(c, CHIP_VIEW_COL, CHIP_VIEW_HELP_ROW, "SELECT MOVE ALGORITHM STEP", TEXT);
    cv->built = 1;
    cv->copying = 1;
}

/* ---- the view ---- */

void chip_view_init(chip_view* cv, live* l) {
    cv->l = l;
    cv->chip = 1;
    cv->built = 0;
    cv->copying = 0;
    cv->cursor = 0;
    cv->held = 0;
}

uint32_t chip_view_focus(const chip_view* cv) {
    return cv->chip ? CHIP_VIEW_CHIP : 1u + cv->l->fx_select % LIVE_SLOTS;
}

uint32_t chip_view_page(const chip_view* cv) {
    return cv->chip ? PANEL_PAGE_CHARACTER : PANEL_PAGE_SLOT;
}

/* Built anew at its first compose: what changed while it was away. */
static void enter(screen* s, void* state) {
    (void)s;
    chip_view* cv = state;
    cv->built = 0;
    cv->cursor = 0;
}

/* steps added to v, clamped to 0..max. */
static uint32_t step_clamped(uint32_t v, int32_t steps, uint32_t max) {
    int32_t n = (int32_t)v + steps;
    return n < 0 ? 0u : (uint32_t)n > max ? max : (uint32_t)n;
}

/* SELECT moves the focus, ALGORITHM steps the profile or the slot's algorithm. */
static int input(screen* s, void* state, const screen_event* e) {
    (void)s;
    chip_view* cv = state;
    live* l = cv->l;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint32_t f = step_clamped(chip_view_focus(cv), e->steps, CHIP_VIEW_SECTIONS - 1u);
        cv->chip = (uint8_t)(f == CHIP_VIEW_CHIP);
        if (!cv->chip) {
            live_fx_select(l, f - 1u);
        }
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        if (cv->chip) {
            live_param_set(l, LIVE_PARAM_PROFILE,
                           step_clamped(profile_of(l), e->steps, SID_CHIPS - 1u));
        } else {
            uint32_t k = l->fx_select % LIVE_SLOTS;
            live_slot_set(l, k, LIVE_SLOT_ALGORITHM,
                          (int32_t)step_clamped(l->slot[k][LIVE_SLOT_ALGORITHM], e->steps,
                                                live_slot_algorithms(k) - 1u));
        }
        return 1;
    }
    return 0;
}

static int changed(screen* s, void* state) {
    (void)s;
    const chip_view* cv = state;
    return !cv->built || cv->copying || looks_changed(cv);
}

/* 1 for a cell of the screen's outer ring. */
static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

/* The canvas onto the grid, as SCENE copies its own (app/scene_view.c): under a wipe the rows
 * it reveals whole; otherwise the changed cells from where the last frame stopped, within the
 * frame's budget, leaving alone what is covered this frame and the border while the raster
 * draws over it. */
static void compose(screen* s, void* state, grid* g) {
    chip_view* cv = state;
    if (!cv->built || looks_changed(cv)) {
        build(cv);
    }
    const chip_view_canvas* c = &cv->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_row(g, r, c->ch[r], c->attr[r]);
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < CHIP_VIEW_FRAME_CELLS ? CHIP_VIEW_FRAME_CELLS - used : 0u;
    uint32_t i = cv->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint32_t r = i / GRID_COLS;
        uint32_t k = i % GRID_COLS;
        if ((whole >> r & 1u) || (g->ch[i] == c->ch[r][k] && g->attr[i] == c->attr[r][k]) ||
            screen_covered(s, k, r) || (raster && border(k, r))) {
            continue;
        }
        if (!left) {
            cv->cursor = (uint16_t)i;
            cv->held++;
            cv->copying = 1;
            return;
        }
        grid_put(g, k, r, c->ch[r][k], c->attr[r][k]);
        left--;
    }
    /* All of it is on the grid but what the layers hold: those come back when the screen
     * composes again after them (it forces that). */
    cv->copying = 0;
}

const view chip_view_view = {"CHIP", enter, NULL, input, compose, changed};
