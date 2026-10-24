// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views.h"

#include <stddef.h>
#include <string.h>

#include "chip_spread.h"
#include "font.h"

/* The status screen's version: right-aligned to end before this column. */
#define VERSION_END_COL 28u
/* STATUS: the parameters' rows (two each from STATUS_PARAM_ROW), the notes', the effects' header and
 * the prompt's; the bars after their number at 2x. LOOK_NOTES is where the notes are in `look`. */
#define STATUS_PARAM_ROW 10u
#define STATUS_NOTES_ROW 19u
#define STATUS_SLOTS_ROW 21u
#define STATUS_READY_ROW 27u
#define STATUS_BAR_COL 18u
#define STATUS_BAR_CELLS 10u
#define LOOK_NOTES 13u
/* The menu: large text over the whole width, 14 characters a line with a cell of margin either
 * side. The title bar in rows 0-2 (its text in rows 1-2: the position, and an arrow up and down
 * for the items out of sight), the items from row MENU_ROW, a line every 3 rows, the help in
 * rows 24-27. */
#define MENU_ROW 4u
#define MENU_HELP_ROW 24u
#define MENU_CHARS ((GRID_COLS - 2u) / 2u)
/* The cells a frame of the menu sends at most (LIVE, SCENE and CHIP hold to 200: 17.5 ms of SPI
 * with their windows). 178, not 190 since SETTINGS (ergonomics review E31): a scroll's third
 * line waits a frame, so a scroll over the shorter menu's action lines stays within 15.1 ms. */
#define MENU_FRAME_CELLS 178u
/* The lines the MIDI page draws in a frame: the compose's cost is its ui_poll's. */
#define MIDI_LINES 2u
/* A control asked for the popup (views_encoder, master_moved): ASK_BASE, its encoder (a K
 * knob, ALGORITHM, ASK_MASTER for the MASTER pot) and ASK_EDIT when EDIT was held. */
#define ASK_BASE 0x20u
#define ASK_EDIT 8u
#define ASK_MASTER 7u
/* A parameter changed, not a control: ASK_PARAM and its parameter (live.h), from MIDI's watcher. */
#define ASK_PARAM 0x40u

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
static void put(grid* g, uint8_t attr, uint32_t row, uint32_t col, const char* s) {
    while (*s && col < GRID_COLS - 1u) {
        grid_put(g, col++, row, (uint8_t)*s++, attr);
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

/* A profile's model and its word into buf (SCREEN_POPUP_TEXT): "6581 CLASSIC", "8580 CLEAN". */
__attribute__((noinline, unused)) static void profile_label(char* buf, uint32_t id) {
    const sid_chip_profile* p = sid_chip_profile_get(id);
    join(buf, SCREEN_POPUP_TEXT, p && p->model == SID_MODEL_8580 ? "8580 " : "6581 ", "");
    append(buf, SCREEN_POPUP_TEXT, sid_chip_profile_word(id));
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

/* A UI sound (app/ui_sound.h), if there are UI sounds: ui_sound_play does nothing with
 * SOUNDS off. */
static void sound(views* v, ui_sound_id id) {
    if (v->snd) {
        ui_sound_play(v->snd, id);
    }
}

/* ---- what the views show ---- */

/* The menu's STORE: its state (STORE_LOOK_*) and, for a store that writes, its free kilobytes
 * (or, while a format runs, the sectors it still erases) in *n. NEWER: a foreign store that is a
 * newer SIDOU's (store.h mount_newer), told apart because updating, not formatting, keeps it. */
enum {
    STORE_LOOK_NONE, STORE_LOOK_FREE, STORE_LOOK_FOREIGN, STORE_LOOK_NEWER, STORE_LOOK_ARMED,
    STORE_LOOK_ERASING
};
static uint32_t store_look(const views* v, uint32_t* n) {
    const store* st = v->set ? v->set->st : NULL;
    if (!st) {
        return STORE_LOOK_NONE;
    }
    store_state_id state = store_state(st);
    if (state == STORE_STATE_FOREIGN) {
        return v->format_arm == 1u ? STORE_LOOK_ARMED : st->mount_newer ? STORE_LOOK_NEWER
                                                                   : STORE_LOOK_FOREIGN;
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
static void look(const views* v, uint32_t blink, uint8_t out[VIEWS_LOOK]) {
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
    views* v = state;
    return looks_changed(v, 0) || sid_view_status_look(&v->sids, v->tune_drawn, 0);
}

/* The notes held, on the NOTES row from column 9. */
static void status_notes(grid* g, uint8_t attr, const live* l) {
    grid_fill(g, 9, STATUS_NOTES_ROW, GRID_COLS - 10u, 1, ' ', attr);
    uint32_t col = 9;
    for (int k = 0; k < LIVE_VOICES; k++) {
        uint8_t n = l->voice[k].note;
        if (!l->voice[k].gate || n == LIVE_NO_NOTE) {
            continue;
        }
        const char* name = NOTE_NAMES[n % 12u];
        put(g, attr, STATUS_NOTES_ROW, col, name);
        uint32_t oc = col + (name[1] ? 2u : 1u);
        if (n < 12u) {
            put(g, attr, STATUS_NOTES_ROW, oc, "-1");
        } else {
            widget_number(g, oc, STATUS_NOTES_ROW, 1, n / 12 - 1, 0, attr);
        }
        col += 5u;
    }
}

/* One parameter of the sound, two rows: its name small, its number at 2x, its bar. */
static void status_param(grid* g, const theme* th, uint32_t row, const char* label, uint32_t value,
                         uint32_t max, uint8_t color) {
    char num[6];
    number_text(num, value);
    put(g, THEME_ATTR(th, label), row + 1u, 2, label);
    grid_text_big(g, 9, row, 2, num, THEME_ATTR(th, value));
    widget_bar(g, STATUS_BAR_COL, row + 1u, STATUS_BAR_CELLS, value, max, GRID_ATTR(color, th->dim));
}

static void status_compose(screen* s, void* state, grid* g) {
    views* v = state;
    const live* l = v->l;
    const theme* th = theme_get(l->theme);
    const uint8_t a = THEME_ATTR(th, value);
    const uint8_t lbl = THEME_ATTR(th, label);
    /* Only the notes changed: their row alone (a held key costs a row, not the screen). */
    uint8_t now[VIEWS_LOOK];
    look(v, 0, now);
    uint32_t other = 0; /* something but the notes changed */
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        other |= (uint32_t)(now[i] != v->drawn[i]) & (uint32_t)!(i >= LOOK_NOTES && i < LOOK_NOTES + LIVE_VOICES);
    }
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        v->drawn[i] = now[i];
    }
    /* A .sid tune loaded: the 6502's rows in the parameters' place (their numbers alone when only
     * they moved; the screen when the tune comes or goes). */
    int tune_was = v->tune_drawn[0] != SID_T_NONE;
    int tune_moved = sid_view_status_look(&v->sids, v->tune_drawn, 1);
    int tune = v->tune_drawn[0] != SID_T_NONE;
    if (!other && !s->redraw && tune == tune_was) {
        status_notes(g, a, l);
        if (tune_moved) {
            sid_view_status_draw(&v->sids, g, th, STATUS_PARAM_ROW);
        }
        return;
    }
    /* The ring of full blocks in the dim colour, and the blank every view shares inside it: each
     * cell written once (widget_screen's way), so an unchanged compose marks no row. */
    const uint8_t ring = THEME_ATTR(th, dim);
    grid_fill(g, 0, 0, GRID_COLS, 1, FONT_FULL, ring);
    grid_fill(g, 0, GRID_ROWS - 1u, GRID_COLS, 1, FONT_FULL, ring);
    grid_fill(g, 0, 1, 1, GRID_ROWS - 2u, FONT_FULL, ring);
    grid_fill(g, GRID_COLS - 1u, 1, 1, GRID_ROWS - 2u, FONT_FULL, ring);
    grid_fill(g, 1, 1, GRID_COLS - 2u, GRID_ROWS - 2u, ' ', a);
    grid_text_big(g, 2, 2, 2, "SIDOU", THEME_ATTR(th, hot));
    put(g, lbl, 2, VERSION_END_COL - length(v->version), v->version);

    uint32_t top = 5;
    if (l->two_chips) {
        /* Both chips' profiles (their model in the name); KEYS marks the one the keys play
         * (app/keys_route.h): the keys' setup as asked for, the other's as it has it. */
        for (uint32_t x = 0; x < 2u; x++, top++) {
            uint32_t id = x == l->keys_chip ? l->profile_req : l->other_profile;
            char name[SCREEN_POPUP_TEXT] = "--"; /* not made yet */
            if (sid_chip_profile_get(id)) {
                profile_label(name, id);
            }
            put(g, lbl, top, 2, x ? "CHIP B" : "CHIP A");
            put(g, a, top, 9, name);
            if (x == l->keys_chip) {
                put(g, THEME_ATTR(th, hot), top, 22, "KEYS");
            }
        }
    } else {
        char name[SCREEN_POPUP_TEXT];
        profile_label(name, l->profile_req);
        put(g, lbl, 5, 2, "CHIP");
        put(g, a, 5, 9, name);
        top = 6;
    }
    put(g, lbl, top, 2, "PRESET");
    const live_preset* p = live_preset_get(l->preset);
    put(g, a, top, 9, p ? p->name : "?");
    put(g, lbl, top + 1u, 2, "OCTAVE");
    widget_number(g, 9, top + 1u, 2, l->octave, 1, a);

    if (tune) {
        sid_view_status_draw(&v->sids, g, th, STATUS_PARAM_ROW);
    } else {
        status_param(g, th, STATUS_PARAM_ROW, "CUTOFF", l->cutoff, LIVE_CUTOFF_MAX, th->knob[0]);
        status_param(g, th, STATUS_PARAM_ROW + 2u, "RESO", l->resonance, LIVE_RESONANCE_MAX,
                     th->knob[1]);
        status_param(g, th, STATUS_PARAM_ROW + 4u, "PULSE", l->pulse_width, LIVE_PULSE_WIDTH_MAX,
                     th->knob[2]);
        status_param(g, th, STATUS_PARAM_ROW + 6u, "VOLUME", l->volume, 100, th->value);
    }

    put(g, lbl, STATUS_NOTES_ROW, 2, "NOTES");
    status_notes(g, a, l);

    /* The effect slots: '>' marks the one EDIT + K1-K4 and ENV act on (SCL moves it). */
    put(g, lbl, STATUS_SLOTS_ROW, 2, "EFFECTS");
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint32_t row = STATUS_SLOTS_ROW + 1u + k;
        if (k == l->fx_select) {
            put(g, THEME_ATTR(th, hot), row, 2, ">");
        }
        put(g, lbl, row, 3, SLOT_NAMES[k]);
        put(g, a, row, 8, views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]));
        put(g, lbl, row, 19, "MIX");
        widget_number(g, 23, row, 3, l->slot[k][LIVE_SLOT_MIX], 0, a);
    }
    const uint8_t* sel = l->slot[l->fx_select];
    uint32_t arow = STATUS_SLOTS_ROW + 1u + LIVE_SLOTS;
    put(g, lbl, arow, 3, "AMT");
    widget_number(g, 7, arow, 3, sel[LIVE_SLOT_AMOUNT], 0, a);
    put(g, lbl, arow, 12, "A");
    widget_number(g, 14, arow, 3, sel[LIVE_SLOT_A], 0, a);
    put(g, lbl, arow, 19, "B");
    widget_number(g, 21, arow, 3, sel[LIVE_SLOT_B], 0, a);

    put(g, lbl, STATUS_READY_ROW, 2, "READY.");
}

const view views_status = {"STATUS", NULL, NULL, NULL, status_compose, status_changed};

/* ---- MENU ---- */

static void preset_value(void* ctx, char* buf) {
    const views* v = ctx;
    char name[LIVE_PRESET_NAME_CHARS];
    live_preset_name(v->l, name);
    join(buf, MENU_VALUE_CHARS, name, "");
}

static void preset_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    live_preset_step(v->l, steps, 0);
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
    static const char* const NAME[3] = {"OFF", "ONCE", "ALWAYS"};
    join(buf, MENU_VALUE_CHARS, NAME[v->cards < 3u ? v->cards : 1u], "");
}

/* Clockwise steps OFF, ONCE, ALWAYS, counter-clockwise back; clamped at the ends. */
static void cards_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    int32_t to = (int32_t)v->cards + steps;
    to = to < 0 ? 0 : (to > 2 ? 2 : to);
    if (to != (int32_t)v->cards) {
        v->cards = (uint8_t)to;
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
    case STORE_LOOK_NEWER: join(buf, MENU_VALUE_CHARS, "NEWER", ""); break;
    case STORE_LOOK_ARMED: join(buf, MENU_VALUE_CHARS, "FORMAT?", ""); break;
    default: join(buf, MENU_VALUE_CHARS, "NONE", ""); break;
    }
}

/* On a foreign store: clockwise arms the format, counter-clockwise disarms; more clockwise
 * turns keep it armed and no more. Only SAVE then formats (store_confirm): a fast spin of
 * ALGORITHM, however many detents, never erases anything. Else nothing. */
static void store_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    store* st = v->set ? v->set->st : NULL;
    if (!st || store_state(st) != STORE_STATE_FOREIGN || steps == 0) {
        v->format_arm = 0;
        return;
    }
    v->format_arm = steps > 0;
}

static void reset_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->format_arm == 2u ? "RESET?" : "", "");
}

