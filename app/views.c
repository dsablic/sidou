// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views.h"

#include <stddef.h>

#include "font.h"

#define TEXT VIEWS_TEXT
/* The status screen's version: right-aligned to end before this column. */
#define VERSION_END_COL 28u
#define BAR_COL 14u
#define BAR_CELLS 14u
/* The menu: a frame from (2, 4), 26 wide, one row per item. */
#define MENU_COL 2u
#define MENU_ROW 4u
#define MENU_W 26u

static const char* const NOTE_NAMES[12] = {"C",  "C#", "D",  "D#", "E",  "F",
                                           "F#", "G",  "G#", "A",  "A#", "B"};

/* The effect slots (core/fx/fx_slots.h), as the screen names them (at most 9 cells). */
static const char* const SLOT_NAMES[LIVE_SLOTS] = {"DIST", "MOD", "FX"};
static const char* const DIST_NAMES[] = {"OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LO-FI"};
static const char* const MOD_NAMES[] = {"OFF",     "CHORUS",   "FLANGER", "PHASER",
                                        "TREMOLO", "AUTO-PAN", "WOW"};
static const char* const FX_NAMES[] = {"OFF", "DELAY", "PING-PONG", "REVERB", "LUSH"};

static const char* algorithm_name(uint32_t slot, uint32_t alg) {
    static const char* const* const NAMES[LIVE_SLOTS] = {DIST_NAMES, MOD_NAMES, FX_NAMES};
    return alg < live_slot_algorithms(slot) ? NAMES[slot][alg] : "?";
}

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* Text inside the border. */
static void put(grid* g, uint32_t row, uint32_t col, const char* s) {
    while (*s && col < GRID_COLS - 1u) {
        grid_put(g, col++, row, (uint8_t)*s++, TEXT);
    }
}

/* s after buf's text, buf holding at most cap - 1 characters. */
static void append(char* buf, uint32_t cap, const char* s) {
    uint32_t n = length(buf);
    while (*s && n + 1u < cap) {
        buf[n++] = *s++;
    }
    buf[n] = 0;
}

/* a then b into buf. */
static void join(char* buf, uint32_t cap, const char* a, const char* b) {
    buf[0] = 0;
    append(buf, cap, a);
    append(buf, cap, b);
}

/* "+n" / "-n" for an octave. */
static void octave_text(char* buf, int32_t octave) {
    buf[0] = octave < 0 ? '-' : '+';
    buf[1] = (char)('0' + (octave < 0 ? -octave : octave));
    buf[2] = 0;
}

static void put_param(grid* g, uint32_t row, const char* label, uint32_t value, uint32_t max) {
    put(g, row, 2, label);
    widget_number(g, 9, row, 4, (int32_t)value, 0, TEXT);
    widget_bar(g, BAR_COL, row, BAR_CELLS, value, max, TEXT);
}

/* ---- what the views show ---- */

/* The fields STATUS and MENU show, as bytes, with the menu's blink phase (0 for STATUS). */
static void look(const views* v, uint32_t blink, uint8_t out[VIEWS_LOOK]) {
    const live* l = v->l;
    uint32_t n = 0;
    out[n++] = (uint8_t)l->model;
    out[n++] = l->preset;
    out[n++] = (uint8_t)l->octave;
    out[n++] = (uint8_t)l->cutoff;
    out[n++] = (uint8_t)(l->cutoff >> 8);
    out[n++] = l->resonance;
    out[n++] = (uint8_t)l->pulse_width;
    out[n++] = (uint8_t)(l->pulse_width >> 8);
    out[n++] = l->volume;
    out[n++] = l->fx_select;
    for (uint32_t k = 0; k < LIVE_VOICES; k++) {
        out[n++] = l->voice[k].gate ? l->voice[k].note : LIVE_NO_NOTE;
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            out[n++] = l->slot[k][f];
        }
    }
    out[n++] = v->home.cursor;
    out[n++] = v->home.top;
    out[n] = (uint8_t)blink;
}

/* 1 when the view on top would show other fields than at its last compose. */
static int looks_changed(const views* v, uint32_t blink) {
    uint8_t now[VIEWS_LOOK];
    look(v, blink, now);
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        if (now[i] != v->drawn[i]) {
            return 1;
        }
    }
    return 0;
}

