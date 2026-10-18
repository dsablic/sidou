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
/* The menu: large text over the whole width, 14 characters a line with a cell of margin either
 * side. The title bar in rows 0-2 (its text in rows 1-2: the position, and an arrow up and down
 * for the items out of sight), the items from row MENU_ROW, a line every 3 rows, the help in
 * rows 24-27. */
#define MENU_ROW 4u
#define MENU_HEAD GRID_ATTR(GRID_BLUE, GRID_LIGHT_BLUE)
#define MENU_HELP_ROW 24u
#define MENU_CHARS ((GRID_COLS - 2u) / 2u)
/* The cells a frame of the menu sends at most (LIVE, SCENE and CHIP hold to 200: 17.5 ms of SPI
 * with their windows). */
#define MENU_FRAME_CELLS 190u
/* A control asked for the popup (views_encoder, master_moved): ASK_BASE, its encoder (a K
 * knob, ALGORITHM, ASK_MASTER for the MASTER pot) and ASK_EDIT when EDIT was held. */
#define ASK_BASE 0x20u
#define ASK_EDIT 8u
#define ASK_MASTER 7u
/* A parameter changed, not a control: ASK_PARAM and its parameter (live.h), from MIDI's watcher. */
#define ASK_PARAM 0x40u

/* STATUS and MENU's shared snapshot, without STORE's two menu-only bytes. */
#define COMMON_LOOK (VIEWS_LOOK - 2u)

static const char* const NOTE_NAMES[12] = {"C",  "C#", "D",  "D#", "E",  "F",
                                           "F#", "G",  "G#", "A",  "A#", "B"};

/* The effect slots (core/fx/fx_slots.h), as the screen names them (at most 9 cells). */
static const char* const SLOT_NAMES[LIVE_SLOTS] = {"DIST", "MOD", "FX"};
static const char* const DIST_NAMES[] = {"OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LO-FI"};
static const char* const MOD_NAMES[] = {"OFF",     "CHORUS",   "FLANGER", "PHASER",
                                        "TREMOLO", "AUTO-PAN", "WOW"};
static const char* const FX_NAMES[] = {"OFF", "DELAY", "PING-PONG", "REVERB", "LUSH"};

const char* views_algorithm_name(uint32_t slot, uint32_t alg) {
    static const char* const* const NAMES[LIVE_SLOTS] = {DIST_NAMES, MOD_NAMES, FX_NAMES};
    return slot < LIVE_SLOTS && alg < live_slot_algorithms(slot) ? NAMES[slot][alg] : "?";
}