/* RESET asks the same two steps as a format (RESET ALL SOUNDS AND SETTINGS?): clockwise arms,
 * counter-clockwise disarms, and only SAVE resets (reset_confirm). */
static void reset_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    v->format_arm = steps > 0 ? 2u : 0u;
}

/* SAVE on SETTINGS' armed RESET: every preset's edits and the settings back to factory, saved
 * now (the toast follows the save). Songs and USER profiles are other store keys: untouched.
 * 0, nothing done, when RESET is not armed under the cursor. */
static int reset_confirm(views* v) {
    if (v->format_arm != 2u || v->setup.cursor != VIEWS_SETTINGS_RESET || !v->set) {
        return 0;
    }
    v->format_arm = 0;
    settings_reset(v->set);
    v->resetting = 1;
    settings_save_now(v->set, v->s.now);
    return 1;
}

/* SAVE on SETTINGS' armed STORE: the format (every sector erased, the erases forced: the user
 * asked for their gaps). 0, nothing done, when STORE is not armed under the cursor. */
static int store_confirm(views* v) {
    store* st = v->set ? v->set->st : NULL;
    if (v->format_arm != 1u || v->setup.cursor != VIEWS_SETTINGS_STORE || !st ||
        store_state(st) != STORE_STATE_FOREIGN) {
        return 0;
    }
    v->format_arm = 0;
    store_format(st);
    settings_forget(v->set);
    user_profiles_forget(&v->users);
    v->formatting = 1;
    if (v->set->force) {
        v->set->force(1);
    }
    return 1;
}

/* Title card id over the view on top, as the CARDS option says: ALWAYS every one, ONCE THE CHIP
 * and THE SCENE the first time each since power-on and never THE OSCILLATOR, OFF none. From the
 * views' input only (a push from the frame would draw a frame of the view first). */
static void show_card(views* v, title_card_id id) {
    uint8_t seen = (uint8_t)(1u << id);
    if (v->cards == 1u) {
        if (id == TITLE_CARD_OSCILLATOR || (v->cards_seen & seen)) {
            return;
        }
        v->cards_seen |= seen;
    }
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

static void quant_value(void* ctx, char* buf) {
    const views* v = ctx;
    uint8_t q = v->song.quant;
    join(buf, MENU_VALUE_CHARS, q == SONG_VIEW_QUANT_OFF ? "OFF" : q == SONG_VIEW_QUANT_COUNT ? "COUNT-IN" : "NEAREST", "");
}

/* REC's quantising: clockwise NEAREST, then OFF, then COUNT-IN (the nearest row, a bar first);
 * counter-clockwise back. Not saved: it is a take's setting. */
static void quant_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        int32_t q = (int32_t)v->song.quant + (steps > 0 ? 1 : -1);
        v->song.quant = (uint8_t)(q < 0 ? 0 : q >= (int32_t)SONG_VIEW_QUANTS ? (int32_t)SONG_VIEW_QUANTS - 1 : q);
    }
}

static void theme_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, theme_get(v->l->theme)->menu_name, "");
}

/* Clockwise C64, counter-clockwise HIGH CONTRAST. Every view draws again in the new colours. */
static void theme_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        live_set_theme(v->l, steps > 0 ? THEME_C64 : THEME_HIGH_CONTRAST);
        screen_redraw(&v->s);
    }
}

/* SPREAD (app/chip_spread.h): OFF, then 10% .. 100%. */
static void spread_value(void* ctx, char* buf) {
    const views* v = ctx;
    char num[4];
    number_text(num, v->l->spread);
    join(buf, MENU_VALUE_CHARS, v->l->spread ? num : "OFF", v->l->spread ? "%" : "");
}

/* In steps of 10 % (a value MIDI set between them goes to the next step), clamped at both ends. */
static void spread_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    int32_t s = ((int32_t)v->l->spread + (steps > 0 ? 0 : 9)) / 10 + (steps > 0 ? 1 : -1);
    if (steps != 0) {
        chip_spread_set(v->l, (uint32_t)(s < 0 ? 0 : s > 10 ? 10 : s) * 10u);
    }
}

/* SID MODEL (app/settings.h): AUTO, a tune's chips as its header names them; KEYS, the keys'
 * setup for every tune. */
static void sid_model_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, !v->set ? "-" : v->set->sid_model ? "KEYS" : "AUTO", "");
}

/* Clockwise KEYS, counter-clockwise AUTO; from the next tune that loads. */
static void sid_model_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    uint8_t to = (uint8_t)(steps > 0 ? SETTINGS_SID_KEYS : SETTINGS_SID_AUTO);
    if (steps != 0 && v->set && v->set->sid_model != to) {
        v->set->sid_model = to;
        v->l->settings_dirty = 1;
    }
}

static const view* shows(const views* v);

/* While ABOUT's tune is the player's song (song_io about, until its close has read the user's
 * song back), the song views would edit ABOUT's tune and its close would lose the edits: they
 * do not open, and a toast says why. 1: refused. */
static int song_refused(views* v) {
    uint32_t tune = sid_view_state(&v->sids);
    if (v->tune || tune != SID_T_NONE) {
        if (tune == SID_T_STOPPED || tune == SID_T_FAILED || tune == SID_T_ENDING) {
            /* a stopped tune gives way: the song comes back for the next press */
            (void)v->sids.t->end(v->sids.t->ctx);
            screen_toast(&v->s, "LOADING", SCREEN_TOAST_MS);
        } else {
            screen_toast(&v->s, "SID PLAYING: STOP TO EDIT", SCREEN_TOAST_MS);
        }
        return 1;
    }
    if (!v->song.io || !v->song.io->about) {
        return 0;
    }
    screen_toast(&v->s, screen_top(&v->s) == &about_view ? "LEAVE ABOUT FIRST" : "LOADING",
                 SCREEN_TOAST_MS);
    return 1;
}

static void tracker_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    if (song_refused(v)) {
        return;
    }
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

/* ---- SONG: its pages borrow the song views' canvas ---- */

/* The state a song view runs on: TRACKER the song view's, STEP its own, the pages theirs. */
static void* song_state(views* v, const view* w) {
    return w == &tracker_view_view ? (void*)&v->song : w == &step_view_view ? (void*)&v->step :
           (void*)&v->pages;
}

enum { SONG_MENU_ROOT, SONG_MENU_DIR, SONG_MENU_NAME, SONG_MENU_CONFIRM };
enum { SONG_MENU_LOAD, SONG_MENU_NEW, SONG_MENU_SAVE, SONG_MENU_DELETE };
#define SONG_MENU_CANVAS SONG_VIEW_CANVAS_MENU

static const char* const SONG_ACTIONS[] = {"LOAD", "NEW", "SAVE AS", "DELETE"};
static int song_menu_input(void* ctx, const screen_event* e);
static int global(void* ctx, const screen_event* e);
/* SONG's HOME handling survives LEARN's capture without work in ordinary input routing. */
static int song_menu_global(void* ctx, const screen_event* e) {
    return song_menu_input(ctx, e) || global(ctx, e);
}

static void song_menu_changed(views* v) {
    v->songs.built = 0;
}

/* LOAD's first entry: the built-in song (song_io's hooks.builtin, SIDOU LOOP in the image), its
 * name and a dot that marks it built-in. No stored name has the dot (they are 0x20..0x5F): the
 * entry chosen is told by its text. */
static const char SONG_BUILTIN[] = {'S', 'I', 'D', 'O', 'U', ' ', 'L', 'O', 'O', 'P', ' ',
                                    (char)FONT_MIDDOT, 0};
#define SONG_BUILTIN_NAME "SIDOU LOOP"

/* LOAD lists the built-in song (when there is one) before the store's files. */
static int song_builtin_listed(const views* v) {
    return v->songs.action == SONG_MENU_LOAD && v->song.io && v->song.io->hooks.builtin;
}

static void song_directory(views* v, uint8_t page) {
    views_song_menu* m = &v->songs;
    uint32_t builtin = (uint32_t)song_builtin_listed(v);
    m->page = SONG_MENU_DIR;
    m->directory_page = page;
    m->cursor = 0;
    m->count = 0;
    m->after[0] = 0;
    /* the built-in is the first page's first entry: the files after it move on by one */
    m->skip = (uint8_t)(page * VIEWS_SONG_FILES - (page ? builtin : 0u));
    if (builtin && !page) {
        join(m->files[m->count++], sizeof m->files[0], SONG_BUILTIN, "");
    }
    m->scanning = 1;
    song_menu_changed(v);
}

static void song_name(views* v, int direct) {
    views_song_menu* m = &v->songs;
    /* Unnamed: the built-in song while it is unchanged since the boot or its LOAD (NEW and a
     * lost file mark the song changed), else a new one. */
    const song_io* io = v->song.io;
    const char* name = io && io->name[0] ? io->name
                       : io && io->hooks.builtin && !io->dirty ? SONG_BUILTIN_NAME
                                                               : "NEW SONG";
    uint32_t n = 0;
    while (name[n] && n < STORE_NAME_MAX) {
        m->name[n] = name[n];
        n++;
    }
    while (n < STORE_NAME_MAX) {
        m->name[n++] = ' ';
    }
    m->name[STORE_NAME_MAX] = 0;
    m->page = SONG_MENU_NAME;
    m->action = SONG_MENU_SAVE;
    m->cursor = 0;
    m->direct = (uint8_t)direct;
    m->return_view = NULL;
    m->scanning = 0;
    song_menu_changed(v);
}

static void song_menu_done(views* v) {
    uint8_t direct = v->songs.direct;
    if (direct) {
        const view* back = v->songs.return_view;
        screen_pop(&v->s, SCREEN_WIPE_UP);
        v->songs.direct = 0;
        v->songs.page = SONG_MENU_ROOT;
        v->songs.cursor = SONG_MENU_SAVE;
        v->songs.scanning = 0;
        song_menu_changed(v);
        if (direct == 2u) {
            screen_push(&v->s, back, song_state(v, back), SCREEN_WIPE_UP);
        }
    } else {
        v->songs.page = SONG_MENU_ROOT;
        v->songs.cursor = v->songs.action;
        v->songs.scanning = 0;
        song_menu_changed(v);
    }
}

static void song_menu_execute(views* v, int confirmed) {
    views_song_menu* m = &v->songs;
    song_io* io = v->song.io;
    if (!io) {
        screen_toast(&v->s, "NO STORE", SCREEN_TOAST_MS);
        return;
    }
    if (song_io_busy(io)) {
        screen_toast(&v->s, song_io_saving(io) ? "SAVING" : "SONG BUSY", SCREEN_TOAST_MS);
        return;
    }
    if (!confirmed && ((song_io_changed(io) && (m->action == SONG_MENU_LOAD || m->action == SONG_MENU_NEW)) ||
                       m->action == SONG_MENU_DELETE)) {
        m->page = SONG_MENU_CONFIRM;
        m->confirm = 0;
        song_menu_changed(v);
        return;
    }
    int accepted;
    switch (m->action) {
    case SONG_MENU_LOAD:
        accepted = strcmp(m->name, SONG_BUILTIN) == 0 ? song_io_load_builtin(io, confirmed)
                                                      : song_io_load(io, m->name, confirmed);
        break;
    case SONG_MENU_NEW: accepted = song_io_new(io, confirmed); break;
    case SONG_MENU_SAVE: accepted = song_io_save(io, m->name, v->s.now); break;
    case SONG_MENU_DELETE: accepted = song_io_delete(io, m->name, confirmed); break;
    default: return;
    }
    if (accepted) {
        song_menu_done(v);
    }
}

static void song_menu_save_name(views* v) {
    views_song_menu* m = &v->songs;
    uint32_t n = STORE_NAME_MAX;
    while (n && m->name[n - 1u] == ' ') {
        n--;
    }
    m->name[n] = 0;
    if (!n || m->name[0] == ' ') {
        screen_toast(&v->s, "NAME NEEDS FIRST CHARACTER", SCREEN_TOAST_MS);
        while (n < STORE_NAME_MAX) {
            m->name[n++] = ' ';
        }
        return;
    }
    song_menu_execute(v, 0);
    if (screen_top(&v->s) == &views_song_menu_view && m->page == SONG_MENU_NAME) {
        while (n < STORE_NAME_MAX) {
            m->name[n++] = ' ';
        }
    }
}

static int32_t song_clamp(int32_t n, int32_t max) {
    return n < 0 ? 0 : n > max ? max : n;
}

/* HOME let go on one of SONG's pages (home_go): back a page, or out of a SAVE AS opened from a
 * song view. 0 on SONG's actions: HOME goes back a view as anywhere. HOME acts on its release
 * here as everywhere, so that a hold opens the page's help card. */
static int song_menu_back(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_NAME && m->direct) {
        song_menu_done(v);
        return 1;
    }
    if (m->page == SONG_MENU_ROOT) {
        return 0;
    }
    if (m->page == SONG_MENU_CONFIRM && m->action != SONG_MENU_NEW) {
        m->page = m->action == SONG_MENU_SAVE ? SONG_MENU_NAME : SONG_MENU_DIR;
    } else {
        m->page = SONG_MENU_ROOT;
        m->cursor = m->action;
        m->scanning = 0;
    }
    song_menu_changed(v);
    return 1;
}

/* The highlighted item opened or chosen (ALGORITHM clockwise, or SAVE: song_menu_ok): an action
 * its page, a file its load or delete, a confirmation's YES the action, SAVE AS's SAVE the name. */
