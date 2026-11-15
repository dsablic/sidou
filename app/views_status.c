// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

#define STATUS_PAGES 3u
#define STATUS_ROW 5u
#define STATUS_KNOB_ROWS 6u
#define STATUS_PAGE_COL 22u
#define LOOK_NOTES 13u

void views_look(const views* v, uint32_t blink, uint8_t out[VIEWS_LOOK]) {
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
    out[n++] = v->home.cursor;
    out[n++] = v->home.top;
    const store* st = v->set ? v->set->st : NULL;
    out[n++] = st ? st->retired : 0u;
    out[n++] = st ? (uint8_t)(st->errors < 99u ? st->errors : 99u) : 0u;
    out[n] = (uint8_t)blink;
}

int views_looks_changed(const views* v, uint32_t blink) {
    uint8_t now[VIEWS_LOOK];
    views_look(v, blink, now);
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        if (now[i] != v->drawn[i]) {
            return 1;
        }
    }
    return 0;
}

static int status_changed(screen* s, void* state) {
    (void)s;
    views* v = state;
    return views_looks_changed(v, 0) || sid_view_status_look(&v->sids, v->tune_drawn, 0);
}

static uint32_t status_notes_row(const live* l) {
    return STATUS_ROW + (l->two_chips ? 7u : 5u) * STATUS_LINE_ROWS;
}

static void status_notes(grid* g, uint8_t attr, const live* l) {
    uint32_t row = status_notes_row(l);
    grid_fill(g, 2, row, GRID_COLS - 4u, 2, ' ', attr);
    uint32_t col = 2;
    for (int k = 0; k < LIVE_VOICES; k++) {
        uint8_t n = l->voice[k].note;
        if (!l->voice[k].gate || n == LIVE_NO_NOTE) {
            continue;
        }
        char t[5];
        uint32_t len = 0;
        t[len++] = screen_note_names[n % 12u][0];
        if (screen_note_names[n % 12u][1] == '#') {
            t[len++] = '#';
        }
        if (n < 12u) {
            t[len++] = '-';
            t[len++] = '1';
        } else {
            t[len++] = (char)('0' + n / 12 - 1);
        }
        t[len] = 0;
        if (col + 2u * len > GRID_COLS - 2u) {
            break;
        }
        grid_text_big(g, col, row, 2, t, attr);
        col += 2u * len + 2u;
    }
}

static void status_line(grid* g, uint8_t lbl, uint8_t val, uint32_t row, const char* label,
                        const char* value) {
    grid_text_big(g, 2, row, 2, label, lbl);
    grid_text_big(g, GRID_COLS - 2u - 2u * grid_text_length(value), row, 2, value, val);
}

static void status_param(grid* g, const theme* th, uint32_t row, const char* label, uint32_t value,
                         uint32_t max, uint8_t color) {
    char num[6];
    views_number_text(num, value);
    status_line(g, THEME_ATTR(th, label), THEME_ATTR(th, value), row, label, num);
    widget_bar(g, 2, row + 2u, GRID_COLS - 4u, value, max, GRID_ATTR(color, th->dim));
}

static void status_chip(grid* g, const theme* th, uint32_t row, const char* label, const char* name,
                        int keys) {
    status_line(g, THEME_ATTR(th, label), THEME_ATTR(th, hot), row, label, keys ? "KEYS" : "");
    grid_text_big(g, 2, row + STATUS_LINE_ROWS, 2, name, THEME_ATTR(th, value));
}

