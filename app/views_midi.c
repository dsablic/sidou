// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

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

static uint8_t stepped(uint32_t x, int32_t steps, uint32_t lo, uint32_t hi) {
    int32_t n = (int32_t)x + steps;
    return (uint8_t)(n < (int32_t)lo ? lo : n > (int32_t)hi ? hi : (uint32_t)n);
}

static void midi_none(char* buf) {
    views_join(buf, MENU_VALUE_CHARS, "-", "");
}

static void mode_value(void* ctx, char* buf) {
    midi_route_setup s;
    if (!midi_setup(ctx, &s)) {
        midi_none(buf);
        return;
    }
    views_join(buf, MENU_VALUE_CHARS, midi_route_mode_name(s.mode), "");
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
    views_number_text(buf, s.base + 1u);
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
    views_join(buf, MENU_VALUE_CHARS, midi_route_option_name(s.option), "");
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
    views_join(buf, MENU_VALUE_CHARS, s.legato ? "ON" : "OFF", "");
}

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
    views_number_text(buf + 1, s.chord[k]);
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
        views_join(buf, MENU_VALUE_CHARS, "CLOCK", "");
    } else {
        views_number_text(buf, s.arp_rate);
        views_append(buf, MENU_VALUE_CHARS, " FR");
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
    views_number_text(buf, s.rows_per_beat);
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
    views_join(buf, MENU_VALUE_CHARS, v->sync ? midi_sync_mode_name(v->sync->mode) : "-", "");
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
    views_join(buf, MENU_VALUE_CHARS, !v->sync ? "-" : v->sync->clock_out ? "ON" : "OFF", "");
}

static void clock_out_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    if (v->sync && steps != 0) {
        midi_sync_set(v->sync, v->sync->mode, steps > 0);
    }
}

const menu_item MIDI_ITEMS[VIEWS_MIDI_ITEMS] = {
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
        views_sound(v, v->midi.cursor != was ? UI_SOUND_BLIP : UI_SOUND_BUZZ);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        uint8_t before[VIEWS_MIDI_LOOK];
        uint8_t after[VIEWS_MIDI_LOOK];
        midi_look(v, before);
        menu_turn(&v->midi, e->steps);
        midi_look(v, after);
        views_sound(v, memcmp(before, after, sizeof before) ? UI_SOUND_CONFIRM : UI_SOUND_BUZZ);
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
        parts |= MENU_PART_BLANK | MENU_PART_TITLE | MENU_PART_HELP;
        if (s->redraw_rows == (1u << GRID_ROWS) - 1u) {
            parts &= ~MENU_PART_LINES;
        }
    } else if (cursor != v->midi_drawn[8] || top != v->midi_drawn[9]) {
        parts |= MENU_PART_TITLE;
    }
    views_big_menu_compose(s, v, &v->midi, "MIDI", g, parts);
    g->scan_row = 0;
    g->scan_col = 0;
    grid_put(g, 0, 0, ' ', (uint8_t)(g->shown_attr[0] ^ 1u));
}

const view views_midi = {"MIDI", NULL, NULL, midi_input, midi_compose, midi_changed};