static void song_menu_open(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_ROOT) {
        m->action = m->cursor;
        if (m->action == SONG_MENU_LOAD || m->action == SONG_MENU_DELETE) {
            song_directory(v, 0);
        } else if (m->action == SONG_MENU_SAVE) {
            song_name(v, 0);
        } else {
            song_menu_execute(v, 0);
        }
    } else if (m->page == SONG_MENU_DIR && !m->scanning && m->count) {
        join(m->name, sizeof m->name, m->files[m->cursor], "");
        song_menu_execute(v, 0);
    } else if (m->page == SONG_MENU_NAME) {
        song_menu_save_name(v);
    } else if (m->page == SONG_MENU_CONFIRM && m->confirm) {
        song_menu_execute(v, 1);
    }
}

static int song_menu_input(void* ctx, const screen_event* e) {
    views* v = ctx;
    views_song_menu* m = &v->songs;
    if (screen_top(&v->s) != &views_song_menu_view) {
        return 0;
    }
    if (e->kind != SCREEN_TURN ||
        (e->id != PANEL_ENC_SELECT && e->id != PANEL_ENC_ALGORITHM)) {
        return 0;
    }
    int32_t steps = e->steps;
    if (e->id == PANEL_ENC_SELECT) {
        if (m->page == SONG_MENU_CONFIRM) {
            m->confirm = (uint8_t)song_clamp((int32_t)m->confirm + steps, 1);
        } else if (m->page == SONG_MENU_DIR) {
            int32_t next = (int32_t)m->cursor + steps;
            if (!m->scanning && next >= (int32_t)VIEWS_SONG_FILES && m->count == VIEWS_SONG_FILES &&
                (uint32_t)(m->directory_page + 1u) * VIEWS_SONG_FILES < STORE_KEYS_MAX) {
                song_directory(v, (uint8_t)(m->directory_page + 1u));
            } else if (next < 0 && m->directory_page) {
                song_directory(v, (uint8_t)(m->directory_page - 1u));
            } else {
                m->cursor = (uint8_t)song_clamp(next, m->count ? m->count - 1 : 0);
            }
        } else {
            m->cursor = (uint8_t)song_clamp((int32_t)m->cursor + steps,
                                           m->page == SONG_MENU_NAME ? STORE_NAME_MAX : 3u);
        }
    } else if (m->page == SONG_MENU_NAME && m->cursor < STORE_NAME_MAX) {
        uint32_t c = m->cursor;
        int32_t lo = c == 0 ? '!' : ' ';
        int32_t range = 0x60 - lo;
        int32_t ch = ((int32_t)m->name[c] - lo + steps) % range;
        m->name[c] = (char)(lo + (ch < 0 ? ch + range : ch));
    } else if (steps > 0) {
        song_menu_open(v);
    }
    song_menu_changed(v);
    return 1;
}

/* SAVE on one of SONG's pages (save): the page's OK, as ALGORITHM clockwise (song_menu_open)
 * but for SAVE AS, where it writes the name wherever the cursor is, and a confirmation's NO,
 * which it chooses: back a page, as HOME. */
static void song_menu_ok(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_NAME) {
        song_menu_save_name(v);
    } else if (m->page == SONG_MENU_CONFIRM && !m->confirm) {
        (void)song_menu_back(v);
    } else {
        song_menu_open(v);
    }
    song_menu_changed(v);
}

static void song_menu_enter(screen* s, void* state) {
    views* v = state;
    song_menu_changed(v);
    screen_global(s, song_menu_global, v);
}

static void song_menu_exit(screen* s, void* state) {
    views* v = state;
    v->song.built = 0;
    screen_global(s, global, v);
    for (uint32_t i = 0; i + 1u < s->depth; i++) {
        if (s->stack[i].v == &views_song_menu_view) {
            screen_global(s, song_menu_global, v);
            break;
        }
    }
}

static void song_directory_step(views* v) {
    views_song_menu* m = &v->songs;
    store_file file;
    memset(&file, 0, sizeof file);
    join(file.name, sizeof file.name, m->after, "");
    if (!v->song.io || !v->song.io->st || !store_dir_next(v->song.io->st, &file)) {
        m->scanning = 0;
        return;
    }
    join(m->after, sizeof m->after, file.name, "");
    if (!song_io_listed(&file)) {
        return;
    }
    if (m->skip) {
        m->skip--;
    } else {
        join(m->files[m->count++], sizeof m->files[0], file.name, "");
        if (m->count == VIEWS_SONG_FILES) {
            m->scanning = 0;
        }
    }
}

static void song_canvas_text(views* v, uint32_t row, uint32_t col, const char* text, uint8_t a) {
    while (*text && col < GRID_COLS) {
        v->song.cv.ch[row][col] = (uint8_t)*text++;
        v->song.cv.attr[row][col++] = a;
    }
}

static void song_canvas_big(views* v, uint32_t row, uint32_t col, const char* text, uint8_t a) {
    while (*text && col + 1u < GRID_COLS) {
        uint8_t ch = (uint8_t)*text++;
        for (uint32_t y = 0; y < 2u; y++) {
            for (uint32_t x = 0; x < 2u; x++) {
                v->song.cv.ch[row + y][col + x] = ch;
                v->song.cv.attr[row + y][col + x] = a;
            }
        }
        col += 2u;
    }
}

/* A file's name at 2x from column 2 (a big character starts on an even column), padded to the
 * 14 characters the row holds so that the selection is a whole bar; a longer name's 14th is an
 * arrow. */
#define SONG_NAME_CELLS 14u
static void song_canvas_name(views* v, uint32_t row, const char* name, uint8_t a) {
    char line[SONG_NAME_CELLS + 1u];
    uint32_t n = 0;
    while (n < SONG_NAME_CELLS && name[n]) {
        line[n] = name[n];
        n++;
    }
    if (n == SONG_NAME_CELLS && name[n]) {
        line[n - 1u] = (char)FONT_ARROW_RIGHT;
    }
    while (n < SONG_NAME_CELLS) {
        line[n++] = ' ';
    }
    line[n] = 0;
    song_canvas_big(v, row, 2, line, a);
}

static uint32_t song_menu_big_rows(const views_song_menu* m) {
    uint32_t rows = 3u << 2; /* heading */
    rows |= 3u << 4; /* busy status, when present */
    if (m->page == SONG_MENU_ROOT) {
        for (uint32_t i = 0; i < 4u; i++) {
            rows |= 3u << (6u + i * 4u);
        }
    } else if (m->page == SONG_MENU_NAME) {
        rows |= (3u << 8) | (3u << 12) | (3u << 16) | (3u << 18) | (3u << 20) | (3u << 22);
    } else if (m->page == SONG_MENU_DIR) {
        rows |= ((1u << (2u * VIEWS_SONG_FILES + 2u)) - 1u) << 6; /* the files, the status */
    } else if (m->page == SONG_MENU_CONFIRM) {
        rows |= (3u << 6) | (3u << 8) | (3u << 10) | (3u << 14);
    }
    if (m->page != SONG_MENU_NAME) {
        rows |= 3u << 24; /* the legend */
    }
    rows |= 3u << 26; /* HOME BACK */
    return rows;
}

static uint8_t song_menu_io_state(const views* v) {
    return v->song.io ? v->song.io->state : 0;
}

static int song_menu_dirty(screen* s, void* state) {
    (void)s;
    const views* v = state;
    return !v->songs.built || v->songs.scanning || v->song.dirty ||
           v->song.which != SONG_MENU_CANVAS || v->songs.io_state != song_menu_io_state(v) ||
           v->songs.drawn_theme != v->l->theme;
}

static void song_menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    views_song_menu* m = &v->songs;
    if (song_view_acquire_canvas(&v->song, SONG_MENU_CANVAS)) {
        m->built = 0;
    }
    const theme* th = theme_get(v->l->theme);
    const uint8_t text = THEME_ATTR(th, menu_item);
    const uint8_t label = THEME_ATTR(th, label);
    const uint8_t legend = THEME_ATTR(th, hint);
    const uint8_t value = THEME_ATTR(th, value);
    const uint8_t hot = THEME_ATTR(th, hot);
    const uint8_t alert = THEME_ATTR(th, alert);
    const uint8_t selected = GRID_ATTR(th->sel_fg, th->sel_bg);
    if (m->scanning && m->page == SONG_MENU_DIR) {
        song_directory_step(v);
        m->built = 0;
    }
    if (!m->built || v->song.which != SONG_MENU_CANVAS || m->io_state != song_menu_io_state(v) ||
        m->drawn_theme != v->l->theme) {
        for (uint32_t r = 0; r < GRID_ROWS; r++) {
            for (uint32_t c = 0; c < GRID_COLS; c++) {
                v->song.cv.ch[r][c] = ' ';
                v->song.cv.attr[r][c] = text;
            }
        }
        song_canvas_big(v, 2, 2, m->page == SONG_MENU_ROOT ? "SONG" : SONG_ACTIONS[m->action],
                        GRID_ATTR(th->title_fg, th->title_bg));
        if (v->song.io && song_io_busy(v->song.io)) {
            song_canvas_big(v, 4, 2, song_io_saving(v->song.io) ? "SAVING" : "BUSY", hot);
        }
        if (m->page == SONG_MENU_ROOT) {
            for (uint32_t i = 0; i < 4u; i++) {
                song_canvas_big(v, 6u + i * 4u, 2, SONG_ACTIONS[i], i == m->cursor ? selected : text);
            }
        } else if (m->page == SONG_MENU_DIR) {
            /* The files at 2x, as the menu's lines; under them, at 2x too, what the scan is doing,
             * or the page and the arrows SELECT pages with (another page may follow a full one). */
            for (uint32_t i = 0; i < m->count; i++) {
                song_canvas_name(v, 6u + i * 2u, m->files[i], i == m->cursor ? selected : text);
            }
            char page[] = {'P', 'A', 'G', 'E', ' ', (char)('1' + m->directory_page), ' ',
                           m->directory_page ? (char)FONT_ARROW_UP : ' ',
                           m->count == VIEWS_SONG_FILES ? (char)FONT_ARROW_DOWN : ' ', 0};
            song_canvas_big(v, 22, 2,
                            m->scanning ? "READING" : !m->count ? "NO SONGS"
                            : m->directory_page || m->count == VIEWS_SONG_FILES ? page : "",
                            label);
        } else if (m->page == SONG_MENU_NAME) {
            char half[9];
            for (uint32_t i = 0; i < 2u; i++) {
                memcpy(half, &m->name[i * 8u], 8);
                half[8] = 0;
                song_canvas_big(v, 8u + i * 4u, 2, half, value);
            }
            if (m->cursor < STORE_NAME_MAX) {
                song_canvas_text(v, 10u + m->cursor / 8u * 4u, 2u + m->cursor % 8u * 2u, "^^", hot);
            }
            song_canvas_big(v, 16, 2, "SAVE", m->cursor == STORE_NAME_MAX ? selected : text);
            /* The key legend at 2x, as the menu's (the hint role); ALG is ALGORITHM. */
            song_canvas_big(v, 18, 2, "SELECT CURSOR", legend);
            song_canvas_big(v, 20, 2, "ALG CHARACTER", legend);
            song_canvas_big(v, 22, 2, "SAVE WRITE", legend);
        } else {
            song_canvas_big(v, 6, 2, m->action == SONG_MENU_DELETE ? "DELETE SONG?" : "DISCARD", alert);
            if (m->action != SONG_MENU_DELETE) {
                song_canvas_big(v, 8, 2, "UNSAVED SONG?", alert);
            }
            if (m->action != SONG_MENU_NEW) {
                song_canvas_name(v, 10, m->name, value);
            }
            song_canvas_big(v, 14, 2, "NO", m->confirm ? text : selected);
            song_canvas_big(v, 14, 16, "YES", m->confirm ? selected : text);
        }
        if (m->page != SONG_MENU_NAME) {
            /* SAVE is the pages' OK, as the menu's (ALGORITHM clockwise does the same) */
            song_canvas_big(v, 24, 2, m->page == SONG_MENU_CONFIRM ? "SAVE CONFIRM"
                                      : m->page == SONG_MENU_DIR   ? "SAVE CHOOSE"
                                                                   : "SAVE OPEN",
                            legend);
        }
        song_canvas_big(v, 26, 2, "HOME BACK", legend);
        v->song.dirty = (1u << GRID_ROWS) - 1u;
        m->built = 1;
        m->io_state = song_menu_io_state(v);
        m->drawn_theme = v->l->theme;
    }
    song_view_copy_large(&v->song, s, g, song_menu_big_rows(m));
}

const view views_song_menu_view = {"SONG", song_menu_enter, song_menu_exit, NULL,
                                   song_menu_compose, song_menu_dirty};

static void song_open(void* ctx, int32_t steps) {
    (void)steps;
    views* v = ctx;
    if (v->tune) {
        (void)song_refused(v);
        return;
    }
    v->songs.page = SONG_MENU_ROOT;
    v->songs.cursor = 0;
    v->songs.direct = 0;
    v->songs.scanning = 0;
    screen_push(&v->s, &views_song_menu_view, v, SCREEN_WIPE_DOWN);
}

static void about_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    if (v->tune) {
        (void)song_refused(v);
        return;
    }
    about_init(&v->about, v->version);
    v->about.io = v->song.io;
    v->about.silent = (uint8_t)(v->song.io && song_io_changed(v->song.io));
    if (v->song.io) {
        song_io_about(v->song.io, 1);
    }
    screen_push(&v->s, &about_view, &v->about, SCREEN_WIPE_DOWN);
}