/* ---- STATUS ---- */

static int status_changed(screen* s, void* state) {
    (void)s;
    return looks_changed(state, 0);
}

static void status_compose(screen* s, void* state, grid* g) {
    (void)s;
    views* v = state;
    const live* l = v->l;
    look(v, 0, v->drawn);
    widget_screen(g, FONT_FULL, TEXT);
    put(g, 2, 2, "SIDOU");
    put(g, 2, VERSION_END_COL - length(v->version), v->version);

    put(g, 5, 2, "CHIP");
    put(g, 5, 9, l->model == SID_MODEL_8580 ? "8580" : "6581");
    put(g, 6, 2, "PRESET");
    const live_preset* p = live_preset_get(l->preset);
    put(g, 6, 9, p ? p->name : "?");
    put(g, 7, 2, "OCTAVE");
    widget_number(g, 9, 7, 2, l->octave, 1, TEXT);

    put_param(g, 10, "CUTOFF", l->cutoff, LIVE_CUTOFF_MAX);
    put_param(g, 11, "RESO", l->resonance, LIVE_RESONANCE_MAX);
    put_param(g, 12, "PULSE", l->pulse_width, LIVE_PULSE_WIDTH_MAX);
    put_param(g, 13, "VOLUME", l->volume, 15);

    put(g, 16, 2, "NOTES");
    uint32_t col = 9;
    for (int k = 0; k < LIVE_VOICES; k++) {
        uint8_t n = l->voice[k].note;
        if (!l->voice[k].gate || n == LIVE_NO_NOTE) {
            continue;
        }
        const char* name = NOTE_NAMES[n % 12u];
        put(g, 16, col, name);
        uint32_t oc = col + (name[1] ? 2u : 1u);
        if (n < 12u) {
            put(g, 16, oc, "-1");
        } else {
            widget_number(g, oc, 16, 1, n / 12 - 1, 0, TEXT);
        }
        col += 5u;
    }

    /* The effect slots: '>' marks the one EDIT + K1-K4 and ENV act on (SCL moves it). */
    put(g, 18, 2, "EFFECTS");
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint32_t row = 19u + k;
        if (k == l->fx_select) {
            put(g, row, 2, ">");
        }
        put(g, row, 3, SLOT_NAMES[k]);
        put(g, row, 8, algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]));
        put(g, row, 19, "MIX");
        widget_number(g, 23, row, 3, l->slot[k][LIVE_SLOT_MIX], 0, TEXT);
    }
    const uint8_t* sel = l->slot[l->fx_select];
    put(g, 22, 3, "AMT");
    widget_number(g, 7, 22, 3, sel[LIVE_SLOT_AMOUNT], 0, TEXT);
    put(g, 22, 12, "A");
    widget_number(g, 14, 22, 3, sel[LIVE_SLOT_A], 0, TEXT);
    put(g, 22, 19, "B");
    widget_number(g, 21, 22, 3, sel[LIVE_SLOT_B], 0, TEXT);

    put(g, 26, 2, "READY.");
}

const view views_status = {"STATUS", NULL, NULL, NULL, status_compose, status_changed};

/* ---- MENU ---- */

static void chip_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->l->model == SID_MODEL_8580 ? "8580" : "6581", "");
}

/* Clockwise the 8580, counter-clockwise the 6581; turning on past either does nothing. */
static void chip_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    sid_model want = steps > 0 ? SID_MODEL_8580 : SID_MODEL_6581;
    if (steps != 0 && v->l->model != want) {
        live_button(v->l, LIVE_BUTTON_FX); /* the toggle */
    }
}

static void preset_value(void* ctx, char* buf) {
    const views* v = ctx;
    const live_preset* p = live_preset_get(v->l->preset);
    join(buf, MENU_VALUE_CHARS, p ? p->name : "?", "");
}

static void preset_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    live_knob(v->l, LIVE_KNOB_PRESET, steps);
}

static void octave_value(void* ctx, char* buf) {
    const views* v = ctx;
    octave_text(buf, v->l->octave);
}

static void octave_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    live_button_id b = steps > 0 ? LIVE_BUTTON_OCT_UP : LIVE_BUTTON_OCT_DOWN;
    for (int32_t n = steps > 0 ? steps : -steps; n > 0; n--) {
        live_button(v->l, b);
    }
}

