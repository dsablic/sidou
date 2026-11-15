// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

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
        return v->format_arm == ARMED_FORMAT ? STORE_LOOK_ARMED : st->mount_newer ? STORE_LOOK_NEWER
                                                                   : STORE_LOOK_FOREIGN;
    }
    if (state != STORE_STATE_BLANK && state != STORE_STATE_SIDOU) {
        return STORE_LOOK_NONE;
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

static void keys_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, v->l->keys_mode == LIVE_KEYS_A ? "A" : "AUTO", "");
}

static void keys_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        live_set_keys_mode(v->l, steps > 0 ? LIVE_KEYS_A : LIVE_KEYS_AUTO);
    }
}

static void sounds_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, v->snd && v->snd->on ? "ON" : "OFF", "");
}

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
            views_join(buf, MENU_VALUE_CHARS, "ERASE ", num);
        } else {
            views_join(buf, MENU_VALUE_CHARS, num, "K");
        }
        break;
    }
    case STORE_LOOK_FOREIGN: views_join(buf, MENU_VALUE_CHARS, "FOREIGN", ""); break;
    case STORE_LOOK_NEWER: views_join(buf, MENU_VALUE_CHARS, "NEWER", ""); break;
    case STORE_LOOK_ARMED: views_join(buf, MENU_VALUE_CHARS, "FORMAT?", ""); break;
    default: views_join(buf, MENU_VALUE_CHARS, "NONE", ""); break;
    }
}

static void reset_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, v->format_arm == ARMED_RESET ? "RESET?" : "", "");
}

static void rows_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, v->song.scroll ? "SCROLL" : "BAR", "");
}

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
    views_join(buf, MENU_VALUE_CHARS, q == SONG_VIEW_QUANT_OFF ? "OFF" : q == SONG_VIEW_QUANT_COUNT ? "COUNT-IN" : "NEAREST", "");
}

static void quant_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        int32_t q = (int32_t)v->song.quant + (steps > 0 ? 1 : -1);
        v->song.quant = (uint8_t)(q < 0 ? 0 : q >= (int32_t)SONG_VIEW_QUANTS ? (int32_t)SONG_VIEW_QUANTS - 1 : q);
    }
}

static void theme_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, theme_get(v->l->theme)->menu_name, "");
}

static void theme_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps != 0) {
        live_set_theme(v->l, steps > 0 ? THEME_C64 : THEME_HIGH_CONTRAST);
        screen_redraw(&v->s);
    }
}

static void spread_value(void* ctx, char* buf) {
    const views* v = ctx;
    char num[4];
    views_number_text(num, v->l->spread);
    views_join(buf, MENU_VALUE_CHARS, v->l->spread ? num : "OFF", v->l->spread ? "%" : "");
}

static void spread_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    int32_t s = ((int32_t)v->l->spread + (steps > 0 ? 0 : 9)) / 10 + (steps > 0 ? 1 : -1);
    if (steps != 0) {
        chip_spread_set(v->l, (uint32_t)(s < 0 ? 0 : s > 10 ? 10 : s) * 10u);
    }
}

static void sid_model_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_join(buf, MENU_VALUE_CHARS, !v->set ? "-" : v->set->sid_model ? "KEYS" : "AUTO", "");
}

static void sid_model_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    uint8_t to = (uint8_t)(steps > 0 ? SETTINGS_SID_KEYS : SETTINGS_SID_AUTO);
    if (steps != 0 && v->set && v->set->sid_model != to) {
        v->set->sid_model = to;
        v->l->settings_dirty = 1;
    }
}

static const char* const BLUETOOTH_MODES[VIEWS_RADIO_MODES] = {"OFF", "ON", "KEYBOARD"};
static char bluetooth_row[VIEWS_DEVICE_LINE + 1u] = "BLUETOOTH";

const char* views_bluetooth_mode(const views* v) {
    return BLUETOOTH_MODES[v->radio->mode(v->radio->ctx)];
}

static uint32_t bluetooth_look(const views* v) {
    if (!v->radio) {
        return 0;
    }
    uint32_t mode = (uint32_t)v->radio->mode(v->radio->ctx);
    const char* name = "";
    uint32_t state = mode == VIEWS_RADIO_KEYBOARD ? v->radio->keyboard(v->radio->ctx, &name) : 0u;
    static const char* const ROWS[] = {"KEYBOARD NONE", "LOOKING", "", "NEEDS PAIRING"};
    views_join(bluetooth_row, sizeof bluetooth_row,
               mode != VIEWS_RADIO_KEYBOARD ? "BLUETOOTH" : ROWS[state],
               state == VIEWS_KEYBOARD_CONNECTED ? name : "");
    return mode << 4 | state;
}