static void sids_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    sid_view_open(&v->sids, &v->s);
}

static void midi_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_midi, v, SCREEN_CUT);
}

static void settings_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    v->format_arm = 0;
    screen_push(&v->s, &views_settings_view, v, SCREEN_CUT);
}

/* The menu: the views and the playing values (PRE is PRESET: the 14-character line holds the
 * longest preset name, NOISE PERC); SETTINGS holds the set-and-forget ones. */
static const menu_item MENU_ITEMS[VIEWS_MENU_ITEMS] = {
    {"CHIP", NULL, chip_open},             {"TRACKER", NULL, tracker_open},
    {"SCENE", NULL, scene_open},           {"SONG", NULL, song_open},
    {"SIDS", NULL, sids_open},             {"PRE", preset_value, preset_turn},
    {"OCTAVE", octave_value, octave_turn},
    {"SETTINGS", NULL, settings_open},     {"STATUS", NULL, status_open},
    {"MIDI", NULL, midi_open},             {"CHARSET", NULL, charset_open},
    {"ABOUT", NULL, about_open},
};
_Static_assert(VIEWS_MENU_CHIP == 0u && VIEWS_MENU_TRACKER == 1u && VIEWS_MENU_SCENE == 2u &&
                   VIEWS_MENU_SONG == 3u && VIEWS_MENU_SIDS == 4u && VIEWS_MENU_PRESET == 5u &&
                   VIEWS_MENU_OCTAVE == 6u && VIEWS_MENU_SETTINGS == 7u &&
                   VIEWS_MENU_STATUS == 8u && VIEWS_MENU_MIDI == 9u &&
                   VIEWS_MENU_CHARSET == 10u && VIEWS_MENU_ABOUT == 11u,
               "the menu's items");
/* SETTINGS: ROWS is TRACKER ROWS and REC REC QUANTISE; STORE's value (FOREIGN, FORMAT?, ERASE
 * 20; 123K, the kilobytes free) fits beside its name. The popup (below) says the whole names. */
static const menu_item SETTINGS_ITEMS[VIEWS_SETTINGS_ITEMS] = {
    {"KEYS", keys_value, keys_turn},       {"SOUNDS", sounds_value, sounds_turn},
    {"ROWS", rows_value, rows_turn},       {"CARDS", cards_value, cards_turn},
    {"STORE", store_value, store_turn},    {"THEME", theme_value, theme_turn},
    {"REC", quant_value, quant_turn},      {"SPREAD", spread_value, spread_turn},
    {"SID MODEL", sid_model_value, sid_model_turn},
    {"RESET", reset_value, reset_turn},
};
_Static_assert(VIEWS_SETTINGS_KEYS == 0u && VIEWS_SETTINGS_SOUNDS == 1u &&
                   VIEWS_SETTINGS_ROWS == 2u && VIEWS_SETTINGS_CARDS == 3u &&
                   VIEWS_SETTINGS_STORE == 4u && VIEWS_SETTINGS_THEME == 5u &&
                   VIEWS_SETTINGS_REC == 6u && VIEWS_SETTINGS_SPREAD == 7u &&
                   VIEWS_SETTINGS_SID_MODEL == 8u && VIEWS_SETTINGS_RESET == 9u,
               "the settings' items");
_Static_assert(LIVE_THEMES == THEME_COUNT, "live.theme holds a theme's id");

static int menu_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint8_t was = v->home.cursor;
        menu_move(&v->home, e->steps);
        if (v->home.cursor != was) {
            s->popup_defer = 1;
            s->layers = 1;
        }
        v->format_arm = 0; /* the confirm is two turns on the item, nothing between */
        sound(v, v->home.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ); /* the end: refused */
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        /* menu_turn's own rule: an item with a value turns both ways, an action only
         * clockwise; else refused. */
        v->menu_ask = v->home.cursor;
        const menu_item* it = &v->home.items[v->home.cursor];
        int does = it->turn && (it->value || e->steps > 0);
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
    return v->home.pending || looks_changed(v, 0);
}

/* What a menu page at 2x draws besides its lines (big_menu_compose). */
#define MENU_PART_BLANK 1u /* the rest of the screen */
#define MENU_PART_TITLE 2u /* the title bar */
#define MENU_PART_HELP 4u  /* what the encoders do */
#define MENU_PART_LINES 8u /* the items */

/* A menu page at 2x: the title bar (title, and the item the cursor is on, of how many), the
 * lines, the help; `parts` says which of the others are drawn. The main menu draws all every
 * frame; a page with menu.lines only what its own compose finds missing. */
static void big_menu_compose(screen* s, views* v, menu* m, const char* title, grid* g,
                             uint32_t parts) {
    const theme* th = theme_get(v->l->theme);
    const uint8_t text = THEME_ATTR(th, hint);
    /* Every blank cell the menu draws has this attribute: the lines' own (menu_draw's) and the
     * rest of the screen's, so that a scroll sends no blank for a colour nobody sees. */
    const uint8_t blank = GRID_ATTR(th->menu_item, th->bg);
    const uint8_t head_attr = GRID_ATTR(th->title_fg, th->title_bg);
    m->sel_attr = GRID_ATTR(th->sel_fg, th->sel_bg); /* the selected line */
    m->value_attr = GRID_ATTR(th->menu_value, th->bg);
    if (parts & MENU_PART_BLANK) {
        /* Not the lines (menu_draw), the title bar or the help's text: the rest of the screen
         * blank. */
        grid_fill(g, 0, 0, GRID_COLS, MENU_ROW, ' ', blank);
        grid_fill(g, 0, MENU_ROW + 3u * m->rows, GRID_COLS,
                  MENU_HELP_ROW - MENU_ROW - 3u * m->rows, ' ', blank);
        grid_fill(g, 0, MENU_HELP_ROW, 1, 4, ' ', blank);
        grid_fill(g, 25, MENU_HELP_ROW, GRID_COLS - 25u, 4, ' ', blank);
        grid_fill(g, 0, MENU_HELP_ROW + 4u, GRID_COLS, GRID_ROWS - MENU_HELP_ROW - 4u, ' ', blank);
    }
    if (parts & MENU_PART_TITLE) {
        char head[MENU_CHARS + 1u];
        char pos[8];
        char num[4];
        number_text(num, m->cursor + 1u);
        join(pos, sizeof pos, num, "/");
        number_text(num, m->count);
        append(pos, sizeof pos, num);
        join(head, sizeof head, title, "");
        uint32_t n = length(head);
        while (n < sizeof head - 1u - length(pos)) {
            head[n++] = ' ';
        }
        head[n] = 0;
        append(head, sizeof head, pos);
        /* the arrows from column 6, or after a longer title (SETTINGS), where the position leaves
         * room (SETTINGS' 10/10: none below it then) */
        uint32_t at = length(title) > 6u ? length(title) : 6u;
        if (at < n) {
            head[at] = m->top > 0 ? (char)FONT_ARROW_UP : ' ';
        }
        if (at + 1u < n) {
            head[at + 1u] = m->top + m->rows < m->count ? (char)FONT_ARROW_DOWN : ' ';
        }
        grid_fill(g, 0, 0, GRID_COLS, 3, ' ', head_attr);
        grid_text_big(g, 1, 1, 2, head, head_attr);
    }
    /* Lines while the frame stays within MENU_FRAME_CELLS, less what the popup or a toast draws
     * the same frame (screen_layer_cells): a line is up to 60 cells, a scroll in one frame
     * would send 250. Except the lines something drew over or a view switch left. */
    uint32_t layers = screen_layer_cells(s);
    if (s->popup && s->popup->rows) {
        layers += MENU_LINE_CELLS;
    }
    m->budget = (uint16_t)(MENU_FRAME_CELLS > layers ? MENU_FRAME_CELLS - layers : 1u);
    m->force_rows = s->redraw ? s->redraw_rows : 0u;
    if (m->lines) {
        m->lines = layers || s->redraw ? 1u : MIDI_LINES;
        /* A whole redraw, a view switch, is no reason to draw what the grid shows already: the
         * lines are compared with it. */
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            m->force_rows = 0;
        }
        m->force_rows |= MENU_BUDGET_FORCED;
    } else {
        m->force_rows |= layers || s->redraw_rows != (1u << GRID_ROWS) - 1u ? MENU_BUDGET_FORCED : 0u;
    }
    if (parts & MENU_PART_LINES) {
        menu_draw(m, g, 0, MENU_ROW, GRID_COLS, blank, 0);
    } else {
        m->pending = 1; /* the next frame */
    }
    if (parts & MENU_PART_HELP) {
        /* What the controls do (ALG is ALGORITHM): SAVE opens the main menu's views (its help card
         * says ALG changes its values), ALG changes the pages' values; HOME goes back, as
         * everywhere. */
        grid_text_big(g, 1, MENU_HELP_ROW, 2, "SELECT  MOVE", text);
        grid_text_big(g, 1, MENU_HELP_ROW + 2u, 2, m == &v->home ? "SAVE    OPEN" : "ALG   CHANGE",
                      text);
    }
}

static void menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    look(v, 0, v->drawn);
    big_menu_compose(s, v, &v->home, "MENU", g,
                     MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP | MENU_PART_LINES);
}

const view views_menu = {"MENU", NULL, NULL, menu_input, menu_compose, menu_changed};

/* ---- MIDI ---- */

/* The page's items edit a copy of the setup, which midi_route_set takes whole. */
static int midi_setup(const views* v, midi_route_setup* out) {
    if (!v->route) {
        return 0;
    }
    *out = v->route->setup;
    return 1;
}

static void midi_apply(views* v, const midi_route_setup* s) {
    midi_route_set(v->route, s);
}

/* x stepped by steps within lo..hi, clamped. */
static uint8_t stepped(uint32_t x, int32_t steps, uint32_t lo, uint32_t hi) {
    int32_t n = (int32_t)x + steps;
    return (uint8_t)(n < (int32_t)lo ? lo : n > (int32_t)hi ? hi : (uint32_t)n);
}

static void midi_none(char* buf) {
    join(buf, MENU_VALUE_CHARS, "-", "");
}

static void mode_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    join(buf, MENU_VALUE_CHARS, midi_route_mode_name(s.mode), "");
}

static void mode_turn(void* ctx, int32_t steps) {
    midi_route_setup s;
    if (midi_setup(ctx, &s)) {
        s.mode = stepped(s.mode, steps, 0, MIDI_MODE_COUNT - 1u);
        midi_apply(ctx, &s);
    }
}

static void channel_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    number_text(buf, s.base + 1u);
}

static void channel_turn(void* ctx, int32_t steps) {
    midi_route_setup s;
    if (midi_setup(ctx, &s)) {
        s.base = stepped(s.base, steps, 0, 15);
        midi_apply(ctx, &s);
    }
}

static void option_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    join(buf, MENU_VALUE_CHARS, midi_route_option_name(s.option), "");
}

static void option_turn(void* ctx, int32_t steps) {
    midi_route_setup s;
    if (midi_setup(ctx, &s)) {
        s.option = stepped(s.option, steps, 0, MIDI_WHOLE_ARP);
        midi_apply(ctx, &s);
    }
}

static void legato_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    join(buf, MENU_VALUE_CHARS, s.legato ? "ON" : "OFF", "");
}

/* Clockwise on, counter-clockwise off. */
static void legato_turn(void* ctx, int32_t steps) {
    midi_route_setup s;
    if (steps != 0 && midi_setup(ctx, &s)) {
        s.legato = (uint8_t)(steps > 0);
        midi_apply(ctx, &s);
    }
}

static void interval_value(void* ctx, char* buf, uint32_t k) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    buf[0] = '+';
    number_text(buf + 1, s.chord[k]);
}

static void interval_turn(void* ctx, int32_t steps, uint32_t k) {
    midi_route_setup s;
    if (midi_setup(ctx, &s)) {
        s.chord[k] = stepped(s.chord[k], steps, 0, MIDI_ROUTE_CHORD_MAX);
        midi_apply(ctx, &s);
    }
}

static void chord2_value(void* ctx, char* buf) {
    interval_value(ctx, buf, 0);
}

static void chord2_turn(void* ctx, int32_t steps) {
    interval_turn(ctx, steps, 0);
}

static void chord3_value(void* ctx, char* buf) {
    interval_value(ctx, buf, 1);
}

static void chord3_turn(void* ctx, int32_t steps) {
    interval_turn(ctx, steps, 1);
}

static void arp_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    if (s.arp_rate == 0u) {
        join(buf, MENU_VALUE_CHARS, "CLOCK", "");
    } else {
        number_text(buf, s.arp_rate);
        append(buf, MENU_VALUE_CHARS, " FR");
    }
}

static void arp_turn(void* ctx, int32_t steps) {
    midi_route_setup s;
    if (midi_setup(ctx, &s)) {
        s.arp_rate = stepped(s.arp_rate, steps, 0, MIDI_ROUTE_ARP_MAX);
        midi_apply(ctx, &s);
    }
}

static void midi_rows_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    number_text(buf, s.rows_per_beat);
}

static void midi_rows_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    midi_route_setup s;
    if (midi_setup(v, &s)) {
        s.rows_per_beat = stepped(s.rows_per_beat, steps, 1, MIDI_ROUTE_ROWS_MAX);
        midi_apply(v, &s);
        v->song.rows_beat = s.rows_per_beat;
    }
}