const char* views_slot_name(uint32_t slot) {
    return slot < LIVE_SLOTS ? SLOT_NAMES[slot] : "?";
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

/* n (0..9999) in decimal into buf (5 characters). */
static void number_text(char* buf, uint32_t n) {
    char d[5];
    uint32_t k = 0;
    do {
        d[k++] = (char)('0' + n % 10u);
        n /= 10u;
    } while (n && k < 4u);
    for (uint32_t i = 0; i < k; i++) {
        buf[i] = d[k - 1u - i];
    }
    buf[k] = 0;
}

static void put_param(grid* g, uint32_t row, const char* label, uint32_t value, uint32_t max) {
    put(g, row, 2, label);
    widget_number(g, 9, row, 4, (int32_t)value, 0, TEXT);
    widget_bar(g, BAR_COL, row, BAR_CELLS, value, max, TEXT);
}

/* A UI sound (app/ui_sound.h), if there are UI sounds: ui_sound_play does nothing with
 * SOUNDS off. */
static void sound(views* v, ui_sound_id id) {
    if (v->snd) {
        ui_sound_play(v->snd, id);
    }
}

/* ---- what the views show ---- */

/* The menu's STORE: its state (STORE_LOOK_*) and, for a store that writes, its free kilobytes
 * (or, while a format runs, the sectors it still erases) in *n. */
enum { STORE_LOOK_NONE, STORE_LOOK_FREE, STORE_LOOK_FOREIGN, STORE_LOOK_ARMED, STORE_LOOK_ERASING };
static uint32_t store_look(const views* v, uint32_t* n) {
    const store* st = v->set ? v->set->st : NULL;
    if (!st) {
        return STORE_LOOK_NONE;
    }
    store_state_id state = store_state(st);
    if (state == STORE_STATE_FOREIGN) {
        return v->format_arm ? STORE_LOOK_ARMED : STORE_LOOK_FOREIGN;
    }
    if (state != STORE_STATE_BLANK && state != STORE_STATE_SIDOU) {
        return STORE_LOOK_NONE; /* not mounted */
    }
    store_stats stats;
    store_stats_get(st, &stats);
    if (v->formatting) {
        *n = stats.dirty;
        return STORE_LOOK_ERASING;
    }
    *n = stats.free / 1024u;
    return STORE_LOOK_FREE;
}

/* The fields STATUS and MENU show, as bytes, with the menu's blink phase (0 for STATUS). */
static void look(const views* v, uint32_t blink, uint8_t out[COMMON_LOOK]) {
    const live* l = v->l;
    uint32_t n = 0;
    out[n++] = (uint8_t)l->model_req;
    out[n++] = (uint8_t)(l->keys_chip | l->two_chips << 1 | l->keys_mode << 2);
    out[n++] = l->profile_req;
    out[n++] = l->other_profile;
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
    out[n++] = v->cards;
    out[n++] = v->home.cursor;
    out[n++] = v->home.top;
    out[n] = (uint8_t)blink;
}

/* The common snapshot followed by the menu's STORE state and kilobytes. */
__attribute__((noinline, unused)) static void menu_look(const views* v, uint32_t blink,
                                                     uint8_t out[VIEWS_LOOK]) {
    look(v, blink, out);
    uint32_t kb = 0;
    out[COMMON_LOOK] = (uint8_t)store_look(v, &kb);
    out[COMMON_LOOK + 1u] = (uint8_t)kb;
}

/* 1 when the view on top would show other fields than at its last compose. */
static int looks_changed(const views* v, uint32_t blink, int menu_showing) {
    uint8_t now[VIEWS_LOOK];
    if (menu_showing) {
        menu_look(v, blink, now);
    } else {
        look(v, blink, now);
    }
    for (uint32_t i = 0; i < (menu_showing ? VIEWS_LOOK : COMMON_LOOK); i++) {
        if (now[i] != v->drawn[i]) {
            return 1;
        }
    }
    return 0;
}

/* ---- STATUS ---- */

static int status_changed(screen* s, void* state) {
    (void)s;
    return looks_changed(state, 0, 0);
}

static void status_compose(screen* s, void* state, grid* g) {
    (void)s;
    views* v = state;
    const live* l = v->l;
    look(v, 0, v->drawn);
    widget_screen(g, FONT_FULL, TEXT);
    put(g, 2, 2, "SIDOU");
    put(g, 2, VERSION_END_COL - length(v->version), v->version);

    uint32_t top = 5;
    if (l->two_chips) {
        /* Both chips' profiles (their model in the name); KEYS marks the one the keys play
         * (app/keys_route.h): the keys' setup as asked for, the other's as it has it. */
        for (uint32_t x = 0; x < 2u; x++, top++) {
            uint32_t id = x == l->keys_chip ? l->profile_req : l->other_profile;
            const sid_chip_profile* cp = sid_chip_profile_get(id);
            put(g, top, 2, x ? "CHIP B" : "CHIP A");
            put(g, top, 9, cp ? cp->name : "--"); /* not made yet */
            if (x == l->keys_chip) {
                put(g, top, 20, "KEYS");
            }
        }
    } else {
        put(g, 5, 2, "CHIP");
        put(g, 5, 9, l->model_req == SID_MODEL_8580 ? "8580" : "6581");
        top = 6;
    }
    put(g, top, 2, "PRESET");
    const live_preset* p = live_preset_get(l->preset);
    put(g, top, 9, p ? p->name : "?");
    put(g, top + 1u, 2, "OCTAVE");
    widget_number(g, 9, top + 1u, 2, l->octave, 1, TEXT);

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
        put(g, row, 8, views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]));
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

static void cards_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->cards ? "ON" : "OFF", "");
}

/* Clockwise on, counter-clockwise off. */
static void cards_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        v->cards = (uint8_t)(steps > 0);
        v->l->settings_dirty = 1;
    }
}

static void keys_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->l->keys_mode == LIVE_KEYS_A ? "A" : "AUTO", "");
}

/* Clockwise A (the keys stay on chip A during a song), counter-clockwise AUTO. */
static void keys_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        live_set_keys_mode(v->l, steps > 0 ? LIVE_KEYS_A : LIVE_KEYS_AUTO);
    }
}

static void sounds_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->snd && v->snd->on ? "ON" : "OFF", "");
}

/* Clockwise on, counter-clockwise off. */
static void sounds_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0 && v->snd) {
        ui_sound_set(v->snd, steps > 0);
    }
}

static void store_value(void* ctx, char* buf) {
    const views* v = ctx;
    uint32_t n = 0;
    char num[4];
    switch (store_look(v, &n)) {
    case STORE_LOOK_FREE:
    case STORE_LOOK_ERASING: {
        uint32_t k = 0;
        for (uint32_t p = n >= 100u ? 100u : n >= 10u ? 10u : 1u; p; p /= 10u) {
            num[k++] = (char)('0' + n / p % 10u);
        }
        num[k] = 0;
        if (v->formatting) {
            join(buf, MENU_VALUE_CHARS, "ERASE ", num);
        } else {
            join(buf, MENU_VALUE_CHARS, num, "K");
        }
        break;
    }
    case STORE_LOOK_FOREIGN: join(buf, MENU_VALUE_CHARS, "FOREIGN", ""); break;
    case STORE_LOOK_ARMED: join(buf, MENU_VALUE_CHARS, "FORMAT?", ""); break;
    default: join(buf, MENU_VALUE_CHARS, "NONE", ""); break;
    }
}

/* On a foreign store: clockwise once arms the format, twice formats (every sector erased, the
 * erases forced: the user asked for their gaps); counter-clockwise disarms. Else nothing. */
static void store_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    store* st = v->set ? v->set->st : NULL;
    if (!st || store_state(st) != STORE_STATE_FOREIGN || steps == 0) {
        v->format_arm = 0;
        return;
    }
    if (steps < 0) {
        v->format_arm = 0;
    } else if (v->format_arm || steps > 1) {
        v->format_arm = 0;
        store_format(st);
        settings_forget(v->set);
        user_profiles_forget(&v->users);
        v->formatting = 1;
        if (v->set->force) {
            v->set->force(1);
        }
    } else {
        v->format_arm = 1;
    }
}