static void charset_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_charset, v, SCREEN_WIPE_DOWN);
}

static const menu_item MENU_ITEMS[VIEWS_MENU_ITEMS] = {
    {"CHIP", chip_value, chip_turn},
    {"PRESET", preset_value, preset_turn},
    {"OCTAVE", octave_value, octave_turn},
    {"CHARSET", NULL, charset_open},
};

static int menu_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        menu_move(&v->home, e->steps);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        menu_turn(&v->home, e->steps);
        return 1;
    }
    return 0;
}

static int menu_changed(screen* s, void* state) {
    return looks_changed(state, (uint32_t)screen_blink(s));
}

static void menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    look(v, (uint32_t)screen_blink(s), v->drawn);
    widget_screen(g, FONT_FULL, TEXT);
    put(g, 2, 2, "SIDOU");
    put(g, 2, VERSION_END_COL - length(v->version), v->version);
    widget_frame(g, MENU_COL, MENU_ROW, MENU_W, VIEWS_MENU_ITEMS + 2u, TEXT, "MENU");
    menu_draw(&v->home, g, MENU_COL + 1u, MENU_ROW + 1u, MENU_W - 2u, TEXT, screen_blink(s));
    put(g, 12, 2, "SELECT     MOVE");
    put(g, 13, 2, "ALGORITHM  CHANGE");
    put(g, 14, 2, "HOME       BACK");
    put(g, 26, 2, "READY.");
}

const view views_menu = {"MENU", NULL, NULL, menu_input, menu_compose, menu_changed};

/* ---- CHARSET ---- */

/* The 256 glyphs in a 16 x 16 block with hex labels, the 16 colours, and a triangle wave
 * drawn as pixel overlays over a row of line glyphs. On black, so that switching between it
 * and the status screen changes every cell. */
static void charset_compose(screen* s, void* state, grid* g) {
    (void)s;
    (void)state;
    static const char HEX[] = "0123456789ABCDEF";
    const uint8_t frame = GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK);
    const uint8_t label = GRID_ATTR(GRID_YELLOW, GRID_BLACK);
    grid_fill(g, 0, 0, GRID_COLS, GRID_ROWS, ' ', frame);
    grid_box(g, 0, 0, GRID_COLS, GRID_ROWS, frame, 1);
    grid_text(g, 2, 0, " CHARSET ", GRID_ATTR(GRID_WHITE, GRID_BLACK));
    for (uint32_t i = 0; i < 16u; i++) {
        grid_put(g, 7u + i, 2, (uint8_t)HEX[i], label);
        grid_put(g, 4, 4u + i, (uint8_t)HEX[i], label);
        for (uint32_t c = 0; c < 16u; c++) {
            grid_put(g, 7u + c, 4u + i, (uint8_t)(i * 16u + c), GRID_ATTR(GRID_WHITE, GRID_BLACK));
        }
        grid_put(g, 7u + i, 22, FONT_FULL, GRID_ATTR(i, GRID_BLACK));
        grid_put(g, 7u + i, 23, FONT_D8, GRID_ATTR(i, GRID_BLACK));
        grid_put(g, 7u + i, 26, FONT_BOX_H, GRID_ATTR(GRID_DARK_GREY, GRID_BLACK));
        /* 4 pixels per cell rise or fall: a 32-pixel period, rows 1..6 */
        uint8_t mask[8] = {0};
        for (uint32_t x = 0; x < 8u; x++) {
            uint32_t p = (i * 8u + x) % 32u;
            uint32_t y = p < 16u ? 1u + p * 5u / 15u : 6u - (p - 16u) * 5u / 15u;
            mask[y] |= (uint8_t)(0x80u >> x);
        }
        grid_overlay(g, 7u + i, 26, mask, GRID_LIGHT_GREEN);
    }
    grid_text(g, 2, 22, "COL", label);
    grid_text(g, 2, 26, "OVL", label);
}

/* The sheet never changes: composed when the screen needs it only. */
static int charset_changed(screen* s, void* state) {
    (void)s;
    (void)state;
    return 0;
}

const view views_charset = {"CHARSET", NULL, NULL, NULL, charset_compose, charset_changed};

/* ---- routing ---- */