static void clock_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, v->sync ? midi_sync_mode_name(v->sync->mode) : "-", "");
}

static void clock_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (v->sync) {
        midi_sync_set(v->sync, stepped(v->sync->mode, steps, 0, MIDI_SYNC_MODES - 1u),
                      v->sync->clock_out);
    }
}

static void clock_out_value(void* ctx, char* buf) {
    const views* v = ctx;
    join(buf, MENU_VALUE_CHARS, !v->sync ? "-" : v->sync->clock_out ? "ON" : "OFF", "");
}

/* Clockwise on, counter-clockwise off. */
static void clock_out_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (v->sync && steps != 0) {
        midi_sync_set(v->sync, v->sync->mode, steps > 0);
    }
}

static const menu_item MIDI_ITEMS[VIEWS_MIDI_ITEMS] = {
    {"MODE", mode_value, mode_turn},         {"CHANNEL", channel_value, channel_turn},
    {"OPTION", option_value, option_turn},   {"LEGATO", legato_value, legato_turn},
    {"CHORD 2", chord2_value, chord2_turn},  {"CHORD 3", chord3_value, chord3_turn},
    {"ARP RATE", arp_value, arp_turn},       {"ROWS/BEAT", midi_rows_value, midi_rows_turn},
    {"CLOCK", clock_value, clock_turn},      {"CLOCK OUT", clock_out_value, clock_out_turn},
};

static void midi_look(const views* v, uint8_t out[VIEWS_MIDI_LOOK]) {
    midi_route_setup s;
    if (!midi_setup(v, &s)) {
        midi_route_setup_default(&s);
    }
    out[0] = s.mode;
    out[1] = s.base;
    out[2] = s.option;
    out[3] = s.legato;
    out[4] = s.chord[0];
    out[5] = s.chord[1];
    out[6] = s.arp_rate;
    out[7] = s.rows_per_beat;
    out[8] = v->midi.cursor;
    out[9] = v->midi.top;
    out[10] = 0;
    out[11] = v->route != NULL;
    out[12] = v->sync ? v->sync->mode : 0xffu;
    out[13] = v->sync ? v->sync->clock_out : 0xffu;
}

static int midi_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint8_t was = v->midi.cursor;
        menu_move(&v->midi, e->steps);
        sound(v, v->midi.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        uint8_t before[VIEWS_MIDI_LOOK];
        uint8_t after[VIEWS_MIDI_LOOK];
        midi_look(v, before);
        menu_turn(&v->midi, e->steps);
        midi_look(v, after);
        sound(v, memcmp(before, after, sizeof before) ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
        return 1;
    }
    return 0;
}

static int midi_changed(screen* s, void* state) {
    (void)s;
    const views* v = state;
    uint8_t now[VIEWS_MIDI_LOOK];
    midi_look(v, now);
    return v->midi.pending || memcmp(now, v->midi_drawn, sizeof now) != 0;
}

static void midi_compose(screen* s, void* state, grid* g) {
    views* v = state;
    uint8_t cursor = v->midi_drawn[8];
    uint8_t top = v->midi_drawn[9];
    midi_look(v, v->midi_drawn);
    uint32_t parts = MENU_PART_LINES;
    if (s->redraw) {
        /* A view switch: the screen's own parts first, the lines from the next frame (a frame
         * is a ui_poll of at most 23k instructions: the blank, the title bar and the help, or
         * two lines); something drew over the page: all of it, the lines at one a frame. */
        parts |= MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP;
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            parts &= ~MENU_PART_LINES;
        }
    } else if (cursor != v->midi_drawn[8] || top != v->midi_drawn[9]) {
        parts |= MENU_PART_TITLE;
    }
    big_menu_compose(s, v, &v->midi, "MIDI", g, parts);
    /* The frame's first ui_poll renders two runs: of a page's big lines, the two rows of one
     * line, 23.5k instructions against the page's 23k. A first run of one cell makes it one run
     * and the next: the top left cell, blank in either colour, scanned first. */
    g->scan_row = 0;
    g->scan_col = 0;
    grid_put(g, 0, 0, ' ', (uint8_t)(g->shown_attr[0] ^ 1u));
}

const view views_midi = {"MIDI", NULL, NULL, midi_input, midi_compose, midi_changed};

/* ---- SETTINGS ---- */

static void settings_look(const views* v, uint8_t out[VIEWS_SETTINGS_LOOK]) {
    const live* l = v->l;
    uint32_t kb = 0;
    out[0] = l->keys_mode;
    out[1] = (uint8_t)(v->snd && v->snd->on);
    out[2] = v->song.scroll;
    out[3] = v->cards;
    out[4] = (uint8_t)store_look(v, &kb);
    out[5] = (uint8_t)kb;
    out[6] = l->theme;
    out[7] = v->song.quant;
    out[8] = l->spread;
    out[9] = v->setup.cursor;
    out[10] = v->setup.top;
    out[11] = v->format_arm;
    out[12] = v->set ? v->set->sid_model : 0xffu;
}

/* As the menu's: SELECT moves, ALGORITHM changes the value; STORE's format is two steps (store_turn
 * arms, SAVE confirms) and any move disarms it. */
static int settings_input(screen* s, void* state, const screen_event* e) {
    views* v = state;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint8_t was = v->setup.cursor;
        menu_move(&v->setup, e->steps);
        if (v->setup.cursor != was) {
            s->popup_defer = 1;
            s->layers = 1;
        }
        v->format_arm = 0; /* the confirm is two steps on the item, nothing between */
        sound(v, v->setup.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        v->menu_ask = v->setup.cursor;
        int does = 1;
        if (v->setup.cursor == VIEWS_SETTINGS_STORE) {
            does = v->set && v->set->st && store_state(v->set->st) == STORE_STATE_FOREIGN;
        }
        menu_turn(&v->setup, e->steps);
        sound(v, does ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
        return 1;
    }
    return 0;
}

static int settings_changed(screen* s, void* state) {
    (void)s;
    const views* v = state;
    uint8_t now[VIEWS_SETTINGS_LOOK];
    settings_look(v, now);
    return v->setup.pending || memcmp(now, v->setup_drawn, sizeof now) != 0;
}

/* As the MIDI page's compose: a view switch draws the screen's own parts first and the lines from
 * the next frames, MIDI_LINES at a time. */
static void settings_compose(screen* s, void* state, grid* g) {
    views* v = state;
    uint8_t cursor = v->setup_drawn[9];
    uint8_t top = v->setup_drawn[10];
    settings_look(v, v->setup_drawn);
    uint32_t parts = MENU_PART_LINES;
    if (s->redraw) {
        parts |= MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP;
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            parts &= ~MENU_PART_LINES;
        }
    } else if (cursor != v->setup_drawn[9] || top != v->setup_drawn[10]) {
        parts |= MENU_PART_TITLE;
    }
    big_menu_compose(s, v, &v->setup, "SETTINGS", g, parts);
    /* the frame's first run of one cell, as the MIDI page's (midi_compose) */
    g->scan_row = 0;
    g->scan_col = 0;
    grid_put(g, 0, 0, ' ', (uint8_t)(g->shown_attr[0] ^ 1u));
}

const view views_settings_view = {"SETTINGS", NULL, NULL, settings_input, settings_compose,
                                  settings_changed};

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

/* SEQ: over the view on top the song view that showed last (STEP the first time), or in one the
 * next (app/song_view.h). */
static void song_step(views* v) {
    const view* top = screen_top(&v->s);
    const view* next = !song_view_is(top) && v->song_last ? v->song_last : song_view_next(top);
    if (!next || next == top || song_refused(v)) {
        return;
    }
    if (song_view_is(top)) {
        screen_pop(&v->s, SCREEN_CUT);
    }
    screen_push(&v->s, next, song_state(v, next), SCREEN_WIPE_DOWN);
}

/* GLO's press: a second press within GLO_TAP_MS of the last latches the layer (the keys are the
 * mutes and solos without holding GLO), a press while it is latched ends it. The press goes on
 * to the panel as before (held GLO is the same layer). */
#define GLO_TAP_MS 400u
__attribute__((noinline, unused)) LIVE_COLD static void glo_tap(views* v) {
    panel* p = v->p;
    uint32_t now = v->s.now + 1u;
    if (p->glo_latch) {
        p->glo_latch = 0;
        screen_toast(&v->s, "GLO OFF", SCREEN_TOAST_MS);
    } else if (v->glo_tap_ms && now - v->glo_tap_ms <= GLO_TAP_MS) {
        p->glo_latch = 1;
        screen_toast(&v->s, "GLO LATCHED", SCREEN_TOAST_MS);
    }
    v->glo_tap_ms = now;
}

/* HOME's press only starts it (views_flow): it acts when it is let go, so that a hold can open the
 * help card instead. A capture (learn, the boot, a card) and SAVE held take the press first.
 * SEQ, PLAY and REC: the song views'. */
static int global(void* ctx, const screen_event* e) {
    views* v = ctx;
    if (e->kind != SCREEN_PRESS) {
        return 0; /* an encoder's turn: nothing here is global */
    }
    if (e->id == PANEL_BTN_HOME) { /* first: the tightest of the input gates (emu-gate screen.rs) */
        v->format_arm = 0;
        v->home_down = 1;
        v->home_at = v->s.now;
        return 1;
    }
    if (e->id == PANEL_BTN_SEQ) {
        if (!song_pages_jump(&v->pages, &v->s, e))
            song_step(v);
        return 1;
    }
    if (e->id == PANEL_BTN_PLAY) {
        int edit = (int)(e->held >> PANEL_BTN_EDIT & 1u);
        if (!sid_view_play(&v->sids, edit, sid_view_is(shows(v)))) { /* a .sid tune's, first */
            song_view_play(&v->song, edit);
        }
        return 1;
    }
    if (e->id == PANEL_BTN_REC) {
        int in_song = song_view_is(shows(v));
        song_view_rec(&v->song, in_song);
        if (!in_song) { /* no REC mark outside the song views: a toast says it */
            screen_toast(&v->s, v->song.rec ? "REC ON" : "REC OFF", SCREEN_TOAST_MS);
        }
        return 1;
    }
    if (e->id == PANEL_BTN_GLO) {
        glo_tap(v); /* and on to the panel */
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
    if (v->pages.pending)
        return;
    song_view_set_transport(&v->song, t);
}

void views_tune(views* v, int on) {
    v->tune = (uint8_t)(on != 0);
    song_view_away(&v->song, on);
}

void views_set_tune(views* v, const sid_transport* t) {
    sid_view_set_transport(&v->sids, t);
}

static int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
                      uint32_t* max, uint32_t* accent);

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

void views_route(views* v, midi_route* route) {
    v->route = route;
    if (route) {
        v->song.rows_beat = route->setup.rows_per_beat;
    }
}

void views_sync(views* v, midi_sync* sync) {
    v->sync = sync;
}

void views_song_io(views* v, song_io* io) {
    v->song.io = io;
}

void views_song_loaded(views* v) {
    song_view_loaded(&v->song);
    uint8_t page = v->pages.page;
    song_pages_init(&v->pages, &v->song);
    v->pages.page = page;
    step_view_init(&v->step, &v->song);
    v->songs.built = 0;
}

void views_song_back(views* v) {
    song_view* sv = &v->song;
    uint8_t place[5] = {sv->voice, sv->instr, sv->order_pos, sv->row, sv->subtune};
    step_view* st = &v->step;
    uint8_t lanes[2 + 2u * SONG_T_VOICES];
    lanes[0] = st->page;
    lanes[1] = st->gate;
    memcpy(lanes + 2, st->note, sizeof st->note);
    memcpy(lanes + 2 + SONG_T_VOICES, st->instr, sizeof st->instr);
    song_pages* pg = &v->pages;
    uint16_t order_cursor = pg->order_cursor;
    uint8_t cursors[4] = {pg->subtune, pg->instr_cursor, pg->table, pg->table_row};
    views_song_loaded(v);
    sv->voice = place[0];
    sv->instr = place[1];
    sv->order_pos = place[2];
    sv->row = place[3];
    sv->subtune = place[4];
    st->page = lanes[0];
    st->gate = lanes[1];
    memcpy(st->note, lanes + 2, sizeof st->note);
    memcpy(st->instr, lanes + 2 + SONG_T_VOICES, sizeof st->instr);
    pg->order_cursor = order_cursor;
    pg->subtune = cursors[0];
    pg->instr_cursor = cursors[1];
    pg->table = cursors[2];
    pg->table_row = cursors[3];
}