/* Title card id over the view on top, if the cards are on: from the views' input only (a push
 * from the frame would draw a frame of the view first). */
static void show_card(views* v, title_card_id id) {
    if (v->cards) {
        title_card_init(&v->card, id, v->l);
        screen_push(&v->s, &title_card_view, &v->card, SCREEN_CUT);
    }
}

static void chip_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &chip_view_view, &v->chip, SCREEN_WIPE_DOWN);
    show_card(v, TITLE_CARD_CHIP);
}

static void rows_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->song.scroll ? "SCROLL" : "BAR", "");
}

/* Clockwise SCROLL (GoatTracker's), counter-clockwise BAR (the moving bar). */
static void rows_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        v->song.scroll = (uint8_t)(steps > 0);
        v->l->settings_dirty = 1;
        screen_redraw(&v->s);
    }
}

static void tracker_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &tracker_view_view, &v->song, SCREEN_WIPE_DOWN);
}

static void status_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_status, v, SCREEN_WIPE_DOWN);
}

static void scene_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &scene_view_view, &v->scene, SCREEN_WIPE_DOWN);
    show_card(v, TITLE_CARD_SCENE);
}

static void charset_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_charset, v, SCREEN_WIPE_DOWN);
}

static void about_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    about_init(&v->about, v->version);
    screen_push(&v->s, &about_view, &v->about, SCREEN_WIPE_DOWN);
}

/* PRE is PRESET and ROWS TRACKER ROWS: the 14-character lines hold the longest preset name
 * (NOISE PERC) and STORE's value (FOREIGN, FORMAT?, ERASE 20; 123K, the kilobytes free) beside
 * them. The popup (below) says the whole names. */
static const menu_item MENU_ITEMS[VIEWS_MENU_ITEMS] = {
    {"CHIP", NULL, chip_open},             {"TRACKER", NULL, tracker_open},
    {"PRE", preset_value, preset_turn},    {"OCTAVE", octave_value, octave_turn},
    {"CARDS", cards_value, cards_turn},    {"STORE", store_value, store_turn},
    {"KEYS", keys_value, keys_turn},       {"SOUNDS", sounds_value, sounds_turn},
    {"ROWS", rows_value, rows_turn},       {"STATUS", NULL, status_open},
    {"SCENE", NULL, scene_open},           {"CHARSET", NULL, charset_open},
    {"ABOUT", NULL, about_open},
};
_Static_assert(VIEWS_MENU_CHIP == 0u && VIEWS_MENU_TRACKER == 1u && VIEWS_MENU_PRESET == 2u &&
                   VIEWS_MENU_CARDS == 4u && VIEWS_MENU_STORE == 5u && VIEWS_MENU_KEYS == 6u &&
                   VIEWS_MENU_SOUNDS == 7u && VIEWS_MENU_ROWS == 8u && VIEWS_MENU_STATUS == 9u &&
                   VIEWS_MENU_SCENE == 10u && VIEWS_MENU_CHARSET == 11u &&
                   VIEWS_MENU_ABOUT == 12u,
               "the menu's items");

static int menu_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint8_t was = v->home.cursor;
        menu_move(&v->home, e->steps);
        v->format_arm = 0; /* the confirm is two turns on the item, nothing between */
        sound(v, v->home.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ); /* the end: refused */
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        /* menu_turn's own rule: an item with a value turns both ways, an action only
         * clockwise; else refused. */
        const menu_item* it = &v->home.items[v->home.cursor];
        int does = it->turn && (it->value || e->steps > 0);
        if (v->home.cursor == VIEWS_MENU_STORE) {
            does = v->set && v->set->st && store_state(v->set->st) == STORE_STATE_FOREIGN;
        }
        menu_turn(&v->home, e->steps);
        sound(v, does ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
        return 1;
    }
    return 0;
}

/* The selection is a line in reverse video, which does not blink: the menu composes when what
 * it shows changes, not on a phase. */
static int menu_changed(screen* s, void* state) {
    (void)s;
    const views* v = state;
    return v->home.pending || looks_changed(v, 0, 1);
}