static void status_page_setup(grid* g, const theme* th, const live* l) {
    const uint8_t a = THEME_ATTR(th, value);
    const uint8_t lbl = THEME_ATTR(th, label);
    uint32_t row = STATUS_ROW;
    if (l->two_chips) {
        for (uint32_t x = 0; x < 2u; x++, row += 2u * STATUS_LINE_ROWS) {
            uint32_t id = x == l->keys_chip ? l->profile_req : l->other_profile;
            char name[SCREEN_POPUP_TEXT] = "--";
            if (sid_chip_profile_get(id)) {
                views_profile_label(name, id);
            }
            status_chip(g, th, row, x ? "CHIP B" : "CHIP A", name, x == l->keys_chip);
        }
    } else {
        char name[SCREEN_POPUP_TEXT];
        views_profile_label(name, l->profile_req);
        status_chip(g, th, row, "CHIP", name, 0);
        row += 2u * STATUS_LINE_ROWS;
    }
    const live_preset* p = live_preset_get(l->preset);
    char oct[3];
    views_octave_text(oct, l->octave);
    grid_text_big(g, 2, row, 2, "PRESET", lbl);
    grid_text_big(g, 16, row, 2, "OCT", lbl);
    grid_text_big(g, 24, row, 2, oct, a);
    grid_text_big(g, 2, row + STATUS_LINE_ROWS, 2, l->user_sound ? l->base_name : p ? p->name : "?", a);
    row += 2u * STATUS_LINE_ROWS;
    grid_text_big(g, 2, row, 2, "NOTES", lbl);
    status_notes(g, a, l);
}

static void status_page_effects(grid* g, const theme* th, const live* l) {
    const uint8_t a = THEME_ATTR(th, value);
    const uint8_t lbl = THEME_ATTR(th, label);
    char num[6];
    uint32_t row = STATUS_ROW;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++, row += 2u * STATUS_LINE_ROWS) {
        if (k == l->fx_select) {
            grid_text_big(g, 2, row, 2, ">", THEME_ATTR(th, hot));
        }
        grid_text_big(g, 4, row, 2, SLOT_NAMES[k], lbl);
        views_number_text(num, l->slot[k][LIVE_SLOT_MIX]);
        grid_text_big(g, GRID_COLS - 2u - 2u * grid_text_length(num), row, 2, num, a);
        grid_text_big(g, 14, row, 2, "MIX", lbl);
        grid_text_big(g, 4, row + STATUS_LINE_ROWS, 2,
                      views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]), a);
    }
    const uint8_t* sel = l->slot[l->fx_select];
    grid_text_big(g, 2, row, 2, SLOT_NAMES[l->fx_select], lbl);
    grid_text_big(g, 12, row, 2, "AMT", lbl);
    views_number_text(num, sel[LIVE_SLOT_AMOUNT]);
    grid_text_big(g, GRID_COLS - 2u - 2u * grid_text_length(num), row, 2, num, a);
    row += STATUS_LINE_ROWS;
    views_number_text(num, sel[LIVE_SLOT_A]);
    grid_text_big(g, 2, row, 2, "A", lbl);
    grid_text_big(g, 6, row, 2, num, a);
    views_number_text(num, sel[LIVE_SLOT_B]);
    grid_text_big(g, 14, row, 2, "B", lbl);
    grid_text_big(g, 18, row, 2, num, a);
}

static int status_input(screen* s, void* state, const screen_event* e) {
    views* v = state;
    if (e->kind != SCREEN_TURN || e->id != PANEL_ENC_SELECT) {
        return 0;
    }
    v->status_page = (uint8_t)((v->status_page + (e->steps > 0 ? 1u : STATUS_PAGES - 1u)) % STATUS_PAGES);
    screen_redraw(s);
    return 1;
}

static void status_store(grid* g, const theme* th, uint32_t retired, uint32_t errors) {
    char t[24] = "";
    char n[6];
    if (retired) {
        views_append(t, sizeof t, "RETIRED ");
        views_number_text(n, retired);
        views_append(t, sizeof t, n);
    }
    if (errors) {
        views_append(t, sizeof t, retired ? " ERR " : "STORE ERR ");
        views_number_text(n, errors);
        views_append(t, sizeof t, n);
    }
    if (retired | errors) {
        views_put(g, THEME_ATTR(th, hot), STATUS_READY_ROW, GRID_COLS - 1u - grid_text_length(t), t);
    }
}