void views_init(views* v, grid* g, live* l, scope* sc, const char* version) {
    v->l = l;
    v->sc = sc;
    v->version = version;
    v->cards = 1;
    v->cards_seen = 0;
    v->home_down = 0;
    v->help.done = 0;
    v->tour = 0;
    v->toast_held = 0;
    v->song_last = NULL;
    v->tune = 0;
    v->snd = NULL;
    v->set = NULL;
    v->format_arm = 0;
    v->resetting = 0;
    v->formatting = 0;
    v->menu_ask = 0;
    save_key_init(&v->key);
    menu_init(&v->home, MENU_ITEMS, VIEWS_MENU_ITEMS, VIEWS_MENU_SHOWN, v);
    v->home.scale = 2;
    v->home.sel_attr = GRID_ATTR(theme_get(l->theme)->sel_fg, theme_get(l->theme)->sel_bg);
    menu_init(&v->midi, MIDI_ITEMS, VIEWS_MIDI_ITEMS, VIEWS_MENU_SHOWN, v);
    v->midi.scale = 2;
    v->midi.lines = MIDI_LINES;
    menu_init(&v->setup, SETTINGS_ITEMS, VIEWS_SETTINGS_ITEMS, VIEWS_MENU_SHOWN, v);
    v->setup.scale = 2;
    v->setup.lines = MIDI_LINES;
    v->route = NULL;
    v->sync = NULL;
    remember(v);
    v->p = NULL;
    learn_view_init(&v->learn, l, NULL);
    chip_view_init(&v->chip, l, &v->song);
    user_profiles_init(&v->users, l, NULL);
    chip_view_set_users(&v->chip, &v->users);
    live_view_init(&v->live, l, sc);
    scene_view_init(&v->scene, l, sc);
    song_view_init(&v->song, l);
    song_pages_init(&v->pages, &v->song);
    step_view_init(&v->step, &v->song);
    v->songs.built = 0;
    v->songs.scanning = 0;
    v->song.editor = &v->editor;
    song_edit_init(&v->editor, NULL);
    sid_view_init(&v->sids, &v->song);
    memset(v->tune_drawn, 0, sizeof v->tune_drawn);
    screen_init(&v->s, g, &live_view_view, &v->live);
    screen_global(&v->s, global, v);
    screen_popup_source(&v->s, &v->popup, popup_text, v);
    screen_theme_source(&v->s, &l->theme);
    if (VIEWS_POPUP_MIDI) {
        views_popup_midi(v, 1);
    }
}

void views_learn(views* v, midi_learn* ml) {
    learn_view_init(&v->learn, v->l, ml);
}

void views_settings(views* v, settings* set) {
    v->set = set;
    v->sids.st = set ? set->st : NULL;
    v->users.st = set ? set->st : NULL;
    user_profiles_load(&v->users);
}

/* The first-run tour's steps (tour_frame, below). */
enum {
    TOUR_OFF,
    TOUR_BOOT, /* the boot is still on */
    /* Step s shows as TOUR_K1 + 2 s (TURN K1, until the cutoff or the selected slot's amount
     * changes; PRESS ENV, until the selected slot's algorithm changes; HOLD HOME FOR HELP, until
     * a help card opens) and the gap before step s + 1 as TOUR_K1 + 2 s + 1. */
    TOUR_K1
};
#define TOUR_TOAST_MS 8000u
#define TOUR_GAP_MS 1600u

void views_boot(views* v, int full, int home_held) {
    boot_init(&v->boot, v->l, v->version, full || home_held, home_held);
    v->tour = (uint8_t)(full && !home_held ? TOUR_BOOT : TOUR_OFF); /* HOME asked for it: no tour */
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
 * every frame. Inlined into the encoder's input (knob_page) whatever the file's optimisation,
 * for the popup input's bound (emu-gate screen.rs POPUP_INPUT); shows everywhere else. */
__attribute__((always_inline)) static inline const view* shown(const views* v) {
    const screen* s = &v->s;
    const view* top = screen_top(s);
    if (top == &learn_view_view && s->depth >= 2u) {
        top = s->stack[s->depth - 2u].v;
    }
    return top;
}

static const view* shows(const views* v) {
    return shown(v);
}

/* What K1..K4 turn (app/panel.h panel.page): the CHIP view's page while it shows, LIVE's
 * anywhere else; and the panel MIDI learn reads it from. Before each event and after the views
 * took it, so the panel, which turns the knob, and learn, which picks it, read the same. */
__attribute__((always_inline)) static inline void knob_page(views* v, panel* p) {
    v->p = p;
    p->page = (uint8_t)(shown(v) == &chip_view_view ? chip_view_page(&v->chip) : PANEL_PAGE_LIVE);
}

/* The boot or a title card on top: they take every control, SAVE included (it ends them). */
static int curtain(const views* v) {
    const view* top = screen_top(&v->s);
    return top == &boot_view || top == &title_card_view || top == &help_view_view;
}

/* ---- HOME: its release, and a hold ---- */

/* HOME held this long opens the help card of the view that shows. */
#define HOME_HELP_MS 600u

/* The menu from the root view, back one view from any other: from the menu to LIVE (behind THE
 * OSCILLATOR's card with CARDS at ALWAYS). What HOME's press did, on its release. */
__attribute__((noinline, unused)) static void home_go(views* v) {
    const view* from = screen_top(&v->s);
    if (curtain(v) || from == &learn_view_view) {
        return; /* a capture that appeared during the hold took HOME's place */
    }
    if (from == &views_song_menu_view && song_menu_back(v)) {
        return; /* back a page of SONG's */
    }
    if (from == &sid_view_browser && sid_view_home(&v->sids)) {
        return; /* the question cancelled */
    }
    if (from == &song_pages_table_view && song_pages_arp_close(&v->pages)) {
        return; /* out of the ARP helper, back to the table */
    }
    if (screen_pop(&v->s, SCREEN_WIPE_UP) != 0) {
        screen_push(&v->s, &views_menu, v, SCREEN_WIPE_DOWN);
    } else if (from == &views_menu && screen_top(&v->s) == &live_view_view) {
        show_card(v, TITLE_CARD_OSCILLATOR);
    }
}

/* The help card for the view that shows, in the state it is in (help_view.h: CHIP by its focus,
 * STEP with a step held or recording, TRACKER with REC on, LIVE on a slot's page, SETTINGS on
 * STORE), and the state's words for it into *arg. */
static uint32_t help_card_of(const views* v, uint32_t* arg) {
    const view* top = shows(v);
    const live* l = v->l;
    uint32_t slot = l->fx_select % LIVE_SLOTS;
    *arg = slot | (uint32_t)l->slot[slot][LIVE_SLOT_ALGORITHM] << 8;
    if (top == &live_view_view) {
        return v->live.page ? HELP_CARD_LIVE_SLOT : HELP_CARD_LIVE;
    }
    if (top == &views_menu) {
        return HELP_CARD_MENU;
    }
    if (top == &views_midi) {
        return HELP_CARD_MIDI;
    }
    if (top == &views_settings_view) {
        return v->setup.cursor == VIEWS_SETTINGS_STORE   ? HELP_CARD_STORE
               : v->setup.cursor == VIEWS_SETTINGS_RESET ? HELP_CARD_RESET
                                                         : HELP_CARD_SETTINGS;
    }
    if (top == &views_song_menu_view) {
        uint8_t page = v->songs.page;
        return page == SONG_MENU_DIR    ? HELP_CARD_FILES
               : page == SONG_MENU_NAME ? HELP_CARD_NAME
               : page == SONG_MENU_CONFIRM ? HELP_CARD_CONFIRM
                                           : HELP_CARD_SONG;
    }
    if (top == &about_view) {
        return HELP_CARD_ABOUT;
    }
    if (top == &sid_view_browser) {
        return v->sids.asking ? HELP_CARD_UNSAVED : HELP_CARD_SIDS;
    }
    if (top == &sid_view_playing) {
        return HELP_CARD_PLAYING;
    }
    if (top == &views_charset) {
        return HELP_CARD_CHARSET;
    }
    if (top == &chip_view_view) {
        uint32_t f = chip_view_focus(&v->chip);
        return f == CHIP_VIEW_TABS     ? HELP_CARD_CHIP_TABS
               : f == CHIP_VIEW_CHIP   ? HELP_CARD_CHIP
               : f == CHIP_VIEW_EXPERT ? HELP_CARD_EXPERT
                                       : HELP_CARD_CHIP_SLOT;
    }
    if (top == &scene_view_view) {
        return HELP_CARD_SCENE;
    }
    int recording = song_view_rec_playing(&v->song);
    if (recording) {
        *arg = v->song.quant;
    }
    if (top == &step_view_view) {
        return recording ? HELP_CARD_STEP_REC
               : v->step.held != STEP_VIEW_NONE ? HELP_CARD_STEP_HELD
                                                : HELP_CARD_STEP;
    }
    if (top == &tracker_view_view) {
        return recording ? HELP_CARD_TRACKER_REC
               : v->song.rec ? HELP_CARD_TRACKER_EDIT
                             : HELP_CARD_TRACKER;
    }
    if (top == &song_pages_order_view) {
        return HELP_CARD_ORDER;
    }
    if (top == &song_pages_instr_view) {
        return HELP_CARD_INSTR;
    }
    return top == &song_pages_table_view ? (v->pages.arp ? HELP_CARD_ARP : HELP_CARD_TABLE)
                                         : HELP_CARD_STATUS;
}

/* ---- the first-run tour: three toasts after the full boot an install plays ---- */

/* A number that changes whenever what step s waits for does: the cutoff or the selected slot's
 * amount (a step of either moves it by an odd amount, which 16 bits cannot wrap back), then the
 * selected slot's algorithm. */
static uint16_t tour_watch(const views* v, uint32_t s) {
    const live* l = v->l;
    const uint8_t* slot = l->slot[l->fx_select];
    return s ? slot[LIVE_SLOT_ALGORITHM]
             : (uint16_t)(l->cutoff * 129u + slot[LIVE_SLOT_AMOUNT] * 517u);
}

static const char* const TOUR_TEXT[3] = {"TURN K1", "PRESS ENV", "HOLD HOME FOR HELP"};

static void tour_begin(views* v, uint32_t now_ms, uint32_t s) {
    v->tour = (uint8_t)(TOUR_K1 + 2u * s);
    v->tour_at = now_ms;
    v->tour_ref = tour_watch(v, s);
    screen_toast(&v->s, TOUR_TEXT[s], TOUR_TOAST_MS);
}

/* 1 when the screen's toast text is one of the tour's. */
static int tour_text(const views* v) {
    for (uint32_t i = 0; i < 3u; i++) {
        if (strcmp(v->s.toast, TOUR_TEXT[i]) == 0) {
            return 1;
        }
    }
    return 0;
}

/* Ends the tour's own toast, if one shows: never a toast the action itself raised (ENV's DIST
 * TAPE, tell()'s). */
static void tour_clear(views* v) {
    if (v->s.toast_on && tour_text(v)) {
        screen_toast(&v->s, "", 0);
    }
}

/* Once a frame while the tour runs (views_flow): step s toasts, waits for the action it asks for
 * or TOUR_TOAST_MS, and the next comes TOUR_GAP_MS after, so the action's own toast and popup
 * have gone. An action ends its step's toast at once (K1's popup is the answer; on another view
 * than LIVE ENV's own toast replaces it); a help card, opened by the way or as the last step asks, ends the tour. */
__attribute__((noinline, unused)) static void tour_frame(views* v, uint32_t now_ms) {
    uint32_t t = v->tour;
    if (t == TOUR_BOOT) {
        if (screen_top(&v->s) != &boot_view) {
            tour_begin(v, now_ms, 0);
        }
        return;
    }
    if (screen_top(&v->s) == &help_view_view) {
        tour_clear(v); /* nothing over the card (another toast waits: toast_hold) */
        v->tour = TOUR_OFF;
        return;
    }
    uint32_t s = (t - TOUR_K1) >> 1;
    uint32_t waited = now_ms - v->tour_at;
    if (t & 1u) { /* the gap before step s + 1 */
        if (waited >= TOUR_GAP_MS) {
            tour_begin(v, now_ms, s + 1u);
        }
        return;
    }
    int acted = s < 2u && tour_watch(v, s) != v->tour_ref;
    if (acted && (s == 0u || screen_top(&v->s) == &live_view_view)) {
        tour_clear(v); /* LIVE says an algorithm itself: no toast of its own */
    }
    if (acted || waited >= TOUR_TOAST_MS) {
        v->tour = s == 2u ? TOUR_OFF : (uint8_t)(t + 1u);
        v->tour_at = now_ms;
    }
}

/* HOME let go goes home_go, HOME held HOME_HELP_MS opens the help card, a card that was closed is
 * popped (views_flow in app/views.h); and the tour's steps. */
void views_flow(views* v, uint32_t now_ms) {
    if (v->tour) {
        tour_frame(v, now_ms);
    }
    if (!v->p) {
        return;
    }
    if (v->help.done && screen_top(&v->s) == &help_view_view) {
        screen_pop(&v->s, SCREEN_BANDS);
        v->s.popup_ask = 0; /* the control that closed it did nothing else */
    }
    v->help.done = 0;
    if (!v->home_down) {
        return;
    }
    if (v->p->held >> PANEL_BTN_HOME & 1u) {
        if (v->home_down == 1u && now_ms - v->home_at >= HOME_HELP_MS && !curtain(v) &&
            screen_top(&v->s) != &learn_view_view) {
            uint32_t arg;
            uint32_t card = help_card_of(v, &arg);
            if (help_view_open(&v->help, &v->s, card, arg) == 0) {
                v->home_down = 2u;
            } else {
                v->home_down = 0; /* the stack is full: no card, so HOME does what a tap does */
                home_go(v);
            }
        }
        return;
    }
    uint8_t acts = v->home_down == 1u;
    v->home_down = 0;
    if (acts) {
        home_go(v);
    }
}

/* ---- the popup: what a control shows ---- */

/* "+3" for a sync ratio's semitones, "-12", "+0". */
static void ratio_text(char* buf, int32_t semis) {
    char num[3];
    number_text(num, (uint32_t)(semis < 0 ? -semis : semis));
    char sign[2] = {semis < 0 ? '-' : '+', 0};
    join(buf, SCREEN_POPUP_TEXT, sign, num);
}

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

__attribute__((noinline, unused)) static int expert_value_text(uint32_t index, uint32_t raw,
                                                             const chip_setup* setup, char* name, char* value,
                                                             uint32_t* fill, uint32_t* max) {
    if (index >= SID_EXPERT_COUNT) {
        return 0;
    }
    join(name, SCREEN_POPUP_TEXT, chip_view_expert_name(index), "");
    const sid_chip_profile* p = sid_chip_profile_get(setup->profile);
    sid_model model = p ? (sid_model)p->model : SID_MODEL_6581;
    if (!sid_character_expert_active(model, index)) {
        join(value, SCREEN_POPUP_TEXT, "N/A", "");
        *fill = *max = 0;
        return 1;
    }
    chip_view_expert_setup_text(setup, index, value);
    *fill = raw;
    *max = SID_CHARACTER_MAX;
    return 1;
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
        char pct[4];
        number_text(pct, l->volume);
        join(value, SCREEN_POPUP_TEXT, pct, "%");
        *fill = l->master;
        *max = LIVE_MASTER_MAX;
    } else if ((param >= LIVE_PARAM_AGE && param <= LIVE_PARAM_BLEED) || param == LIVE_PARAM_DECAY ||
               param == LIVE_PARAM_RELEASE) {
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
    } else if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
        chip_setup setup;
        chip_setup_of(l, &setup);
        setup.profile = l->profile_req;
        return expert_value_text(param - LIVE_PARAM_EXPERT, live_param_get(l, param),
                                 &setup, name, value, fill, max);
    } else {
        return 0;
    }
    return 1;
}