static void menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    menu_look(v, 0, v->drawn);
    /* Not the lines (menu_draw): the rest of the screen blank, the title bar and the help. */
    grid_fill(g, 0, 0, GRID_COLS, MENU_ROW, ' ', TEXT);
    grid_fill(g, 0, MENU_ROW + 3u * VIEWS_MENU_SHOWN, GRID_COLS,
              GRID_ROWS - MENU_ROW - 3u * VIEWS_MENU_SHOWN, ' ', TEXT);
    /* The title bar: MENU and the item the cursor is on, of how many. */
    char head[MENU_CHARS + 1u];
    char pos[8];
    char num[4];
    number_text(num, v->home.cursor + 1u);
    join(pos, sizeof pos, num, "/");
    number_text(num, VIEWS_MENU_ITEMS);
    append(pos, sizeof pos, num);
    join(head, sizeof head, "MENU", "");
    uint32_t n = length(head);
    while (n < sizeof head - 1u - length(pos)) {
        head[n++] = ' ';
    }
    head[n] = 0;
    append(head, sizeof head, pos);
    head[6] = v->home.top > 0 ? (char)FONT_ARROW_UP : ' ';
    head[7] = v->home.top + VIEWS_MENU_SHOWN < VIEWS_MENU_ITEMS ? (char)FONT_ARROW_DOWN : ' ';
    grid_fill(g, 0, 0, GRID_COLS, 3, ' ', MENU_HEAD);
    grid_text_big(g, 1, 1, 2, head, MENU_HEAD);
    /* Lines while the frame stays within MENU_FRAME_CELLS, less what the popup or a toast draws
     * the same frame (screen_layer_cells): a line is up to 60 cells, a scroll in one frame
     * would send 250. Except the lines something drew over or a view switch left. */
    uint32_t layers = screen_layer_cells(s);
    v->home.budget = (uint16_t)(MENU_FRAME_CELLS > layers ? MENU_FRAME_CELLS - layers : 1u);
    v->home.force_rows = s->redraw ? s->redraw_rows : 0u;
    menu_draw(&v->home, g, 0, MENU_ROW, GRID_COLS, TEXT, 0);
    /* What the encoders do (ALG is ALGORITHM): HOME goes back, as everywhere. */
    grid_text_big(g, 1, MENU_HELP_ROW, 2, "SELECT  MOVE", TEXT);
    grid_text_big(g, 1, MENU_HELP_ROW + 2u, 2, "ALG   CHANGE", TEXT);
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
            uint32_t code = i * 16u + c;
            /* (the codes of large text's pieces are blank glyphs, whose cells would draw one) */
            grid_put(g, 7u + c, 4u + i,
                     code - GRID_BIG_FIRST < GRID_BIG_PIECES ? ' ' : (uint8_t)code,
                     GRID_ATTR(GRID_WHITE, GRID_BLACK));
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
    v->model = (uint8_t)l->model_req;
    v->octave = l->octave;
    v->fx_select = l->fx_select;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        v->algorithm[k] = l->slot[k][LIVE_SLOT_ALGORITHM];
    }
}

/* SEQ: the first song view over the view on top, or in one the next (app/song_view.h). */
static void song_step(views* v) {
    const view* top = screen_top(&v->s);
    const view* next = song_view_next(top);
    if (!next || next == top) {
        return;
    }
    if (song_view_is(top)) {
        screen_pop(&v->s, SCREEN_CUT);
    }
    screen_push(&v->s, next, &v->song, SCREEN_WIPE_DOWN);
}

/* HOME: the menu from the root view, back one view from any other: from the menu to LIVE,
 * behind THE OSCILLATOR's card. SEQ, PLAY and REC: the song views'. */
static int global(void* ctx, const screen_event* e) {
    views* v = ctx;
    if (e->kind != SCREEN_PRESS) {
        return 0; /* an encoder's turn: nothing here is global */
    }
    if (e->id == PANEL_BTN_HOME) { /* first: the tightest of the input gates (emu-gate screen.rs) */
        v->format_arm = 0;
        const view* from = screen_top(&v->s);
        if (screen_pop(&v->s, SCREEN_WIPE_UP) != 0) {
            screen_push(&v->s, &views_menu, v, SCREEN_WIPE_DOWN);
        } else if (from == &views_menu && screen_top(&v->s) == &live_view_view) {
            show_card(v, TITLE_CARD_OSCILLATOR);
        }
        return 1;
    }
    if (e->id == PANEL_BTN_SEQ) {
        song_step(v);
        return 1;
    }
    if (e->id == PANEL_BTN_PLAY) {
        song_view_play(&v->song, (int)(e->held >> PANEL_BTN_EDIT & 1u));
        return 1;
    }
    if (e->id == PANEL_BTN_REC) {
        song_view_rec(&v->song);
        return 1;
    }
    return 0;
}

/* MASTER moved the volume: the popup, as a K knob's. */
static void master_moved(void* ctx) {
    views* v = ctx;
    v->s.popup_ask = ASK_BASE | ASK_MASTER;
    v->s.popup_view = NULL; /* whichever view is on top */
    v->s.layers = 1;
}

void views_set_panel(views* v, panel* p) {
    p->master_hook = master_moved;
    p->master_ctx = v;
}

void views_set_sounds(views* v, ui_sound* us) {
    v->snd = us;
}

void views_set_keys(views* v, keys_route* kr) {
    chip_view_set_keys(&v->chip, kr);
}

void views_set_song(views* v, const song_transport* t) {
    song_view_set_transport(&v->song, t);
}

static int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
                      uint32_t* max);

/* The parameters views_popup_midi watches: the knobs' and the volume. */
static const uint8_t WATCHED[4] = {LIVE_PARAM_CUTOFF, LIVE_PARAM_RESONANCE,
                                   LIVE_PARAM_PULSE_WIDTH, LIVE_PARAM_VOLUME};

/* Every frame while views_popup_midi is on: a watched parameter that moved asks for its popup,
 * unless a panel control already asked this frame (it names the control itself). */
static void midi_watch(void* ctx) {
    views* v = ctx;
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t now = live_param_get(v->l, WATCHED[k]);
        if (now != v->watched[k]) {
            v->watched[k] = (uint16_t)now;
            if (!v->s.popup_ask) {
                v->s.popup_ask = (uint8_t)(ASK_PARAM | WATCHED[k]);
                v->s.popup_view = NULL;
            }
        }
    }
}