static void status_compose(screen* s, void* state, grid* g) {
    views* v = state;
    const live* l = v->l;
    const theme* th = theme_get(l->theme);
    const uint8_t a = THEME_ATTR(th, value);
    const uint8_t lbl = THEME_ATTR(th, label);
    const uint32_t page = v->status_page;
    uint8_t now[VIEWS_LOOK];
    views_look(v, 0, now);
    uint32_t other = 0;
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        other |= (uint32_t)(now[i] != v->drawn[i]) & (uint32_t)!(i >= LOOK_NOTES && i < LOOK_NOTES + LIVE_VOICES);
    }
    for (uint32_t i = 0; i < VIEWS_LOOK; i++) {
        v->drawn[i] = now[i];
    }
    int tune_was = v->tune_drawn[0] != SID_T_NONE;
    int tune_moved = sid_view_status_look(&v->sids, v->tune_drawn, 1);
    int tune = v->tune_drawn[0] != SID_T_NONE;
    if (!other && !s->redraw && tune == tune_was) {
        if (page == 1u) {
            status_notes(g, a, l);
        } else if (page == 0u && tune && tune_moved) {
            sid_view_status_draw(&v->sids, g, th, STATUS_ROW);
        }
        return;
    }
    const uint8_t ring = THEME_ATTR(th, dim);
    grid_fill(g, 0, 0, GRID_COLS, 1, FONT_FULL, ring);
    grid_fill(g, 0, GRID_ROWS - 1u, GRID_COLS, 1, FONT_FULL, ring);
    grid_fill(g, 0, 1, 1, GRID_ROWS - 2u, FONT_FULL, ring);
    grid_fill(g, GRID_COLS - 1u, 1, 1, GRID_ROWS - 2u, FONT_FULL, ring);
    grid_fill(g, 1, 1, GRID_COLS - 2u, GRID_ROWS - 2u, ' ', a);
    grid_text_big(g, 2, 2, 2, "SIDOU", THEME_ATTR(th, hot));
    views_put(g, lbl, 2, VERSION_END_COL - grid_text_length(v->version), v->version);
    const char pages[4] = {(char)('1' + page), '/', (char)('0' + STATUS_PAGES), 0};
    grid_text_big(g, STATUS_PAGE_COL, 2, 2, pages, a);

    if (page == 0u) {
        if (tune) {
            sid_view_status_draw(&v->sids, g, th, STATUS_ROW);
        } else {
            status_param(g, th, STATUS_ROW, "CUTOFF", l->cutoff, LIVE_CUTOFF_MAX, th->knob[0]);
            status_param(g, th, STATUS_ROW + STATUS_KNOB_ROWS, "RESO", l->resonance,
                         LIVE_RESONANCE_MAX, th->knob[1]);
            status_param(g, th, STATUS_ROW + 2u * STATUS_KNOB_ROWS, "PULSE", l->pulse_width,
                         LIVE_PULSE_WIDTH_MAX, th->knob[2]);
            status_param(g, th, STATUS_ROW + 3u * STATUS_KNOB_ROWS, "VOLUME", l->volume, 100,
                         th->value);
        }
    } else if (page == 1u) {
        status_page_setup(g, th, l);
    } else {
        status_page_effects(g, th, l);
    }
#if defined(SIDOU_BLE)
    char report[GRID_COLS];
    if (v->radio) {
        v->radio->report(v->radio->ctx, report);
    }
    views_put(g, lbl, STATUS_READY_ROW, 2, v->radio ? report : "READY.");
#else
    views_put(g, lbl, STATUS_READY_ROW, 2, "READY.");
#endif
    status_store(g, th, v->drawn[VIEWS_LOOK - 3u], v->drawn[VIEWS_LOOK - 2u]);
}

const view views_status = {"STATUS", NULL, NULL, status_input, status_compose, status_changed};