/* The CHIP view's ALGORITHM: the profile on the chip (the song's on a song's chip A), the
 * algorithm on a slot. The tabs' switch shows nothing of its own. */
static int chip_choice_text(const views* v, char* name, char* value, int edit) {
    const chip_view* cv = &v->chip;
    const live* l = v->l;
    if (cv->tabs) {
        return 0;
    }
    if (!cv->chip && !cv->expert) {
        uint32_t k = l->fx_select % LIVE_SLOTS;
        join(name, SCREEN_POPUP_TEXT, SLOT_NAMES[k], "");
        join(value, SCREEN_POPUP_TEXT, views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]),
             "");
        return 1;
    }
    char user[10] = "USER n";
    if (edit) { /* EDIT held: the save target, USER 1..8 or OFF; an empty slot ends in a dash */
        uint32_t t = CHIP_VIEW_TARGET(cv);
        user[5] = (char)('0' + t);
        if (t && !(l->user_valid >> (t - 1u) & 1u)) {
            append(user, sizeof user, " -");
        }
        join(name, SCREEN_POPUP_TEXT, "SAVE TO", "");
        join(value, SCREEN_POPUP_TEXT, t ? user : "OFF", "");
        return 1;
    }
    if (cv->expert) {
        char page[] = "1/4";
        page[0] = (char)('1' + cv->expert_page);
        page[2] = (char)('0' + CHIP_VIEW_EXPERT_PAGES);
        join(name, SCREEN_POPUP_TEXT, "EXPERT PAGE", "");
        join(value, SCREEN_POPUP_TEXT, page, "");
        return 1;
    }
    join(name, SCREEN_POPUP_TEXT, "PROFILE", "");
    uint32_t n = (uint32_t)chip_view_user(cv); /* a USER slot picked: its number, not its base */
    user[5] = (char)('0' + n);
    if (n) {
        join(value, SCREEN_POPUP_TEXT, user, "");
    } else { /* the model and the profile's word: "6581 CLASSIC" */
        profile_label(value, chip_view_song_tab(cv) ? cv->kr->song_want.profile : l->profile_req);
    }
    return 1;
}

/* ALGORITHM on a value item of the menu or SETTINGS (set, 1): its name (in full) and value, as
 * the page shows them. */
static int menu_choice_text(const views* v, int set, char* name, char* value) {
    const menu_item* it = &(set ? &v->setup : &v->home)->items[v->menu_ask];
    if (!it->value) {
        return 0; /* an action opens its view */
    }
    uint32_t i = v->menu_ask + (set ? VIEWS_MENU_ITEMS : 0u); /* one numbering for both */
    join(name, SCREEN_POPUP_TEXT,
         i == VIEWS_MENU_PRESET ? "PRESET" : i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_ROWS ? "TRACKER ROWS" :
         i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_REC ? "REC QUANTISE" : it->label,
         "");
    char text[MENU_VALUE_CHARS]; /* (the menu's values have room for a longer text) */
    text[0] = 0;
    if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_THEME) {
        join(text, sizeof text, theme_get(v->l->theme)->name, ""); /* in full, not the menu's */
    } else if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_RESET && v->format_arm == 2u) {
        join(name, SCREEN_POPUP_TEXT, "RESET ALL?", "");
        join(text, sizeof text, "SAVE RESETS", "");
    } else if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_STORE && v->format_arm == 1u) {
        join(text, sizeof text, "SAVE FORMATS", ""); /* the confirm the menu's FORMAT? asks for */
    } else if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_STORE && v->set && v->set->st && v->set->st->mount_newer &&
               store_state(v->set->st) == STORE_STATE_FOREIGN) {
        join(text, sizeof text, "NEWER SIDOU", "");
    } else {
        it->value((void*)v, text);
    }
    text[SCREEN_POPUP_CHARS] = 0;
    join(value, SCREEN_POPUP_TEXT, text, "");
    return 1;
}

/* The popup's source (screen_popup_source): the control the panel changed, as the view that
 * shows has it. 0 under the boot, a title card and MIDI learn's frame, and for what has no
 * value to show. Incoming MIDI never asks (docs/reference/controls.md "Value popup"). */
static int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
                      uint32_t* max, uint32_t* accent) {
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
    if (shows(v) == &tracker_view_view && v->song.rec) {
        *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4
                                                    ? enc - PANEL_ENC_K1 : 0u];
        return tracker_view_popup(&v->song, enc, (ask & ASK_EDIT) != 0u, name, value, fill, max);
    }
    if (shows(v) == &step_view_view &&
        step_view_popup(&v->step, enc, (ask & ASK_EDIT) != 0u, name, value, fill, max)) {
        *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 ?
                                          enc - PANEL_ENC_K1 : 0u];
        return 1;
    }
    if (song_view_is(shows(v)) && shows(v) != &tracker_view_view &&
        shows(v) != &step_view_view &&
        song_pages_popup(&v->pages, shows(v), enc, (ask & ASK_EDIT) != 0u, name, value, fill, max)) {
        *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 ?
                                          enc - PANEL_ENC_K1 : 0u];
        return 1;
    }
    if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4) {
        *accent = theme_get(l->theme)->knob[enc - PANEL_ENC_K1]; /* the knob's colour, as on the page */
        uint32_t held = ask & ASK_EDIT ? 1u << PANEL_BTN_EDIT : 0u;
        uint32_t param = panel_knob_param(v->p, l, held, enc);
        if (param != LIVE_PARAM_COUNT) {
            return param_text(v, param, name, value, fill, max);
        }
        if (v->p && v->p->page == PANEL_PAGE_NONE) {
            if (v->chip.expert) {
                uint32_t index = v->chip.expert_page * 4u + enc - PANEL_ENC_K1;
                return expert_value_text(index, chip_view_expert_value(&v->chip, index),
                                         &v->chip.kr->song_want,
                                         name, value, fill, max);
            }
            /* a song's chip A: the CHIP view turns the song's character itself */
            uint32_t k = enc - PANEL_ENC_K1;
            learn_view_param_name(LIVE_PARAM_AGE + k, name);
            number_value(value, v->chip.kr->song_want.controls[k], fill, max,
                         live_param_max(LIVE_PARAM_AGE + k));
            return 1;
        }
        if (v->p && v->p->page >= PANEL_PAGE_EXPERT_FIRST) {
            return 0;
        }
        /* K3 on a sync sound: the ratio, no parameter (its MIDI learn says so) */
        join(name, SCREEN_POPUP_TEXT, "RATIO", "");
        ratio_text(value, l->sync_semis);
        *fill = (uint32_t)(l->sync_semis + LIVE_RATIO_SEMIS);
        *max = 2u * LIVE_RATIO_SEMIS;
        return 1;
    }
    if (enc == PANEL_ENC_PRESETS) {
        join(name, SCREEN_POPUP_TEXT, "PRESET", "");
        live_preset_name(l, value);
        return 1;
    }
    if (enc == PANEL_ENC_ALGORITHM) {
        const view* top = shows(v);
        if (top == &chip_view_view) {
            return chip_choice_text(v, name, value, (ask & ASK_EDIT) != 0u);
        }
        if (top == &views_menu || top == &views_settings_view) {
            return menu_choice_text(v, top == &views_settings_view, name, value);
        }
        if (top == &views_midi) {
            const menu_item* it = &v->midi.items[v->midi.cursor];
            join(name, SCREEN_POPUP_TEXT, it->label, "");
            char text[MENU_VALUE_CHARS] = {0};
            it->value((void*)v, text);
            text[SCREEN_POPUP_CHARS] = 0;
            join(value, SCREEN_POPUP_TEXT, text, "");
            return 1;
        }
    }
    return 0;
}