void views_popup_midi(views* v, int on) {
    for (uint32_t k = 0; k < 4u; k++) {
        v->watched[k] = (uint16_t)live_param_get(v->l, WATCHED[k]);
    }
    screen_popup_watch(&v->s, on ? midi_watch : NULL, on ? v : NULL);
}

void views_init(views* v, grid* g, live* l, scope* sc, const char* version) {
    v->l = l;
    v->sc = sc;
    v->version = version;
    v->cards = 1;
    v->snd = NULL;
    v->set = NULL;
    v->format_arm = 0;
    v->formatting = 0;
    save_key_init(&v->key);
    menu_init(&v->home, MENU_ITEMS, VIEWS_MENU_ITEMS, VIEWS_MENU_SHOWN, v);
    v->home.scale = 2;
    v->home.sel_attr = MENU_HEAD; /* the selected line in reverse video */
    remember(v);
    v->p = NULL;
    learn_view_init(&v->learn, l, NULL);
    chip_view_init(&v->chip, l);
    user_profiles_init(&v->users, l, NULL);
    chip_view_set_users(&v->chip, &v->users);
    live_view_init(&v->live, l, sc);
    scene_view_init(&v->scene, l, sc);
    song_view_init(&v->song, l);
    screen_init(&v->s, g, &live_view_view, &v->live);
    screen_global(&v->s, global, v);
    screen_popup_source(&v->s, &v->popup, popup_text, v);
    if (VIEWS_POPUP_MIDI) {
        views_popup_midi(v, 1);
    }
}

void views_learn(views* v, midi_learn* ml) {
    learn_view_init(&v->learn, v->l, ml);
}

void views_settings(views* v, settings* set) {
    v->set = set;
    v->users.st = set ? set->st : NULL;
    user_profiles_load(&v->users);
}

void views_boot(views* v, int full, int home_held) {
    boot_init(&v->boot, v->l, v->version, full || home_held, home_held);
    screen_push(&v->s, &boot_view, &v->boot, SCREEN_CUT);
}

/* The boot on top: its time, and what it asks of the screen. */
static void boot_frame(views* v, uint32_t now_ms) {
    if (screen_top(&v->s) != &boot_view) {
        return;
    }
    switch (boot_update(&v->boot, now_ms)) {
    case BOOT_REVEAL:
        screen_reveal(&v->s, SCREEN_WIPE_DOWN);
        break;
    case BOOT_END:
        screen_pop(&v->s, SCREEN_WIPE_DOWN);
        v->s.popup_ask = 0; /* the control that ended it did nothing else */
        break;
    default:
        break;
    }
}

/* A title card on top that is over: the view under it cuts in. */
static void card_frame(views* v) {
    if (screen_top(&v->s) == &title_card_view && title_card_done(&v->card)) {
        screen_pop(&v->s, SCREEN_BANDS);
        v->s.popup_ask = 0; /* the control that ended it did nothing else */
    }
}

/* The view that shows: the one on top, or the one under MIDI learn's frame, which composes it
 * every frame. */
static const view* shows(const views* v) {
    const screen* s = &v->s;
    const view* top = screen_top(s);
    if (top == &learn_view_view && s->depth >= 2u) {
        top = s->stack[s->depth - 2u].v;
    }
    return top;
}

/* What K1..K4 turn (app/panel.h panel.page): the CHIP view's page while it shows, LIVE's
 * anywhere else; and the panel MIDI learn reads it from. Before each event and after the views
 * took it, so the panel, which turns the knob, and learn, which picks it, read the same. */
static void knob_page(views* v, panel* p) {
    v->p = p;
    p->page = (uint8_t)(shows(v) == &chip_view_view ? chip_view_page(&v->chip) : PANEL_PAGE_LIVE);
}

/* The boot or a title card on top: they take every control, SAVE included (it ends them). */
static int curtain(const views* v) {
    const view* top = screen_top(&v->s);
    return top == &boot_view || top == &title_card_view;
}

/* ---- the popup: what a control shows ---- */

/* "63%" for a pulse width (0..4095): LIVE's PW00..PW99, as a percent. */
static void pulse_text(char* buf, uint32_t pw) {
    char num[4];
    number_text(num, (pw * 99u + 2047u) / 4095u);
    join(buf, SCREEN_POPUP_TEXT, num, "%");
}

/* The value of a number control: its text, and the bar. */
static void number_value(char* value, uint32_t n, uint32_t* fill, uint32_t* max, uint32_t top) {
    number_text(value, n);
    *fill = n;
    *max = top;
}

/* A control of the instrument by its parameter (live.h): its name as MIDI learn says it and its
 * value as the views show it (cutoff in Hz as LIVE's filter, the rest as numbers and names).
 * The one place the popup's names and values come from. */