static void remember(views* v) {
    const live* l = v->l;
    v->model = (uint8_t)l->model;
    v->octave = l->octave;
    v->fx_select = l->fx_select;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        v->algorithm[k] = l->slot[k][LIVE_SLOT_ALGORITHM];
    }
}

/* HOME: the menu from the root view, back one view from any other. */
static int global(void* ctx, const screen_event* e) {
    views* v = ctx;
    if (e->kind != SCREEN_PRESS || e->id != PANEL_BTN_HOME) {
        return 0;
    }
    if (screen_pop(&v->s, SCREEN_WIPE_UP) != 0) {
        screen_push(&v->s, &views_menu, v, SCREEN_WIPE_DOWN);
    }
    return 1;
}

void views_init(views* v, grid* g, live* l, const char* version) {
    v->l = l;
    v->version = version;
    menu_init(&v->home, MENU_ITEMS, VIEWS_MENU_ITEMS, VIEWS_MENU_ITEMS, v);
    remember(v);
    screen_init(&v->s, g, &views_status, v);
    screen_global(&v->s, global, v);
}

void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held) {
    uint32_t rest = pressed;
    for (uint32_t b = 0; b < 32u && pressed >> b; b++) {
        if (!((pressed >> b) & 1u)) {
            continue;
        }
        screen_event e = {SCREEN_PRESS, (uint8_t)b, 0, held};
        if (screen_input(&v->s, &e)) {
            rest &= ~(1u << b);
        }
    }
    panel_buttons(p, v->l, rest, held);
}

void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps) {
    if (steps == 0) {
        return;
    }
    int32_t clamped = steps > INT16_MAX ? INT16_MAX : steps < -INT16_MAX ? -INT16_MAX : steps;
    screen_event e = {SCREEN_TURN, (uint8_t)enc, (int16_t)clamped, p->held};
    if (!screen_input(&v->s, &e)) {
        panel_encoder(p, v->l, enc, steps);
    }
}

/* What changed since the last frame, as a toast; a chip change also runs the raster. The last
 * of several changes in one frame wins the toast. */
static void tell(views* v) {
    const live* l = v->l;
    char buf[SCREEN_TOAST_CHARS + 1u];
    char num[3];
    int changed = 0;
    if (l->octave != v->octave) {
        octave_text(num, l->octave);
        join(buf, sizeof buf, "OCTAVE ", num);
        changed = 1;
    }
    if (l->fx_select != v->fx_select) {
        join(buf, sizeof buf, "SLOT ", SLOT_NAMES[l->fx_select % LIVE_SLOTS]);
        changed = 1;
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint8_t alg = l->slot[k][LIVE_SLOT_ALGORITHM];
        if (alg != v->algorithm[k]) {
            join(buf, sizeof buf, SLOT_NAMES[k], " ");
            append(buf, sizeof buf, algorithm_name(k, alg));
            changed = 1;
        }
    }
    if ((uint8_t)l->model != v->model) {
        join(buf, sizeof buf, "CHIP ", l->model == SID_MODEL_8580 ? "8580" : "6581");
        screen_raster(&v->s, VIEWS_RASTER_STEPS);
        changed = 1;
    }
    if (changed) {
        screen_toast(&v->s, buf, SCREEN_TOAST_MS);
    }
    remember(v);
}

void views_frame(views* v, uint32_t now_ms) {
    tell(v);
    screen_frame(&v->s, now_ms);
}

void views_update(views* v, uint32_t step, int32_t code) {
    static const char* const STEP[6] = {"", "PACKAGE", "CHECK HEAD", "LOADER", "CONFIRM",
                                        "RESTART"};
    grid* g = v->s.g;
    widget_frame(g, 2, 18, 26, 5, TEXT, "UPDATE");
    grid_fill(g, 3, 19, 24, 3, ' ', TEXT);
    if (step < 9u) {
        grid_text(g, 4, 20, STEP[step < 6u ? step : 0u], TEXT);
    } else if (code == 1) {
        grid_text(g, 4, 20, "DRY RUN OK", TEXT);
    } else {
        grid_text(g, 4, 20, "FAILED", TEXT);
        widget_number(g, 11, 20, 3, code, 1, TEXT);
    }
    grid_fill(g, 2, 26, 6, 1, ' ', TEXT); /* READY. */
    screen_redraw(&v->s);
}