static void bluetooth_value(void* ctx, char* buf) {
    const views* v = ctx;
    int mode = v->radio ? v->radio->mode(v->radio->ctx) : VIEWS_RADIO_KEYBOARD;
    views_join(buf, MENU_VALUE_CHARS, mode == VIEWS_RADIO_KEYBOARD ? "" : BLUETOOTH_MODES[mode],
               "");
}

static void bluetooth_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (steps == 0 || !v->radio) {
        return;
    }
    int32_t mode = v->radio->mode(v->radio->ctx) + (steps > 0 ? 1 : -1);
    int32_t last = VIEWS_RADIO_MODES - 1;
    v->radio->set(v->radio->ctx, mode < 0 ? VIEWS_RADIO_OFF : mode > last ? last : mode);
}

const menu_item SETTINGS_ITEMS[VIEWS_SETTINGS_ITEMS + 1u] = {
    {"KEYS", keys_value, keys_turn},       {"SOUNDS", sounds_value, sounds_turn},
    {"ROWS", rows_value, rows_turn},       {"STORE", store_value, NULL},
    {"THEME", theme_value, theme_turn},
    {"REC", quant_value, quant_turn},      {"SPREAD", spread_value, spread_turn},
    {"SID MODEL", sid_model_value, sid_model_turn},
    {"RESET", reset_value, NULL},
    {bluetooth_row, bluetooth_value, bluetooth_turn},
};
_Static_assert(VIEWS_SETTINGS_KEYS == 0u && VIEWS_SETTINGS_SOUNDS == 1u &&
                   VIEWS_SETTINGS_ROWS == 2u && VIEWS_SETTINGS_STORE == 3u &&
                   VIEWS_SETTINGS_THEME == 4u && VIEWS_SETTINGS_REC == 5u &&
                   VIEWS_SETTINGS_SPREAD == 6u && VIEWS_SETTINGS_SID_MODEL == 7u &&
                   VIEWS_SETTINGS_RESET == 8u && VIEWS_SETTINGS_BLUETOOTH == VIEWS_SETTINGS_ITEMS,
               "the settings' items, BLUETOOTH last and only with a radio");
_Static_assert(LIVE_THEMES == THEME_COUNT, "live.theme holds a theme's id");

static void settings_look(const views* v, uint8_t out[VIEWS_SETTINGS_LOOK]) {
    const live* l = v->l;
    uint32_t kb = 0;
    out[0] = l->keys_mode;
    out[1] = (uint8_t)(v->snd && v->snd->on);
    out[2] = v->song.scroll;
    out[3] = (uint8_t)store_look(v, &kb);
    out[4] = (uint8_t)kb;
    out[5] = l->theme;
    out[6] = v->song.quant;
    out[7] = l->spread;
    out[8] = v->setup.cursor;
    out[9] = v->setup.top;
    out[10] = v->format_arm;
    out[11] = v->set ? v->set->sid_model : 0xffu;
    out[12] = (uint8_t)bluetooth_look(v);
}

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
        v->format_arm = 0;
        views_sound(v, v->setup.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        v->menu_ask = v->setup.cursor;
        int does = v->setup.items[v->setup.cursor].turn != NULL;
        menu_turn(&v->setup, e->steps);
        views_sound(v, does ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
        return does;
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

static void settings_compose(screen* s, void* state, grid* g) {
    views* v = state;
    uint8_t cursor = v->setup_drawn[8];
    uint8_t top = v->setup_drawn[9];
    settings_look(v, v->setup_drawn);
    uint32_t parts = MENU_PART_LINES;
    if (s->redraw) {
        parts |= MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP;
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            parts &= ~MENU_PART_LINES;
        }
    } else if (cursor != v->setup_drawn[8] || top != v->setup_drawn[9]) {
        parts |= MENU_PART_TITLE;
    }
    views_big_menu_compose(s, v, &v->setup, "SETTINGS", g, parts);
    g->scan_row = 0;
    g->scan_col = 0;
    grid_put(g, 0, 0, ' ', (uint8_t)(g->shown_attr[0] ^ 1u));
}

const view views_settings_view = {"SETTINGS", NULL, NULL, settings_input, settings_compose,
                                  settings_changed};