static int param_text(const views* v, uint32_t param, char* name, char* value, uint32_t* fill,
                      uint32_t* max) {
    const live* l = v->l;
    learn_view_param_name(param, name);
    if (param == LIVE_PARAM_CUTOFF) {
        live_view_cutoff_text(l, value);
        *fill = l->cutoff;
        *max = LIVE_CUTOFF_MAX;
    } else if (param == LIVE_PARAM_RESONANCE) {
        number_value(value, l->resonance, fill, max, LIVE_RESONANCE_MAX);
    } else if (param == LIVE_PARAM_PULSE_WIDTH) {
        pulse_text(value, l->pulse_width);
        *fill = l->pulse_width;
        *max = LIVE_PULSE_WIDTH_MAX;
    } else if (param == LIVE_PARAM_VOLUME) {
        join(name, SCREEN_POPUP_TEXT, "VOLUME", "");
        number_value(value, l->volume, fill, max, 15u);
    } else if (param >= LIVE_PARAM_AGE && param <= LIVE_PARAM_BLEED) {
        number_value(value, live_param_get(l, param), fill, max, live_param_max(param));
    } else if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t i = param - LIVE_PARAM_SLOT;
        uint32_t slot = i / LIVE_SLOT_FIELDS;
        uint32_t field = i % LIVE_SLOT_FIELDS;
        char label[CHIP_VIEW_LABEL];
        (void)chip_view_label(slot, l->slot[slot][LIVE_SLOT_ALGORITHM], field, label);
        join(name, SCREEN_POPUP_TEXT, SLOT_NAMES[slot], " ");
        append(name, SCREEN_POPUP_TEXT, label);
        number_value(value, l->slot[slot][field], fill, max, LIVE_SLOT_MAX);
    } else {
        return 0;
    }
    return 1;
}

/* The profile's name, as the CHIP view shows it. */
static const char* profile_name(uint32_t id) {
    const sid_chip_profile* p = sid_chip_profile_get(id);
    return p ? p->name : "?";
}

/* The CHIP view's ALGORITHM: the profile on the chip (the song's on a song's chip A), the
 * algorithm on a slot. The tabs' switch shows nothing of its own. */
static int chip_choice_text(const views* v, char* name, char* value) {
    const chip_view* cv = &v->chip;
    const live* l = v->l;
    if (cv->tabs) {
        return 0;
    }
    if (!cv->chip) {
        uint32_t k = l->fx_select % LIVE_SLOTS;
        join(name, SCREEN_POPUP_TEXT, SLOT_NAMES[k], "");
        join(value, SCREEN_POPUP_TEXT, views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]),
             "");
        return 1;
    }
    join(name, SCREEN_POPUP_TEXT, "PROFILE", "");
    join(value, SCREEN_POPUP_TEXT,
         profile_name(chip_view_song_tab(cv) ? cv->kr->song_want.profile : l->profile_req), "");
    return 1;
}

/* The menu's ALGORITHM on a value item: its name (in full) and value, as the menu shows them. */
static int menu_choice_text(const views* v, char* name, char* value) {
    const menu_item* it = &v->home.items[v->home.cursor];
    if (!it->value) {
        return 0; /* an action opens its view */
    }
    uint32_t i = v->home.cursor;
    join(name, SCREEN_POPUP_TEXT,
         i == VIEWS_MENU_PRESET ? "PRESET" : i == VIEWS_MENU_ROWS ? "TRACKER ROWS" : it->label,
         "");
    char text[MENU_VALUE_CHARS]; /* (the menu's values have room for a longer text) */
    text[0] = 0;
    it->value((void*)v, text);
    text[SCREEN_POPUP_CHARS] = 0;
    join(value, SCREEN_POPUP_TEXT, text, "");
    return 1;
}

/* The popup's source (screen_popup_source): the control the panel changed, as the view that
 * shows has it. 0 under the boot, a title card and MIDI learn's frame, and for what has no
 * value to show. MIDI never asks (docs/reference/controls.md "Value popup"). */
static int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
                      uint32_t* max) {
    const views* v = ctx;
    const live* l = v->l;
    if (curtain(v) || learn_view_is_open(&v->learn)) {
        return 0;
    }
    if (ask & ASK_PARAM) {
        return param_text(v, ask & (ASK_PARAM - 1u), name, value, fill, max);
    }
    uint32_t enc = ask & 7u;
    if (enc == ASK_MASTER) {
        return param_text(v, LIVE_PARAM_VOLUME, name, value, fill, max);
    }
    if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4) {
        uint32_t held = ask & ASK_EDIT ? 1u << PANEL_BTN_EDIT : 0u;
        uint32_t param = panel_knob_param(v->p, l, held, enc);
        if (param != LIVE_PARAM_COUNT) {
            return param_text(v, param, name, value, fill, max);
        }
        if (v->p && v->p->page == PANEL_PAGE_NONE) {
            /* a song's chip A: the CHIP view turns the song's character itself */
            uint32_t k = enc - PANEL_ENC_K1;
            learn_view_param_name(LIVE_PARAM_AGE + k, name);
            number_value(value, v->chip.kr->song_want.controls[k], fill, max,
                         live_param_max(LIVE_PARAM_AGE + k));
            return 1;
        }
        join(name, SCREEN_POPUP_TEXT, "PRESET", ""); /* K4 on LIVE's page */
        const live_preset* p = live_preset_get(l->preset);
        join(value, SCREEN_POPUP_TEXT, p ? p->name : "?", "");
        return 1;
    }
    if (enc == PANEL_ENC_ALGORITHM) {
        const view* top = shows(v);
        if (top == &chip_view_view) {
            return chip_choice_text(v, name, value);
        }
        if (top == &views_menu) {
            return menu_choice_text(v, name, value);
        }
    }
    return 0;
}

