// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

#define DEVICES_NAME_CHARS 10u
#define DEVICES_BARS 3u
#define DEVICES_WAIT_MS 8000u
#define DEVICES_BAR_STEP 3u

enum { DEVICES_LISTED, DEVICES_SCANNING, DEVICES_NONE_FOUND };

static const char* const DEVICES_EMPTY[3][2] = {
    {"", ""}, {"SCANNING", ""}, {"NO KEYBOARDS", "FOUND"}};
static const char* const DEVICES_HELP[2][2] = {
    {"SELECT  MOVE", "SAVE CONNECT"}, {"SAVE    SCAN", "HOME    BACK"}};

static void device_value(void* ctx, char* buf) {
    (void)ctx;
    buf[0] = 0;
}

static void device_line(char* line, const char* name, uint32_t bars) {
    uint32_t i = 0;
    for (; i < DEVICES_NAME_CHARS && name[i]; i++) {
        line[i] = name[i];
    }
    while (i < VIEWS_DEVICE_LINE - DEVICES_BARS) {
        line[i++] = ' ';
    }
    for (uint32_t b = 0; b < DEVICES_BARS; b++) {
        line[i++] = (char)(b < bars ? FONT_V2 + DEVICES_BAR_STEP * b : FONT_V1);
    }
    line[i] = 0;
}

static void devices_read(views* v) {
    const char* names[VIEWS_DEVICES];
    uint8_t bars[VIEWS_DEVICES];
    uint32_t n = v->radio->devices(v->radio->ctx, v->devices_fresh, names, bars);
    v->devices_fresh = 0;
    for (uint32_t k = 0; k < n; k++) {
        device_line(v->device_lines[k], names[k], bars[k]);
    }
    v->devices.count = (uint8_t)n;
    if (v->devices.cursor >= n) {
        v->devices.cursor = (uint8_t)(n ? n - 1u : 0u);
    }
    v->devices_empty = n ? DEVICES_LISTED
                         : v->s.now - v->devices_at < DEVICES_WAIT_MS ? DEVICES_SCANNING
                                                                      : DEVICES_NONE_FOUND;
}

void views_devices_open(views* v) {
    for (uint32_t k = 0; k < VIEWS_DEVICES; k++) {
        v->device_items[k] = (menu_item){v->device_lines[k], device_value, NULL};
    }
    menu_init(&v->devices, v->device_items, 0, VIEWS_MENU_SHOWN, v);
    v->devices.scale = 2;
    v->devices.lines = MIDI_LINES;
    v->devices_fresh = 1;
    v->devices_at = v->s.now;
    views_sound(v, UI_SOUND_CONFIRM);
    screen_push(&v->s, &views_devices_view, v, SCREEN_WIPE_DOWN);
}

void views_devices_save(views* v) {
    views_sound(v, UI_SOUND_CONFIRM);
    if (!v->devices.count) {
        v->devices_fresh = 1;
        v->devices_at = v->s.now;
        return;
    }
    v->radio->pick(v->radio->ctx, v->devices.cursor);
    screen_pop(&v->s, SCREEN_WIPE_UP);
}

void views_keyboard_event(views* v, uint32_t event, const char* name) {
    char text[SCREEN_TOAST_CHARS + 1u];
    int passkey = event == VIEWS_KEYBOARD_PASSKEY;
    views_join(text, sizeof text,
               event == VIEWS_KEYBOARD_PAIRING ? "NEEDS PAIRING"
               : passkey                       ? "PASSKEY "
                                               : "KEYBOARD ",
               event == VIEWS_KEYBOARD_CONNECTED || passkey ? name
               : event == VIEWS_KEYBOARD_LOST               ? "LOST"
                                                            : "");
    screen_toast(&v->s, text, passkey ? VIEWS_PASSKEY_MS : SCREEN_TOAST_MS);
}

static int devices_input(screen* s, void* state, const screen_event* e) {
    (void)s;
    views* v = state;
    if (e->kind != SCREEN_TURN || (e->id != PANEL_ENC_SELECT && e->id != PANEL_ENC_ALGORITHM)) {
        return 0;
    }
    uint8_t was = v->devices.cursor;
    menu_move(&v->devices, e->steps);
    views_sound(v, v->devices.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
    return 1;
}

static void devices_compose(screen* s, void* state, grid* g) {
    views* v = state;
    devices_read(v);
    views_big_menu_compose(s, v, &v->devices, "DEVICES", g,
                           MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_LINES);
    const theme* th = theme_get(v->l->theme);
    const char* const* empty = DEVICES_EMPTY[v->devices_empty];
    const char* const* help = DEVICES_HELP[v->devices_empty != DEVICES_LISTED];
    for (uint32_t r = 0; r < 2u; r++) {
        if (*empty[r]) {
            grid_text_big(g, 1, MENU_ROW + 3u * r, 2, empty[r], GRID_ATTR(th->menu_item, th->bg));
        }
        grid_text_big(g, 1, MENU_HELP_ROW + 2u * r, 2, help[r], THEME_ATTR(th, hint));
    }
}

const view views_devices_view = {"DEVICES", NULL, NULL, devices_input, devices_compose, NULL};