/* SAVE's clean release (app/save_key.h): the precedence of views.h's routing. The help cards'
 * completeness test mirrors which views and states SAVE has a meaning of its own in (help_view.rs
 * saves()): a new one goes there too, and on its card. */
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
    if (shows(v) == &views_song_menu_view) {
        song_menu_ok(v); /* the page's OK: the highlighted item, SAVE AS's name */
        return;
    }
    if (shows(v) == &sid_view_browser) {
        /* the page's OK, or the question's SAVE: an unnamed song first through SAVE AS */
        if (sid_view_save(&v->sids, &v->s) == SID_VIEW_SAVE_AS) {
            song_name(v, 1);
            screen_push(&v->s, &views_song_menu_view, v, SCREEN_WIPE_DOWN);
        }
        return;
    }
    if (shows(v) == &song_pages_table_view && v->pages.arp) {
        song_pages_arp_save(&v->pages, &v->s); /* the ARP helper's rows, not the song */
        return;
    }
    if (song_view_is(shows(v))) {
        if (v->song.io && v->song.io->name[0]) {
            song_io_save(v->song.io, v->song.io->name, v->s.now);
        } else {
            song_name(v, 1);
            if (v->s.depth == SCREEN_DEPTH) {
                v->songs.return_view = screen_top(&v->s);
                v->songs.direct = 2;
                screen_pop(&v->s, SCREEN_CUT);
            }
            screen_push(&v->s, &views_song_menu_view, v, SCREEN_WIPE_DOWN);
        }
        return;
    }
    if (shows(v) == &views_menu) {
        /* the menu's OK: an action opens its view; a value item, nothing (ALGORITHM changes it) */
        const menu_item* it = &v->home.items[v->home.cursor];
        if (!it->value) {
            v->menu_ask = v->home.cursor;
            it->turn(v, 1);
            sound(v, UI_SOUND_CONFIRM);
        }
        return;
    }
    if (shows(v) == &views_settings_view) {
        if (!store_confirm(v)) { /* the armed STORE's format; else RESET's; else nothing */
            (void)reset_confirm(v);
        }
        return;
    }
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
    if (v->live.arp_down) {
        live_view_buttons(&v->live, held); /* ARP's release on LIVE: the arpeggio on or off */
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
    if (down | pressed | sv->jam_keys | v->step.down) {
        const view* top = shows(v);
        if (top == &help_view_view && v->s.depth >= 2u) {
            top = v->s.stack[v->s.depth - 2u].v; /* the keys play the view under the card */
        }
        int glo = (int)(((p->held >> PANEL_BTN_GLO) | p->glo_latch) & 1u); /* held, or latched */
        int take = !glo && sv->t && song_view_is(top);
        uint32_t steps = 0u, jam_down = down, jam_pressed = pressed;
        if (top == &step_view_view) {
            /* The 16 leftmost keys are STEP's; the rest are the jam's. */
            steps = step_view_keys(&v->step, &v->s, down, pressed, !glo);
            jam_down &= ~((1u << STEP_VIEW_STEPS) - 1u);
            jam_pressed &= ~((1u << STEP_VIEW_STEPS) - 1u);
        }
        uint32_t owned = take ? panel_owned(p, jam_down & ~jam_pressed & ~sv->jam_keys) : 0u;
        uint32_t taken = steps | song_view_keys(sv, jam_down, jam_pressed, owned, take,
                                        top == &tracker_view_view ? &v->s : NULL,
                                        top == &tracker_view_view || top == &step_view_view
                                            ? &v->s : NULL);
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
            /* the knob picks; on a slot page of LIVE, the slot's control, as with EDIT held */
            uint32_t held = shows(v) == &live_view_view && v->live.page
                                ? p->held | 1u << PANEL_BTN_EDIT
                                : p->held;
            if (learn_view_open(&v->learn, &v->s, p, enc, held) != 0 && v->s.depth >= SCREEN_DEPTH) {
                screen_toast(&v->s, "NO ROOM FOR LEARN", SCREEN_TOAST_MS); /* the stack is full */
            }
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
    int toast = 0;
    /* LIVE says the slot and its algorithm in its bottom band at 2x: no toast for those there. */
    int says = screen_top(&v->s) != &live_view_view;
    if (l->octave != v->octave) {
        octave_text(num, l->octave);
        join(buf, sizeof buf, "OCTAVE ", num);
        changed = toast = 1;
    }
    if (l->fx_select != v->fx_select) {
        if (says) {
            join(buf, sizeof buf, "SLOT ", SLOT_NAMES[l->fx_select % LIVE_SLOTS]);
            toast = 1;
        }
        changed = 1;
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint8_t alg = l->slot[k][LIVE_SLOT_ALGORITHM];
        if (alg != v->algorithm[k]) {
            if (says) {
                join(buf, sizeof buf, SLOT_NAMES[k], " ");
                append(buf, sizeof buf, views_algorithm_name(k, alg));
                toast = 1;
            }
            changed = 1;
        }
    }
    if ((uint8_t)l->model_req != v->model) {
        join(buf, sizeof buf, "CHIP ", l->model_req == SID_MODEL_8580 ? "8580" : "6581");
        screen_raster(&v->s, VIEWS_RASTER_STEPS);
        changed = toast = 1;
    }
    if (changed) {
        /* A control's own popup (a K knob, ALGORITHM, MASTER) says it: no toast as well. */
        if (toast && !(v->s.popup_ask & 7u)) {
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
    } else if (r == USER_PROFILE_FOREIGN || r == USER_PROFILE_FAILED) {
        screen_toast(&v->s, r == USER_PROFILE_FAILED ? "STORE ERROR: NOT SAVED"
                                                     : "STORE FOREIGN: NOT SAVED",
                     SCREEN_TOAST_MS);
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
        screen_toast(&v->s, v->resetting ? "RESET AND SAVED" : "SAVED", SCREEN_TOAST_MS);
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
    case SETTINGS_FAILED:
        screen_toast(&v->s, "STORE ERROR: NOT SAVED", SCREEN_TOAST_MS);
        sound(v, UI_SOUND_BUZZ);
        break;
    default: break;
    }
    v->resetting = 0;
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

/* LIVE and CHIP show a control's change in the control's own place, so the value popup that a
 * K knob (or ALGORITHM) would open over them is dropped here, in the frame, and the place
 * highlights for a second instead (live_view_flash, chip_view_flash): the input paths stay as
 * they were. `top` is the view that shows; views_frame calls this for LIVE and CHIP only (on
 * STATUS and the menu it costs two tests). Other views keep their popups. */
/* The song's state for LIVE: its tempo, and the play mark's beat (a beat's first row, a bar's
 * being the first of four beats), held 120 ms (SONG_BEAT_MS). */
#define SONG_BEAT_MS 120u
__attribute__((noinline, unused)) LIVE_COLD static void live_song(views* v, uint32_t now_ms) {
    live_view* lv = &v->live;
    uint32_t bpm = song_view_shown_bpm(&v->song);
    lv->bpm = bpm == SONG_VIEW_NO_BPM ? 0u : (uint16_t)(bpm > 999u ? 999u : bpm);
    uint32_t entry, row;
    song_view_position(&v->song, 0, &entry, &row);
    uint32_t rb = v->song.rows_beat ? v->song.rows_beat : 4u;
    if (row != lv->row_seen) {
        lv->row_seen = (uint8_t)row;
        if (row % rb == 0u) {
            lv->beat = row % (4u * rb) == 0u ? 2u : 1u;
            lv->beat_until = now_ms + SONG_BEAT_MS;
        }
    }
    if (lv->beat && (int32_t)(now_ms - lv->beat_until) >= 0) {
        lv->beat = 0;
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void live_frame(views* v, const view* top,
                                                                    uint32_t now_ms) {
    uint32_t ask = v->s.popup_ask;
    uint32_t enc = ask & 7u;
    int control = (ask & (ASK_BASE | ASK_PARAM)) == ASK_BASE && enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4;
    if (top == &live_view_view) {
        const panel* p = v->p;
        v->live.edit = p ? (uint8_t)(p->held >> PANEL_BTN_EDIT & 1u) : 0u;
        /* ARP held since its press here: the knob row is the arpeggio's */
        v->live.arp = p ? (uint8_t)(p->held >> PANEL_BTN_ARP & v->live.arp_down) : 0u;
        /* GLO's layer: held or latched (bit 0), latched (bit 1); the song, while it plays */
        v->live.glo = p ? (uint8_t)(((p->held >> PANEL_BTN_GLO | p->glo_latch) & 1u) | p->glo_latch << 1)
                        : 0u;
        /* The song views read the player's status while they show; LIVE does the same, for the
         * play mark and the tempo (not while a PLAY waits for the player: its state is stale). */
        const song_transport* t = v->song.t;
        if (t && !v->song.want) {
            t->status(t->ctx, &v->song.st);
        }
        v->live.playing = (uint8_t)(song_view_playing(&v->song) != 0);
        if (v->live.playing) {
            live_song(v, now_ms);
        } else {
            v->live.bpm = 0;
            v->live.beat = 0;
            v->live.row_seen = 0xffu; /* the song's first row is a beat */
        }
        if (control) {
            live_view_flash(&v->live, enc - PANEL_ENC_K1 + 1u, now_ms);
            v->s.popup_ask = 0;
        } else if ((ask & (ASK_BASE | ASK_PARAM)) == ASK_BASE) {
            if (enc == PANEL_ENC_ALGORITHM) { /* the profile (TONE) or the slot's algorithm */
                live_view_flash(&v->live, v->live.page || v->live.edit ? LIVE_VIEW_HL_ALGORITHM
                                                                       : LIVE_VIEW_HL_PROFILE,
                                now_ms);
                v->s.popup_ask = 0;
            } else if (enc == PANEL_ENC_PRESETS) {
                v->s.popup_ask = 0; /* the header says the preset */
            }
        }
        return;
    }
    /* CHIP: K1..K4 with the chip focus and EDIT held turn a slot that is not on the page: the
     * popup says which; the expert pages keep theirs (their values carry units). */
    const chip_view* cv = &v->chip;
    if (control && !cv->expert && (!(ask & ASK_EDIT) || !cv->chip)) {
        chip_view_flash(&v->chip, enc - PANEL_ENC_K1, now_ms);
        v->s.popup_ask = 0;
    } else if ((ask & (ASK_BASE | ASK_PARAM | ASK_EDIT)) == ASK_BASE && enc == PANEL_ENC_ALGORITHM) {
        v->s.popup_ask = 0; /* the choice shows at 3x on the page */
    }
}

/* Nothing draws over a help card: a toast raised while it shows (not the tour's: the card ends
 * the tour) waits, its time left kept, and shows once the card has closed. The card's frame
 * budget then holds (help_view.c counts the layers too). */
__attribute__((noinline, unused)) static void toast_hold(views* v) {
    if (screen_top(&v->s) == &help_view_view) {
        uint32_t left = screen_toast_take(&v->s);
        if (left && v->s.toast[0] && !tour_text(v)) { /* the card ends the tour: its toast too */
            v->toast_held = (uint16_t)(left > 0xffffu ? 0xffffu : left);
        }
    } else if (v->toast_held) {
        if (v->s.toast[0]) {
            screen_toast(&v->s, v->s.toast, v->toast_held);
        }
        v->toast_held = 0;
    }
}

void views_frame(views* v, uint32_t now_ms) {
    boot_frame(v, now_ms);
    card_frame(v);
    if (curtain(v)) {
        /* Nothing over the boot or a card (app/boot.h, app/title_card.h, app/help_view.h), and
         * nothing of what changed meanwhile after them. */
        remember(v);
    } else {
        tell(v);
    }
    learn_view_frame(&v->learn, &v->s, now_ms);
    if (v->home_down | v->help.done | v->tour) {
        views_flow(v, now_ms);
    }
    if (v->l->settings_dirty) {
        store_frame(v, now_ms);
    }
    /* The tap runs while LIVE or SCENE shows: the audio interrupt pays for it only then. */
    const view* top = shows(v);
    int in_song = song_view_is(top);
    if (in_song) {
        v->song_last = top; /* what SEQ reopens (song_step) */
    }
    song_view_frame(&v->song, in_song);
    if (v->pages.pending) {
        uint8_t previous = v->pages.status;
        (void)song_pages_poll(&v->pages);
        if (previous != v->pages.status && v->pages.status != SONG_PAGES_WAITING &&
            *song_pages_message(&v->pages)) {
            screen_toast(&v->s, song_pages_message(&v->pages), SCREEN_TOAST_MS);
            v->s.popup_ask = 0u;
        }
    }
    toast_hold(v);
    if (v->sids.saving) {
        sid_view_frame(&v->sids, &v->s); /* the question's song saved: the tune */
    }
    v->live.mutes = (uint8_t)song_view_mutes(&v->song);
    live_view_tap(&v->live, top == &live_view_view);
    scene_view_tap(&v->scene, top == &scene_view_view);
    if (v->sc) {
        v->sc->on = (uint8_t)(top == &live_view_view || top == &scene_view_view);
    }
    if (top == &live_view_view || top == &chip_view_view) {
        live_frame(v, top, now_ms);
    }
    screen_frame(&v->s, now_ms);
}

void views_update(views* v, uint32_t step, int32_t code) {
    static const char* const STEP[6] = {"", "PACKAGE", "CHECK HEAD", "LOADER", "CONFIRM",
                                        "RESTART"};
    grid* g = v->s.g;
    const uint8_t a = THEME_ATTR(theme_get(v->l->theme), value);
    widget_frame(g, 2, 18, 26, 5, a, "UPDATE");
    grid_fill(g, 3, 19, 24, 3, ' ', a);
    if (step < 9u) {
        grid_text(g, 4, 20, STEP[step < 6u ? step : 0u], a);
    } else if (code == 1) {
        grid_text(g, 4, 20, "DRY RUN OK", a);
    } else {
        grid_text(g, 4, 20, "FAILED", a);
        widget_number(g, 11, 20, 3, code, 1, a);
    }
    grid_fill(g, 2, STATUS_READY_ROW, 6, 1, ' ', a); /* READY. */
    screen_redraw(&v->s);
}

/* 1 while a question waits for SAVE: MIDI learn's clear-all, the song menu's confirmation, the
 * SIDS browser's SONG NOT SAVED, the SETTINGS store's format once armed. */
static int save_asked(const views* v) {
    if (learn_view_is_open(&v->learn)) {
        return v->learn.ml && v->learn.ml->state == MIDI_LEARN_CONFIRM;
    }
    const view* top = shows(v);
    if (top == &views_song_menu_view) {
        return v->songs.page == SONG_MENU_CONFIRM;
    }
    if (top == &sid_view_browser) {
        return v->sids.asking; /* SONG NOT SAVED: SAVE saves it */
    }
    return top == &views_settings_view && v->format_arm && v->setup.cursor == VIEWS_SETTINGS_STORE;
}

/* The song's place in its beat, 0..255, from the player's row position (the first voice with a
 * row event: its row in the pattern, so a play from mid-pattern, an ORDER jump, a song-position
 * seek or an external clock all stay on the beat) and how far into the row it is (the calls
 * since the row began and the fraction of the current one): rows_beat rows make a beat.
 * PANEL_BEAT_NONE before a row event, or without a player. Integer only. */
static uint32_t beat_phase(const views* v) {
    const song_transport* t = v->song.t;
    song_status st;
    if (!t) {
        return PANEL_BEAT_NONE;
    }
    t->status(t->ctx, &st);
    uint32_t n = 0;
    while (n < SONG_T_VOICES && st.row[n] == SONG_T_NOWHERE) {
        n++;
    }
    uint32_t rb = v->song.rows_beat ? v->song.rows_beat : 4u;
    uint32_t len = ((uint32_t)st.speed[0] + st.speed[1]) * 128u; /* a row, in 256ths of a call */
    if (st.state != SONG_T_PLAYING || n == SONG_T_VOICES || len == 0u) {
        return PANEL_BEAT_NONE;
    }
    uint32_t in_row = ((uint32_t)(uint16_t)(st.now_call - st.row_call[n]) << 8) + st.now_frac;
    uint32_t pos = st.row[n] % rb * len + (in_row < len ? in_row : len - 1u);
    return pos * 256u / (rb * len);
}

void views_leds(views* v, uint32_t now_ms, panel_led_in* out) {
    const panel* p = v->p;
    const view* top = shows(v);
    int in_song = song_view_is(top);
    out->ms = now_ms;
    out->beat = (uint8_t)(p && p->playing ? beat_phase(v) : PANEL_BEAT_NONE);
    out->rec = !v->song.rec ? PANEL_REC_OFF
               : !in_song || !song_view_rec_playing(&v->song) ? PANEL_REC_ARMED
               : song_view_recording(&v->song)                ? PANEL_REC_ON
                                                              : PANEL_REC_COUNT;
    out->silent = (uint8_t)song_view_mute_mask(&v->song);
    out->solo = v->song.solo;
    out->ask = (uint8_t)save_asked(v);
    out->step_on = (uint8_t)(top == &step_view_view);
    out->step_head = PANEL_NO_STEP;
    out->step_set = out->step_rows = 0;
    if (out->step_on) {
        uint32_t set, rows, head;
        step_view_leds(&v->step, &set, &rows, &head);
        out->step_set = (uint16_t)set;
        out->step_rows = (uint16_t)rows;
        if (head != v->led_head) {
            v->led_head = (uint8_t)head;
            v->led_head_ms = now_ms;
        }
        out->step_head = head == STEP_VIEW_NONE ? PANEL_NO_STEP : (uint8_t)head;
    } else {
        v->led_head = STEP_VIEW_NONE;
    }
    out->head_ms = v->led_head_ms;
}