/* SAVE's clean release (app/save_key.h): the precedence of views.h's routing. */
static void save(views* v) {
    if (learn_view_is_open(&v->learn)) {
        learn_view_save(&v->learn); /* forget the picked CC, or confirm the clear */
        return;
    }
    if (shows(v) == &chip_view_view) {
        chip_view_save_result r = chip_view_save(&v->chip, v->s.now);
        if (r) { /* a USER slot: its toast now, or when the store commits (store_frame) */
            if (r == CHIP_VIEW_SAVE_MSG) {
                screen_toast(&v->s, v->chip.msg, SCREEN_TOAST_MS);
            }
            return;
        }
    }
    /* A song view saves the song here, to its file (M2c Task 8, not in yet). */
    if (v->set) {
        settings_save_now(v->set, v->s.now); /* its toast when it commits (views_frame) */
    }
}

/* Route SAVE's gesture and consume HOME while SAVE is held. */
__attribute__((noinline, unused)) static uint32_t save_buttons(views* v, uint32_t pressed,
                                                            uint32_t held) {
    uint32_t save_bit = 1u << SAVE_KEY_BUTTON;
    int ends = (pressed & save_bit) && curtain(v);
    if (ends) {
        save_key_buttons(&v->key, save_bit, held | save_bit);
        save_key_spoil(&v->key); /* it ends the boot or the card: no save at its release */
        screen_event e = {SCREEN_PRESS, (uint8_t)SAVE_KEY_BUTTON, 0, held};
        (void)screen_input(&v->s, &e);
    }
    if (save_key_buttons(&v->key, pressed & ~(ends ? save_bit : 0u), held) == SAVE_KEY_SAVE) {
        save(v);
    }
    pressed &= ~save_bit; /* SAVE goes nowhere else */
    /* Read save_key's held bit directly to save input cycles. */
    if (v->key.down && (pressed & (1u << PANEL_BTN_HOME))) {
        pressed &= ~(1u << PANEL_BTN_HOME); /* SAVE held + HOME: never the menu */
        learn_view_ask_clear(&v->learn);
    }
    return pressed;
}

void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held) {
    knob_page(v, p);
    /* Read save_key's held bit directly to save ordinary button-path cycles. */
    if ((pressed | v->key.down) & (1u << SAVE_KEY_BUTTON)) {
        pressed = save_buttons(v, pressed, held);
    }
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
    knob_page(v, p);
    panel_buttons(p, v->l, rest, held);
}

/* The keys the panel holds: sounding (key_note) or quiet. */
static uint32_t panel_owned(const panel* p, uint32_t keys) {
    uint32_t owned = p->quiet;
    for (uint32_t k = 0; keys >> k; k++) {
        if ((keys >> k & 1u) && p->key_note[k] != LIVE_NO_NOTE) {
            owned |= 1u << k;
        }
    }
    return owned;
}

void views_note_keys(views* v, panel* p, uint32_t down, uint32_t pressed) {
    song_view* sv = &v->song;
    /* Most passes: no key, and no jam to let go: the panel's (releases) alone. */
    if (down | pressed | sv->jam_keys) {
        int take = !(p->held >> PANEL_BTN_GLO & 1u) && sv->t && song_view_is(shows(v));
        uint32_t owned = take ? panel_owned(p, down & ~pressed & ~sv->jam_keys) : 0u;
        uint32_t taken = song_view_keys(sv, down, pressed, owned, take);
        if (taken) {
            panel_quiet_keys(p, v->l, taken);
        }
    }
    panel_note_keys(p, v->l, down, pressed);
    if (p->glo) {
        song_view_glo(sv, panel_take_glo(p));
    }
}

void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps) {
    if (steps == 0) {
        return;
    }
    int32_t clamped = steps > INT16_MAX ? INT16_MAX : steps < -INT16_MAX ? -INT16_MAX : steps;
    screen_event e = {SCREEN_TURN, (uint8_t)enc, (int16_t)clamped, p->held};
    knob_page(v, p);
    /* Read save_key's held bit directly to save encoder-path cycles. */
    if (v->key.down) {
        save_key_spoil(&v->key); /* a knob while SAVE is down: its release saves nothing */
        if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 && !learn_view_is_open(&v->learn) &&
            !curtain(v) && v->learn.ml) {
            (void)learn_view_open(&v->learn, &v->s, p, enc, p->held); /* the knob picks */
            return;
        }
    }
    if (enc != PANEL_ENC_SELECT) {
        /* A control changed: the screen asks for its popup at the frame (screen_popup_ask, whose
         * two stores these are, here to cost the input as little as it can). */
        v->s.popup_ask = (uint8_t)(ASK_BASE | enc | ((e.held >> 3) & ASK_EDIT));
        v->s.popup_view = v->s.stack[v->s.depth - 1u].v; /* on this view or none */
        v->s.layers = 1;
    }
    /* What nobody consumed turns the knob on the page set above: an encoder's turn that no view
     * takes leaves the stack, and so the page, as it was. */
    if (!screen_input(&v->s, &e)) {
        panel_encoder(p, v->l, enc, steps);
    }
}

/* What changed since the last frame, as a toast; a chip change also runs the raster. The last
 * of several changes in one frame wins the toast. The chip's is the model asked for: the toast
 * covers the ticks its chip takes to prepare (app/reg_ring.h). */
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
            append(buf, sizeof buf, views_algorithm_name(k, alg));
            changed = 1;
        }
    }
    if ((uint8_t)l->model_req != v->model) {
        join(buf, sizeof buf, "CHIP ", l->model_req == SID_MODEL_8580 ? "8580" : "6581");
        screen_raster(&v->s, VIEWS_RASTER_STEPS);
        changed = 1;
    }
    if (changed) {
        /* A control's own popup (a K knob, ALGORITHM, MASTER) says it: no toast as well. */
        if (!(v->s.popup_ask & 7u)) {
            screen_toast(&v->s, buf, SCREEN_TOAST_MS);
        }
        sound(v, UI_SOUND_CHIME);
    }
    remember(v);
}

/* A USER profile's write: handed to the store, committed, its toast; keeps the frame awake while
 * it has work. */
__attribute__((noinline, unused)) static void users_frame(views* v, uint32_t now_ms) {
    user_profiles_tick(&v->users, now_ms, v->set->force);
    uint32_t slot;
    char buf[24] = "SAVED TO USER n";
    user_profile_result r = user_profiles_take(&v->users, &slot);
    buf[14] = (char)('1' + slot);
    if (r == USER_PROFILE_SAVED_GAPS) {
        append(buf, sizeof buf, " (GAPS)");
    }
    if (r == USER_PROFILE_SAVED || r == USER_PROFILE_SAVED_GAPS) {
        screen_toast(&v->s, buf, SCREEN_TOAST_MS);
        sound(v, UI_SOUND_CHUNK);
    } else if (r == USER_PROFILE_FOREIGN) {
        screen_toast(&v->s, "STORE FOREIGN: NOT SAVED", SCREEN_TOAST_MS);
        sound(v, UI_SOUND_BUZZ);
    }
    if (user_profiles_busy(&v->users)) {
        v->l->settings_dirty |= 2u;
    }
}

/* The settings' debounce and an explicit save's toast; a format's end. */
__attribute__((noinline, unused)) static void store_frame(views* v, uint32_t now_ms) {
    if (!v->set) {
        return;
    }
    settings_tick(v->set, now_ms);
    if (user_profiles_busy(&v->users)) {
        users_frame(v, now_ms);
    }
    /* Peek at settings' result directly to save idle-frame cycles. */
    if (!v->set->result && !v->formatting) {
        return;
    }
    switch (settings_take_result(v->set)) {
    case SETTINGS_SAVED:
        screen_toast(&v->s, "SAVED", SCREEN_TOAST_MS);
        sound(v, UI_SOUND_CHUNK);
        break;
    case SETTINGS_SAVED_GAPS:
        screen_toast(&v->s, "SAVED (GAPS)", SCREEN_TOAST_MS);
        sound(v, UI_SOUND_CHUNK);
        break;
    case SETTINGS_FOREIGN:
        screen_toast(&v->s, "STORE FOREIGN: NOT SAVED", SCREEN_TOAST_MS);
        sound(v, UI_SOUND_BUZZ);
        break;
    default: break;
    }
    if (v->formatting && v->set->st && !store_busy(v->set->st)) {
        store_stats stats;
        store_stats_get(v->set->st, &stats);
        if (!stats.dirty) {
            v->formatting = 0;
            if (v->set->force) {
                v->set->force(0);
            }
            screen_toast(&v->s, "STORE FORMATTED", SCREEN_TOAST_MS);
        }
    }
    if (v->formatting) {
        v->l->settings_dirty |= 2u;
    }
}

void views_prepare(views* v) {
    if (screen_animating(&v->s)) {
        return;
    }
    const view* top = shows(v);
    if (top == &live_view_view) {
        live_view_prepare(&v->live);
    } else if (top == &scene_view_view) {
        scene_view_prepare(&v->scene);
    }
}

int views_idle(views* v) {
    return shows(v) == &scene_view_view ? scene_view_idle(&v->scene) : 0;
}

void views_song(views* v, const song_ctl* sc, const song_event* ev, uint32_t n) {
    scene_view_song(&v->scene, sc, ev, n);
}

void views_frame(views* v, uint32_t now_ms) {
    boot_frame(v, now_ms);
    card_frame(v);
    const view* top = screen_top(&v->s);
    if (top == &boot_view || top == &title_card_view) {
        /* Nothing over the boot or a card (app/boot.h, app/title_card.h), and nothing of what
         * changed meanwhile after them. */
        remember(v);
    } else {
        tell(v);
    }
    learn_view_frame(&v->learn, &v->s, now_ms);
    if (v->l->settings_dirty) {
        store_frame(v, now_ms);
    }
    /* The tap runs while LIVE or SCENE shows: the audio interrupt pays for it only then. */
    top = shows(v);
    song_view_frame(&v->song, song_view_is(top));
    v->live.mutes = (uint8_t)song_view_mutes(&v->song);
    live_view_tap(&v->live, top == &live_view_view);
    scene_view_tap(&v->scene, top == &scene_view_view);
    if (v->sc) {
        v->sc->on = (uint8_t)(top == &live_view_view || top == &scene_view_view);
    }
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
