// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_view.h"

#include <string.h>

#include "chip_view.h"
#include "font.h"
#include "lab_draw.h"
#include "lab_song.h"
#include "live_dice.h"
#include "live_view.h"
#include "panel.h"
#include "theme.h"
#include "views.h"

static const uint8_t KNOB_COL[4] = {0, 8, 15, 22};
static const uint8_t KNOB_W[4] = {8, 7, 7, 8};
#define PLACE_COL 24u
#define ALG_COL 27u
#define ALL_COL 20u

_Static_assert(LAB_VIEW_ART_ROW + LAB_VIEW_ART_ROWS < LAB_VIEW_CHOICE_ROW &&
                   LAB_VIEW_CHOICE_ROW + 2u < LAB_VIEW_LABEL_ROW &&
                   LAB_VIEW_LABEL_ROW + 1u == LAB_VIEW_VALUE_ROW &&
                   LAB_VIEW_VALUE_ROW + 3u == LAB_VIEW_BAR_ROW &&
                   LAB_VIEW_BAR_ROW + 1u == LAB_VIEW_TAG_ROW &&
                   LAB_VIEW_TAG_ROW < LAB_VIEW_HINT_ROW && LAB_VIEW_HINT_ROW < GRID_ROWS,
               "the page's bands follow each other");
_Static_assert(PANEL_PAGE_LAB_END - PANEL_PAGE_LAB_FIRST == LAB_PAGES, "a panel page a LAB page");
_Static_assert(LIVE_PARAM_ATTACK + 3 == LIVE_PARAM_RELEASE &&
                   LIVE_PARAM_VIBRATO_RATE + 2 == LIVE_PARAM_VIBRATO_DELAY,
               "ENVELOPE's and MOD's cards are consecutive parameters");

static const char* const PAGE_NAMES[LAB_PAGES] = {"OSC", "ENVELOPE", "FILTER", "MOD", "FX",
                                                  "MACRO"};

static const uint8_t WAVES[LAB_VIEW_WAVES] = {0x10, 0x20, 0x40, 0x80, 0x30, 0x50,
                                              0x60, 0x70, 0x14, 0x22, 0x42};
static const char* const WAVE_NAMES[LAB_VIEW_WAVES] = {
    "TRIANGLE",  "SAW",       "PULSE",       "NOISE",    "TRI+SAW",  "TRI+PULSE",
    "SAW+PULSE", "TRI+SAW+PUL", "TRI RING", "SAW SYNC", "PULSE SYNC"};
#define WAVE_BITS 0xF6u
static const uint8_t MODES[4] = {1, 2, 4, 5};
static const char* const MODE_NAMES[8] = {"OFF", "LP",    "BP",    "LP+BP",
                                          "HP",  "NOTCH", "BP+HP", "ALL"};
static const char* const ARP_NAMES[5] = {"ARP OFF", "ARP UP", "ARP DOWN", "ARP UP-DOWN",
                                         "ARP PLAYED"};
static const char* const HINTS[LAB_VIEW_HINTS] = {
    "EDIT A/B  +LFO UNDO  +ARP VARY",
    "EDIT +SCL ROLL  +SAVE NAME",
    "SAVE KEEP  SELECT NEXT PAGE",
    "EDIT+KEY PEEK A  REC SEND",
};

const char* lab_view_page_name(uint32_t page) {
    return page < LAB_PAGES ? PAGE_NAMES[page] : "";
}

const char* lab_view_hint(uint32_t n) {
    return n < LAB_VIEW_HINTS ? HINTS[n] : "";
}

uint8_t lab_view_wave_control(uint32_t i) {
    return i < LAB_VIEW_WAVES ? WAVES[i] : WAVES[0];
}

uint32_t lab_view_wave_of(uint8_t control) {
    uint32_t c = control & WAVE_BITS;
    for (uint32_t i = 0; i < LAB_VIEW_WAVES; i++) {
        if (WAVES[i] == c) {
            return i;
        }
    }
    for (uint32_t i = 0; i < 8u; i++) {
        if (WAVES[i] == (c & 0xF0u)) {
            return i;
        }
    }
    return 0;
}

static char* number(char* out, int32_t n, int sign) {
    if (n < 0) {
        *out++ = '-';
        n = -n;
    } else if (sign && n > 0) {
        *out++ = '+';
    }
    return out + screen_decimal(out, (uint32_t)n);
}

static void tenths(char* out, uint32_t t) {
    if (t == 0u) {
        out[0] = '0';
        out[1] = 0;
    } else if (t < 100u) {
        out[0] = (char)('0' + t / 10u);
        out[1] = '.';
        out[2] = (char)('0' + t % 10u);
        out[3] = 0;
    } else {
        (void)number(out, (int32_t)((t + 5u) / 10u), 0);
    }
}

static void lfo_hz(char* out, uint32_t step, int brief) {
    uint32_t c = (step * 10000u + 32768u) >> 16;
    if (c >= 100u || c % 10u == 0u) {
        tenths(out, (c + 5u) / 10u);
        return;
    }
    if (!brief) {
        *out++ = '0';
    }
    out[0] = '.';
    out[1] = (char)('0' + c / 10u);
    out[2] = (char)('0' + c % 10u);
    out[3] = 0;
}

static void copy(char* out, const char* s) {
    uint32_t n = 0;
    for (; s[n] && n + 1u < LAB_VIEW_TEXT; n++) {
        out[n] = s[n];
    }
    out[n] = 0;
}

static void pulse_percent(char* out, uint32_t pw) {
    uint32_t side = pw > 2048u ? 4096u - pw : pw;
    char* e = number(out, (int32_t)((side * 99u + 2047u) / 4095u), 0);
    e[0] = '%';
    e[1] = 0;
}

static char* append(char* out, const char* s);
static int ratio_card(const live* l) {
    return l->mono_sync && !(l->control & 0x40u);
}

int lab_view_card(const lab_view* lv, uint32_t page, uint32_t k, char* name, char* value,
                  uint32_t* fill, uint32_t* max) {
    const live* l = lv->l;
    name[0] = 0;
    value[0] = '-';
    value[1] = 0;
    *fill = 0;
    *max = 0;
    uint32_t param = panel_lab_param(l, page, k);
    uint32_t v = param < LIVE_PARAM_COUNT ? live_param_get(l, param) : 0u;
    if (param < LIVE_PARAM_COUNT) {
        *fill = v;
        *max = live_param_max(param);
    }
    switch (page) {
    case LAB_PAGE_OSC:
        if (k == 0u) {
            if (ratio_card(l)) {
                copy(name, "RATIO");
                (void)number(value, l->sync_semis, 1);
                *fill = (uint32_t)(l->sync_semis + LIVE_RATIO_SEMIS);
                *max = 2u * LIVE_RATIO_SEMIS;
            } else {
                copy(name, "PULSE");
                pulse_percent(value, v);
                *fill = v > 2048u ? 4096u - v : v;
                *max = 2048u;
            }
        } else if (k == 1u) {
            copy(name, "PWM");
            v = live_pwm_depth_effective(l);
            *fill = v;
            char* e = number(value, (int32_t)((v * 100u + 2048u) / 4096u), 0);
            e[0] = '%';
            e[1] = 0;
        } else if (k == 2u) {
            copy(name, "PWM HZ");
            lfo_hz(value, l->pwm_rate, 0);
        } else {
            copy(name, "TUNE");
            (void)number(value, (int32_t)v - LIVE_TUNE_CENTS, 1);
        }
        return 1;
    case LAB_PAGE_ENVELOPE: {
        static const char* const NAMES[4] = {"ATTACK", "DECAY", "SUS", "RELEASE"};
        copy(name, NAMES[k & 3u]);
        (void)number(value, (int32_t)v, 0);
        return 1;
    }
    case LAB_PAGE_FILTER:
        if (k == 3u) {
            char hz[6];
            lfo_hz(hz, l->pwm_rate, 1);
            char* e = append(name, grid_text_length(hz) < 3u ? "LFO " : "LFO");
            e = append(append(e, hz), "HZ");
            *e = 0;
            (void)number(value, (int32_t)v, 0);
            return 1;
        }
        if (k < 2u) {
            copy(name, k ? "RESO" : "CUTOFF");
            live_view_knob_text(l, k, value);
            *fill = k ? l->res_out : l->cutoff_out;
        } else {
            copy(name, "VOICES");
            uint32_t n = 0;
            for (uint32_t b = 0; b < 3u; b++) {
                if (v >> b & 1u) {
                    value[n++] = (char)('1' + b);
                }
            }
            if (n == 0u) {
                value[n++] = '-';
            }
            value[n] = 0;
        }
        return 1;
    case LAB_PAGE_MOD:
        if (k == 0u) {
            copy(name, "VIB HZ");
            tenths(value, v);
        } else if (k == 3u) {
            copy(name, "RATE");
            if (v >= LIVE_ARP_RATE_CLOCK) {
                char* e = append(value, "1/");
                *number(e, (int32_t)(96u / live_arp_steps[v % LIVE_ARP_RATES]), 0) = 0;
            } else {
                *append(number(value, live_arp_steps[v], 0), "FR") = 0;
            }
            return l->arp_on;
        } else {
            copy(name, k == 1u ? "DEPTH" : "DELAY");
            (void)number(value, (int32_t)v, 0);
        }
        return 1;
    case LAB_PAGE_MACRO:
        if (k >= 2u) {
            return 0;
        }
        copy(name, k ? "PWM LNK" : "RES LNK");
        (void)number(value, (int32_t)v - LIVE_LINK_MAX, 1);
        return 1;
    default: {
        uint32_t slot = l->fx_select % LIVE_SLOTS;
        char label[CHIP_VIEW_LABEL];
        int used = chip_view_label(slot, l->slot[slot][LIVE_SLOT_ALGORITHM], LIVE_SLOT_AMOUNT + k,
                                   label);
        copy(name, label);
        (void)number(value, (int32_t)v, 0);
        return used;
    }
    }
}

void lab_view_choice(const lab_view* lv, uint32_t page, char* out) {
    const live* l = lv->l;
    switch (page) {
    case LAB_PAGE_OSC: {
        uint32_t c = l->control & WAVE_BITS;
        uint32_t i = lab_view_wave_of(l->control);
        if (WAVES[i] == c) {
            copy(out, WAVE_NAMES[i]);
            return;
        }
        static const char* const PART[4] = {"TRI", "SAW", "PUL", "NOI"};
        char buf[24];
        uint32_t n = 0;
        for (uint32_t b = 0; b < 4u; b++) {
            if (c >> (4u + b) & 1u) {
                if (n) {
                    buf[n++] = '+';
                }
                for (const char* p = PART[b]; *p; p++) {
                    buf[n++] = *p;
                }
            }
        }
        if (n == 0u) {
            for (const char* p = "NONE"; *p; p++) {
                buf[n++] = *p;
            }
        }
        for (const char* p = c & 0x04u ? " RING" : c & 0x02u ? " SYNC" : ""; *p; p++) {
            buf[n++] = *p;
        }
        buf[n] = 0;
        copy(out, buf);
        return;
    }
    case LAB_PAGE_ENVELOPE:
        copy(out, l->envelope_mode ? "AUTHENTIC" : "INSTANT");
        return;
    case LAB_PAGE_FILTER:
        copy(out, MODE_NAMES[l->filter_mode >> 4 & 7u]);
        return;
    case LAB_PAGE_MOD:
        copy(out, ARP_NAMES[l->arp_on ? 1u + (l->arp_order & 3u) : 0u]);
        return;
    case LAB_PAGE_MACRO:
        copy(out, l->reso_link || l->pwm_link ? "LINKED" : "NO LINKS");
        return;
    default: {
        uint32_t slot = l->fx_select % LIVE_SLOTS;
        copy(out, views_algorithm_name(slot, l->slot[slot][LIVE_SLOT_ALGORITHM]));
        return;
    }
    }
}

static char* slot_label(char* out, uint32_t n) {
    out[0] = 'U';
    out[1] = (char)('0' + (n + 1u) / 10u);
    out[2] = (char)('0' + (n + 1u) % 10u);
    return out + 3;
}

static char* append(char* out, const char* s) {
    while (*s) {
        *out++ = *s++;
    }
    return out;
}

static void auto_name(char* out, uint32_t n) {
    char* e = append(out, "LAB ");
    e[0] = (char)('0' + (n + 1u) / 10u);
    e[1] = (char)('0' + (n + 1u) % 10u);
    e[2] = 0;
}

typedef grid_canvas canvas;

static void put(canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        c->ch[row][col] = ch;
        c->attr[row][col] = attr;
    }
}

static uint32_t text(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++) {
        put(c, col++, row, (uint8_t)*s, attr);
    }
    return col;
}

static int choice_off(const lab_view* lv) {
    const live* l = lv->l;
    switch (lv->page) {
    case LAB_PAGE_FILTER: return (l->filter_mode >> 4 & 7u) == 0u;
    case LAB_PAGE_MOD: return !l->arp_on;
    case LAB_PAGE_FX: return l->slot[l->fx_select % LIVE_SLOTS][LIVE_SLOT_ALGORITHM] == 0u;
    case LAB_PAGE_MACRO: return !l->reso_link && !l->pwm_link;
    default: return 0;
    }
}

static void draw_card(const lab_view* lv, canvas* c, const theme* th, uint32_t k) {
    char name[LAB_VIEW_TEXT];
    char value[LAB_VIEW_TEXT];
    uint32_t fill;
    uint32_t max;
    int used = lab_view_card(lv, lv->page, k, name, value, &fill, &max);
    uint32_t col = KNOB_COL[k];
    uint32_t cells = KNOB_W[k] - 1u;
    uint8_t accent = used ? th->knob[k] : th->dim;
    uint8_t na = !used ? THEME_ATTR(th, dim)
                 : lv->hl == k + 1u || lv->hl_link == k + 1u ? GRID_ATTR(th->bg, th->knob_text[k])
                                                             : GRID_ATTR(th->knob_text[k], th->bg);
    text(c, col, LAB_VIEW_LABEL_ROW, name, na);
    uint8_t va = GRID_ATTR(used ? th->value : th->dim, th->bg);
    if (grid_text_length(value) <= 2u) {
        (void)grid_canvas_text_big(c, col, LAB_VIEW_VALUE_ROW, 3, value, va);
    } else {
        (void)grid_canvas_text_big(c, col, LAB_VIEW_VALUE_ROW + 1u, 2, value, va);
    }
    uint32_t px = max ? (fill > max ? max : fill) * (cells * 8u) / max : 0u;
    for (uint32_t i = 0; i < cells; i++) {
        uint32_t f = px >= 8u ? 8u : px;
        px -= f;
        put(c, col + i, LAB_VIEW_BAR_ROW, (uint8_t)(FONT_BAR0 + f), GRID_ATTR(accent, th->dim));
    }
    char tag[3] = {'K', (char)('1' + k), 0};
    text(c, col, LAB_VIEW_TAG_ROW, tag, THEME_ATTR(th, hint));
}

static uint32_t hint_phase(uint32_t now) {
    return now / LAB_VIEW_HINT_MS % LAB_VIEW_HINTS;
}

static uint32_t look(const lab_view* lv, uint32_t now) {
    const live* l = lv->l;
    uint8_t b[LAB_SOUND_BYTES + LIVE_PRESET_NAME_CHARS + 12u + 4u + LAB_SOUND_NAME];
    live_sound_get(l, b);
    uint32_t n = LAB_SOUND_BYTES;
    char name[LIVE_PRESET_NAME_CHARS + 2u];
    live_preset_name(l, name);
    for (uint32_t i = 0; name[i] && i < LIVE_PRESET_NAME_CHARS + 1u; i++) {
        b[n++] = (uint8_t)name[i];
    }
    b[n++] = lv->page;
    b[n++] = l->theme;
    b[n++] = (uint8_t)(lv->hl | lv->hl_link << 4);
    b[n++] = (uint8_t)hint_phase(now);
    b[n++] = (int32_t)(lv->sent_until - now) > 0;
    b[n++] = l->envelope_mode;
    b[n++] = l->fx_select;
    b[n++] = (uint8_t)l->model;
    b[n++] = (uint8_t)l->cutoff_out;
    b[n++] = (uint8_t)(l->cutoff_out >> 8);
    b[n++] = (uint8_t)(lv->ab | lv->peek << 4);
    if (lv->mode) {
        b[n++] = lv->mode;
        b[n++] = (uint8_t)(lv->slot | (l->sounds_valid >> lv->slot & 1u) << 7);
        b[n++] = lv->cursor_at;
        for (uint32_t i = 0; i < LAB_SOUND_NAME; i++) {
            b[n++] = (uint8_t)lv->name[i];
        }
    }
    uint32_t h = 2166136261u;
    for (uint32_t i = 0; i < n; i++) {
        h = (h ^ b[i]) * 16777619u;
    }
    return h;
}

static void build_name(lab_view* lv, canvas* c, const theme* th) {
    const live* l = lv->l;
    uint32_t n = lv->slot;
    uint8_t used = (uint8_t)(l->sounds_valid >> n & 1u);
    (void)grid_canvas_text_big(c, 0, 0, 2, "KEEP AS", THEME_ATTR(th, hot));
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_SLOT_ROW, 2, "SLOT", THEME_ATTR(th, label));
    char slot[4];
    (void)slot_label(slot, n);
    slot[3] = 0;
    (void)grid_canvas_text_big(c, 10, LAB_VIEW_NAME_SLOT_ROW, 2, slot, GRID_ATTR(th->knob[0], th->bg));
    (void)grid_canvas_text_big(c, 18, LAB_VIEW_NAME_SLOT_ROW, 2, used ? "USED" : "FREE",
                               used ? THEME_ATTR(th, alert) : THEME_ATTR(th, dim));
    if (used) {
        char old[LAB_SOUND_NAME + 1u];
        uint8_t sound[LAB_SOUND_BYTES];
        if (user_sound_read(lv->us, n, old, sound)) {
            (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_USED_ROW, 2, old, THEME_ATTR(th, alert));
        }
    }
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_LABEL_ROW, 2, "NAME", THEME_ATTR(th, label));
    for (uint32_t i = 0; i < LAB_SOUND_NAME; i++) {
        char ch[2] = {lv->name[i], 0};
        (void)grid_canvas_text_big(c, 2u * i + 3u, LAB_VIEW_NAME_CHARS_ROW, 2, ch,
                                   i == lv->cursor_at ? GRID_ATTR(th->bg, th->value)
                                                      : THEME_ATTR(th, value));
    }
    static const char* const LEGEND[5] = {"KEYS TYPE", "SELECT CURSOR", "ALG CHARACTER",
                                          "K1 SLOT", "SAVE WRITE"};
    for (uint32_t i = 0; i < 5u; i++) {
        (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_LEGEND_ROW + 2u * i, 2, LEGEND[i],
                                   THEME_ATTR(th, hint));
    }
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_LEGEND_ROW + 10u, 2, "HOME BACK",
                               THEME_ATTR(th, hint));
    if (lv->mode == LAB_VIEW_MODE_ASK) {
        char ask[16] = "OVERWRITE U00?";
        (void)slot_label(&ask[10], n);
        ask[13] = '?';
        (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_ASK_ROW, 2, ask, THEME_ATTR(th, alert));
        (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_ASK_ROW + 2u, 2, "SAVE AGAIN",
                                   THEME_ATTR(th, alert));
    }
}

static void build(lab_view* lv, uint32_t now) {
    canvas* c = &lv->canvas->cv;
    const live* l = lv->l;
    const theme* th = theme_get(l->theme);
    lv->look = look(lv, now);
    memset(c->ch, ' ', sizeof c->ch);
    memset(c->attr, THEME_ATTR(th, value), sizeof c->attr);
    if (lv->mode) {
        build_name(lv, c, th);
        lv->built = 1;
        lv->copying = 1;
        return;
    }
    (void)grid_canvas_text_big(c, 0, 0, 2, PAGE_NAMES[lv->page], THEME_ATTR(th, hot));
    char place[4] = {(char)('1' + lv->page), '/', (char)('0' + LAB_PAGES), 0};
    (void)grid_canvas_text_big(c, PLACE_COL, 0, 2, place, THEME_ATTR(th, hint));
    char name[LIVE_PRESET_NAME_CHARS + 2u];
    live_preset_name(l, name);
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_ROW, 2, name, THEME_ATTR(th, value));
    if (lv->peek) {
        (void)grid_canvas_text_big(c, LAB_VIEW_AB_COL, LAB_VIEW_NAME_ROW, 2, "A",
                                   GRID_ATTR(th->hot, th->bg));
    } else if (lv->ab) {
        char ab[2] = {lv->ab == LAB_VIEW_AB_A ? 'A' : 'B', 0};
        (void)grid_canvas_text_big(c, LAB_VIEW_AB_COL, LAB_VIEW_NAME_ROW, 2, ab,
                                   GRID_ATTR(th->bg, th->hot));
    }
    lab_draw_guides(c, l, lv->page, lv->hl, th, LAB_VIEW_ART_ROW);
    char choice[LAB_VIEW_TEXT];
    lab_view_choice(lv, lv->page, choice);
    uint8_t ca = lv->hl == LAB_VIEW_HL_CHOICE ? GRID_ATTR(th->bg, th->value)
                 : choice_off(lv)            ? THEME_ATTR(th, dim)
                                             : THEME_ATTR(th, value);
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_CHOICE_ROW, 2, choice, ca);
    text(c, ALG_COL, LAB_VIEW_CHOICE_ROW, "ALG", THEME_ATTR(th, hint));
    if (lv->page == LAB_PAGE_ENVELOPE) {
        text(c, ALL_COL, LAB_VIEW_CHOICE_ROW + 1u, "ALL SOUNDS", THEME_ATTR(th, hint));
    }
    for (uint32_t k = 0; k < 4u; k++) {
        draw_card(lv, c, th, k);
    }
    char hint[GRID_COLS + 1u];
    text(c, 0, LAB_VIEW_HINT_ROW, lab_view_hint_text(lv, now, hint), THEME_ATTR(th, hint));
    lv->built = 1;
    lv->copying = 1;
}

static void expire(lab_view* lv, const screen* s) {
    if ((lv->hl | lv->hl_link) && (int32_t)(s->now - lv->hl_until) >= 0) {
        lv->hl = 0;
        lv->hl_link = 0;
    }
}

static void a_sound(const live* l, uint8_t out[LAB_SOUND_BYTES]) {
    memcpy(out, l->base, LAB_SOUND_BYTES);
    if (l->bend_range_rpn) {
        out[LAB_SOUND_BEND_RANGE] = l->bend_range;
    }
}

static void sync(lab_view* lv) {
    if (lv->loads != lv->l->loads) {
        lv->loads = lv->l->loads;
        lv->ab = LAB_VIEW_AB_OFF;
        lv->peek = 0;
        lab_edit_clear(&lv->undo);
        lv->run = LAB_VIEW_NO_RUN;
    }
}

static void record(lab_view* lv, const uint8_t before[LAB_EDIT_BYTES], uint32_t run) {
    uint8_t after[LAB_EDIT_BYTES];
    lab_edit_get(lv->l, after);
    int merge = run != LAB_VIEW_NO_RUN && run == lv->run;
    lv->run = lab_edit_push(&lv->undo, before, after, merge) ? (uint8_t)run : LAB_VIEW_NO_RUN;
}

static void a_becomes_edit(lab_view* lv) {
    sync(lv);
    if (lv->ab == LAB_VIEW_AB_A) {
        uint8_t b[LAB_EDIT_BYTES];
        memcpy(b, lv->other, LAB_SOUND_BYTES);
        b[LAB_EDIT_MODE] = lv->l->envelope_mode;
        record(lv, b, LAB_VIEW_NO_RUN);
        lv->ab = LAB_VIEW_AB_OFF;
    }
}

static void begin(lab_view* lv, uint8_t before[LAB_EDIT_BYTES]) {
    a_becomes_edit(lv);
    lab_edit_get(lv->l, before);
}

static void undo(lab_view* lv, screen* s) {
    a_becomes_edit(lv);
    lv->run = LAB_VIEW_NO_RUN;
    if (!lab_edit_undo(&lv->undo, lv->l)) {
        screen_toast(s, "NOTHING TO UNDO", SCREEN_TOAST_MS);
        return;
    }
    char t[10] = "UNDO (";
    uint32_t n = lv->undo.steps;
    uint32_t k = 6;
    if (n >= 10u) {
        t[k++] = (char)('0' + n / 10u);
    }
    t[k++] = (char)('0' + n % 10u);
    t[k++] = ')';
    t[k] = 0;
    screen_toast(s, t, SCREEN_TOAST_MS);
}

static void peek_end(lab_view* lv) {
    if (lv->peek) {
        lv->ab = (uint8_t)(lv->peek - 1u);
        lv->peek = 0;
        live_sound_apply(lv->l, lv->other);
    }
}

static void peek_start(lab_view* lv) {
    live* l = lv->l;
    sync(lv);
    lv->edit_spoiled = 1;
    if (lv->peek || lv->ab == LAB_VIEW_AB_A) {
        return;
    }
    uint8_t now[LAB_SOUND_BYTES];
    uint8_t a[LAB_SOUND_BYTES];
    live_sound_get(l, now);
    a_sound(l, a);
    if (!lab_sound_equal(now, a)) {
        memcpy(lv->other, now, LAB_SOUND_BYTES);
        live_sound_apply(l, a);
        lv->peek = (uint8_t)(lv->ab + 1u);
    }
}

static void ab_tap(lab_view* lv, screen* s) {
    live* l = lv->l;
    sync(lv);
    uint8_t now[LAB_SOUND_BYTES];
    uint8_t a[LAB_SOUND_BYTES];
    live_sound_get(l, now);
    a_sound(l, a);
    if (lv->ab == LAB_VIEW_AB_A) {
        if (lab_sound_equal(now, a)) {
            live_sound_apply(l, lv->other);
            lv->ab = LAB_VIEW_AB_B;
            return;
        }
        a_becomes_edit(lv);
    }
    if (lab_sound_equal(now, a)) {
        lv->ab = LAB_VIEW_AB_OFF;
        screen_toast(s, "NOTHING TO COMPARE", SCREEN_TOAST_MS);
        return;
    }
    memcpy(lv->other, now, LAB_SOUND_BYTES);
    live_sound_apply(l, a);
    lv->ab = LAB_VIEW_AB_A;
}

void lab_view_buttons(lab_view* lv, screen* s, uint32_t pressed, uint32_t held, int top) {
    uint32_t bit = 1u << PANEL_BTN_EDIT;
    if (pressed & bit) {
        lv->edit_down = (uint8_t)(top != 0);
        lv->edit_spoiled = (held & ~bit) != 0;
    }
    if (!lv->edit_down) {
        return;
    }
    if (pressed & ~bit) {
        lv->edit_spoiled = 1;
        peek_end(lv);
    }
    if (!(held & bit)) {
        lv->edit_down = 0;
        peek_end(lv);
        if (!lv->edit_spoiled && top && lv->mode == LAB_VIEW_MODE_PAGES) {
            ab_tap(lv, s);
        }
    }
}

static int32_t clamp(int32_t v, int32_t lo, int32_t hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

static void add(live* l, uint32_t param, int32_t delta) {
    int32_t v = (int32_t)live_param_get(l, param) + delta;
    live_param_set(l, param, (uint32_t)clamp(v, 0, (int32_t)live_param_max(param)));
}

static void pulse_turn(live* l, int32_t d) {
    if (ratio_card(l)) {
        live_knob(l, LIVE_KNOB_TONE, d);
        return;
    }
    int32_t pw = l->pulse_width > 2048u ? 4096 - (int32_t)l->pulse_width : (int32_t)l->pulse_width;
    int32_t pos = pw < LIVE_PULSE_FIRST ? 0 : (pw - LIVE_PULSE_FIRST + LIVE_PULSE_STEP / 2) / LIVE_PULSE_STEP;
    pos = clamp(pos + d, 0, LIVE_PULSE_POSITIONS - 1);
    live_param_set(l, LIVE_PARAM_PULSE_WIDTH, (uint32_t)(LIVE_PULSE_FIRST + LIVE_PULSE_STEP * pos));
}

static void card_turn(lab_view* lv, uint32_t k, int32_t d) {
    live* l = lv->l;
    d = clamp(d, -64, 64);
    switch (lv->page) {
    case LAB_PAGE_OSC:
        if (k == 0u) {
            pulse_turn(l, d);
        } else if (k < 3u) {
            live_walk_turn(l, k == 1u ? LIVE_PARAM_PWM_DEPTH : LIVE_PARAM_PWM_RATE, d);
        } else {
            add(l, LIVE_PARAM_TUNE, d);
        }
        break;
    case LAB_PAGE_ENVELOPE: add(l, LIVE_PARAM_ATTACK + k, d); break;
    case LAB_PAGE_FILTER:
        if (k < 2u) {
            live_knob(l, k ? LIVE_KNOB_RESONANCE : LIVE_KNOB_CUTOFF, d);
        } else if (k == 2u) {
            add(l, LIVE_PARAM_FILTER_VOICES, d);
        } else {
            if (d > 0 && !l->filter_lfo && !l->pwm_rate) {
                live_param_set(l, LIVE_PARAM_PWM_RATE, 20u);
            }
            add(l, LIVE_PARAM_FILTER_LFO, d);
        }
        break;
    case LAB_PAGE_MOD:
        if (k == 3u) {
            add(l, LIVE_PARAM_ARP_RATE, d);
        } else {
            live_walk_turn(l, LIVE_PARAM_VIBRATO_RATE + k, d);
        }
        break;
    case LAB_PAGE_MACRO:
        if (k < 2u) {
            add(l, LIVE_PARAM_RESO_LINK + k, d);
        }
        break;
    default: live_slot_turn(l, l->fx_select % LIVE_SLOTS, LIVE_SLOT_AMOUNT + k, d); break;
    }
}

static void choice_turn(lab_view* lv, int32_t steps) {
    live* l = lv->l;
    switch (lv->page) {
    case LAB_PAGE_OSC: {
        uint32_t i = (uint32_t)clamp((int32_t)lab_view_wave_of(l->control) + steps, 0,
                                     LAB_VIEW_WAVES - 1);
        uint8_t w = WAVES[i];
        if (w != (l->control & WAVE_BITS)) {
            live_param_set(l, LIVE_PARAM_RING, w >> 2 & 1u);
            live_param_set(l, LIVE_PARAM_SYNC, w >> 1 & 1u);
            live_param_set(l, LIVE_PARAM_WAVEFORM, w >> 4);
        }
        break;
    }
    case LAB_PAGE_ENVELOPE:
        live_param_set(l, LIVE_PARAM_ENVELOPE_MODE, (uint32_t)clamp(l->envelope_mode + steps, 0, 1));
        break;
    case LAB_PAGE_MACRO: break;
    case LAB_PAGE_FILTER: {
        int32_t mode = l->filter_mode >> 4 & 7u;
        int32_t at = -1;
        for (int32_t i = 0; i < 4; i++) {
            if (MODES[i] == mode) {
                at = i;
            }
        }
        if (at < 0) {
            int32_t below = 0;
            while (below < 4 && MODES[below] < mode) {
                below++;
            }
            if ((below == 0 && steps < 0) || (below == 4 && steps > 0)) {
                break;
            }
            at = below + (steps > 0 ? -1 : 0);
        }
        at = clamp(at + steps, 0, 3);
        live_param_set(l, LIVE_PARAM_FILTER_MODE, MODES[at]);
        break;
    }
    case LAB_PAGE_MOD: {
        int32_t at = l->arp_on ? 1 + (int32_t)l->arp_order : 0;
        int32_t to = clamp(at + steps, 0, 4);
        if (to != at) {
            if (to) {
                live_arp_set(l, LIVE_ARP_ORDER, (uint32_t)to - 1u);
            }
            live_arp_set(l, LIVE_ARP_ON, to != 0);
        }
        break;
    }
    default: {
        uint32_t slot = l->fx_select % LIVE_SLOTS;
        int32_t to = (int32_t)l->slot[slot][LIVE_SLOT_ALGORITHM] + steps;
        live_slot_pick(l, slot, (uint32_t)clamp(to, 0, (int32_t)live_slot_algorithms(slot) - 1));
        break;
    }
    }
}

static void name_turn(lab_view* lv, uint32_t id, int32_t steps);
static void roll(lab_view* lv, screen* s, int whole);

static void lone_due(lab_view* lv, uint32_t button) {
    if (button == PANEL_BTN_SCL) {
        live_fx_select_next(lv->l);
    }
}

void lab_view_lone_released(lab_view* lv, uint32_t held) {
    lone_due(lv, screen_lone_released(&lv->lone, held));
}

static int input(screen* s, void* state, const screen_event* e) {
    lab_view* lv = state;
    if (lv->lone.waiting) {
        uint32_t chord = screen_lone_chord(&lv->lone, e);
        if (chord != SCREEN_LONE_NO_BUTTON) {
            lv->run = LAB_VIEW_NO_RUN;
            if (chord == PANEL_BTN_LFO) {
                undo(lv, s);
            } else {
                roll(lv, s, chord == PANEL_BTN_SCL);
            }
            return 1;
        }
        lone_due(lv, screen_lone_take(&lv->lone));
    }
    if (e->kind == SCREEN_TURN && lv->edit_down) {
        lv->edit_spoiled = 1;
        peek_end(lv);
    }
    if (lv->mode) {
        if (e->kind == SCREEN_PRESS) {
            return e->id == PANEL_BTN_ENV || e->id == PANEL_BTN_LFO || e->id == PANEL_BTN_FX;
        }
        if (e->id == PANEL_ENC_SELECT || e->id == PANEL_ENC_ALGORITHM || e->id == PANEL_ENC_K1) {
            name_turn(lv, e->id, e->steps);
            return 1;
        }
        return e->id > PANEL_ENC_K1 && e->id <= PANEL_ENC_K4;
    }
    live* l = lv->l;
    int edit = (int)(e->held >> PANEL_BTN_EDIT & 1u);
    if (e->kind == SCREEN_PRESS) {
        lv->run = LAB_VIEW_NO_RUN;
        uint32_t id = e->id;
        if (edit && id == PANEL_BTN_LFO) {
            undo(lv, s);
            return 1;
        }
        if (edit && (id == PANEL_BTN_ARP || id == PANEL_BTN_SCL)) {
            roll(lv, s, id == PANEL_BTN_SCL);
            return 1;
        }
        if (id == PANEL_BTN_ARP || id == PANEL_BTN_LFO) {
            screen_lone_wait(&lv->lone, id, SCREEN_LONE_DONE);
        }
        if (id == PANEL_BTN_SCL) {
            screen_lone_wait(&lv->lone, id, SCREEN_LONE_HELD_BACK);
            return 1;
        }
        uint32_t to = e->id == PANEL_BTN_ENV   ? LAB_PAGE_ENVELOPE
                      : e->id == PANEL_BTN_LFO ? LAB_PAGE_MOD
                      : e->id == PANEL_BTN_FX  ? LAB_PAGE_FX
                                               : LAB_PAGES;
        if (to == LAB_PAGES) {
            return 0;
        }
        lv->page = (uint8_t)to;
        return 1;
    }
    uint8_t before[LAB_EDIT_BYTES];
    if (e->id >= PANEL_ENC_K1 && e->id <= PANEL_ENC_K4) {
        begin(lv, before);
        uint32_t linked = lv->page == LAB_PAGE_FILTER ? l->res_out : live_pwm_depth_effective(l);
        card_turn(lv, e->id - PANEL_ENC_K1, e->steps);
        live_links_update(l);
        if (e->id == PANEL_ENC_K1 && (lv->page == LAB_PAGE_FILTER || lv->page == LAB_PAGE_OSC) &&
            linked != (lv->page == LAB_PAGE_FILTER ? l->res_out : live_pwm_depth_effective(l))) {
            lv->hl_link = 2u;
            lv->hl_until = s->now + SCREEN_POPUP_MS;
        }
        record(lv, before, e->id);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        begin(lv, before);
        choice_turn(lv, e->steps);
        record(lv, before, e->id);
        return 1;
    }
    lv->run = LAB_VIEW_NO_RUN;
    if (e->id == PANEL_ENC_SELECT) {
        lv->page = (uint8_t)clamp((int32_t)lv->page + e->steps, 0, LAB_PAGES - 1);
        return 1;
    }
    if (e->id == PANEL_ENC_PRESETS && edit) {
        begin(lv, before);
        live_preset_step(l, e->steps, 1);
        lv->loads = l->loads;
        record(lv, before, LAB_VIEW_NO_RUN);
        return 1;
    }
    return 0;
}

static uint32_t rnd(lab_view* lv) {
    uint32_t x = lv->rng | 1u;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    lv->rng = x;
    return x;
}

static int fresh(const lab_view* lv, const uint8_t before[LAB_EDIT_BYTES]) {
    uint8_t now[LAB_SOUND_BYTES];
    live_sound_get(lv->l, now);
    return lab_audible(now, lv->l->model) && !lab_sound_equal(now, before) &&
           lab_edit_hash(now) != lv->rolled;
}

static int card_used(const lab_view* lv, uint32_t k) {
    char name[LAB_VIEW_TEXT], value[LAB_VIEW_TEXT];
    uint32_t fill, max;
    return lab_view_card(lv, lv->page, k, name, value, &fill, &max);
}

static void vary_once(lab_view* lv) {
    uint32_t used = 0;
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t r = rnd(lv);
        if (card_used(lv, k)) {
            int32_t d = 1 + (int32_t)(r % 3u);
            card_turn(lv, k, r & 8u ? d : -d);
            used++;
        }
    }
    uint32_t r = rnd(lv);
    if (lv->page != LAB_PAGE_ENVELOPE && (r % 4u == 0u || !used)) {
        choice_turn(lv, r & 8u ? 1 : -1);
    }
}

static void roll(lab_view* lv, screen* s, int whole) {
    live* l = lv->l;
    uint8_t before[LAB_EDIT_BYTES];
    begin(lv, before);
    lv->rng ^= s->now * 0x9E3779B9u;
    int ok = 0;
    uint32_t pw = (uint32_t)(before[LAB_SOUND_PULSE] | before[LAB_SOUND_PULSE + 1u] << 8);
    for (uint32_t t = 0; t < (whole ? LAB_VIEW_ROLL_TRIES : LAB_VIEW_VARY_TRIES) && !ok; t++) {
        if (whole) {
            live_param_set(l, LIVE_PARAM_PULSE_WIDTH, pw);
            live_dice_shape(l, rnd(lv));
        } else {
            if (t) {
                live_sound_apply(l, before);
            }
            vary_once(lv);
        }
        ok = fresh(lv, before);
    }
    for (uint32_t k = 0; k < 10u && !ok && !whole; k++) {
        live_sound_apply(l, before);
        int32_t d = k & 1u ? -1 : 1;
        if (k >= 8u) {
            if (lv->page != LAB_PAGE_ENVELOPE) {
                choice_turn(lv, d);
                ok = fresh(lv, before);
            }
        } else if (card_used(lv, k >> 1)) {
            card_turn(lv, k >> 1, d);
            ok = fresh(lv, before);
        }
    }
    if (!ok) {
        live_sound_apply(l, before);
        screen_toast(s, "NOTHING AUDIBLE", SCREEN_TOAST_MS);
        return;
    }
    uint8_t now[LAB_SOUND_BYTES];
    live_sound_get(l, now);
    lv->rolled = (uint16_t)lab_edit_hash(now);
    record(lv, before, LAB_VIEW_NO_RUN);
    screen_toast(s, whole ? "ROLLED" : "VARIED", SCREEN_TOAST_MS);
}

static int keep(lab_view* lv, screen* s, uint32_t n, const char* name) {
    live* l = lv->l;
    uint8_t sound[LAB_SOUND_BYTES];
    live_sound_get(l, sound);
    uint8_t fresh = l->user_sound != n + 1u;
    int r = user_sound_save(lv->us, n, name, sound, s->now);
    if (r < 0) {
        screen_toast(s, "SAVING: WAIT", SCREEN_TOAST_MS);
        return 0;
    }
    lv->kept_new = fresh;
    if (r > 0) {
        lab_view_kept(lv, n, 0, s);
    }
    return 1;
}

static uint32_t free_slot(const live* l) {
    uint32_t n = 0;
    while (n < USER_SOUNDS && (l->sounds_valid >> n & 1u)) {
        n++;
    }
    return n;
}

static void name_set(lab_view* lv, const char* from) {
    char auto_[LAB_SOUND_NAME + 1u];
    if (!from) {
        auto_name(auto_, lv->slot);
        from = auto_;
    }
    uint32_t i = 0;
    for (; i < LAB_SOUND_NAME && from[i]; i++) {
        lv->name[i] = from[i];
    }
    for (; i < LAB_SOUND_NAME; i++) {
        lv->name[i] = ' ';
    }
    lv->name[LAB_SOUND_NAME] = 0;
}

static void name_open(lab_view* lv) {
    const live* l = lv->l;
    uint32_t n = l->user_sound ? l->user_sound - 1u : free_slot(l);
    lv->slot = (uint8_t)(n < USER_SOUNDS ? n : 0u);
    lv->cursor_at = 0;
    lv->named = l->user_sound != 0;
    name_set(lv, lv->named ? l->base_name : NULL);
    lv->mode = LAB_VIEW_MODE_NAME;
}

static void name_write(lab_view* lv, screen* s) {
    const char* first = lv->name;
    uint32_t n = LAB_SOUND_NAME;
    while (n && first[n - 1u] == ' ') {
        n--;
    }
    while (n && *first == ' ') {
        first++;
        n--;
    }
    char name[LAB_SOUND_NAME + 1u];
    memcpy(name, first, n);
    name[n] = 0;
    if (!n) {
        auto_name(name, lv->slot);
    }
    if ((lv->l->sounds_valid >> lv->slot & 1u) &&
        lv->mode != LAB_VIEW_MODE_ASK) {
        lv->mode = LAB_VIEW_MODE_ASK;
        return;
    }
    if (keep(lv, s, lv->slot, name)) {
        lv->mode = LAB_VIEW_MODE_PAGES;
    }
}

void lab_view_save(lab_view* lv, screen* s, int edit) {
    live* l = lv->l;
    lv->run = LAB_VIEW_NO_RUN;
    if (lv->mode) {
        name_write(lv, s);
        return;
    }
    if (edit) {
        name_open(lv);
        return;
    }
    uint32_t n = l->user_sound ? l->user_sound - 1u : free_slot(l);
    char name[LAB_SOUND_NAME + 1u];
    if (l->user_sound) {
        memcpy(name, l->base_name, sizeof name);
    } else if (n == USER_SOUNDS) {
        screen_toast(s, "USER FULL: EDIT+SAVE", SCREEN_TOAST_MS);
        return;
    } else {
        auto_name(name, n);
    }
    (void)keep(lv, s, n, name);
}

void lab_view_kept(const lab_view* lv, uint32_t slot, int gaps, screen* s) {
    char name[LAB_SOUND_NAME + 1u];
    uint8_t sound[LAB_SOUND_BYTES];
    if (!user_sound_read(lv->us, slot, name, sound)) {
        memcpy(name, lv->l->base_name, sizeof name);
    }
    char text[SCREEN_TOAST_CHARS + 8u];
    char* e = append(text, lv->kept_new ? "KEPT AS " : "KEPT ");
    e = slot_label(e, slot);
    *e++ = ' ';
    e = append(e, name);
    if (gaps && e - text + 7 <= (int)SCREEN_TOAST_CHARS) {
        e = append(e, " (GAPS)");
    }
    *e = 0;
    screen_toast(s, text, SCREEN_TOAST_MS);
}

int lab_view_home(lab_view* lv) {
    if (!lv->mode) {
        return 0;
    }
    lv->mode = LAB_VIEW_MODE_PAGES;
    return 1;
}

uint32_t lab_view_name_keys(lab_view* lv, uint32_t pressed) {
    uint32_t keys = pressed & ((1u << LAB_VIEW_NAME_KEYS) - 1u);
    if (!lv->mode && pressed && lv->edit_down) {
        peek_start(lv);
        return 0;
    }
    if (!lv->mode || !keys) {
        return 0;
    }
    if (!lv->named) {
        memset(lv->name, ' ', LAB_SOUND_NAME);
    }
    for (uint32_t k = 0; k < LAB_VIEW_NAME_KEYS; k++) {
        if (keys >> k & 1u) {
            lv->name[lv->cursor_at] = k < 26u ? (char)('A' + k) : ' ';
            if (lv->cursor_at + 1u < LAB_SOUND_NAME) {
                lv->cursor_at++;
            }
        }
    }
    lv->named = 1;
    lv->mode = LAB_VIEW_MODE_NAME;
    return keys;
}

static void name_turn(lab_view* lv, uint32_t id, int32_t steps) {
    lv->mode = LAB_VIEW_MODE_NAME;
    if (id == PANEL_ENC_K1) {
        lv->slot = (uint8_t)clamp((int32_t)lv->slot + steps, 0, USER_SOUNDS - 1);
        if (!lv->named) {
            name_set(lv, NULL);
        }
    } else if (id == PANEL_ENC_SELECT) {
        lv->cursor_at = (uint8_t)clamp((int32_t)lv->cursor_at + steps, 0, LAB_SOUND_NAME - 1);
    } else if (id == PANEL_ENC_ALGORITHM) {
        int32_t ch = ((int32_t)lv->name[lv->cursor_at] - ' ' + steps) % 64;
        lv->name[lv->cursor_at] = (char)(' ' + (ch < 0 ? ch + 64 : ch));
        lv->named = 1;
    }
}

void lab_view_init(lab_view* lv, live* l, user_sounds* us, song_view* shared, const scope* sc) {
    memset(lv, 0, sizeof *lv);
    lv->l = l;
    lv->us = us;
    lv->canvas = shared;
    lv->sc = sc;
    lv->loads = l->loads;
    lv->run = LAB_VIEW_NO_RUN;
    lv->rng = 0x2545F491u;
}

void lab_view_page_set(lab_view* lv, uint32_t page) {
    lv->mode = LAB_VIEW_MODE_PAGES;
    lv->page = (uint8_t)(page < LAB_PAGES ? page : LAB_PAGE_OSC);
}

uint32_t lab_view_panel_page(const lab_view* lv) {
    return PANEL_PAGE_LAB_FIRST + lv->page;
}

void lab_view_flash(lab_view* lv, uint32_t k, uint32_t now_ms) {
    lv->hl = (uint8_t)(k + 1u);
    lv->hl_until = now_ms + SCREEN_POPUP_MS;
}

static void enter(screen* s, void* state) {
    (void)s;
    lab_view* lv = state;
    lv->built = 0;
    lv->cursor = 0;
    lv->hl = 0;
    lv->art_done = 0;
}

static uint32_t art_page(const lab_view* lv) {
    return lv->mode ? LAB_DRAW_BLANK : lv->page;
}

static int art_stale(const lab_view* lv) {
    return !lv->art_done || lab_draw_look(lv->l, lv->sc, art_page(lv), lv->hl) != lv->art;
}

static int changed(screen* s, void* state) {
    lab_view* lv = state;
    expire(lv, s);
    sync(lv);
    return lv->canvas->which != SONG_VIEW_CANVAS_LAB || !lv->built || lv->copying ||
           look(lv, s->now) != lv->look || art_stale(lv);
}

static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

static void compose(screen* s, void* state, grid* g) {
    lab_view* lv = state;
    if (song_view_acquire_canvas(lv->canvas, SONG_VIEW_CANVAS_LAB)) {
        lv->built = 0;
        lv->cursor = 0;
    }
    expire(lv, s);
    int text = !lv->built || look(lv, s->now) != lv->look;
    int art = art_stale(lv);
    if (text && art && lv->built) {
        if (lv->art_turn) {
            text = 0;
        } else {
            art = 0;
        }
        lv->art_turn ^= 1u;
    }
    if (text) {
        build(lv, s->now);
    }
    const canvas* c = &lv->canvas->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_canvas_row(g, c, r);
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < LAB_VIEW_FRAME_CELLS ? LAB_VIEW_FRAME_CELLS - used : 0u;
    const uint8_t* cc = &c->ch[0][0];
    const uint8_t* ca = &c->attr[0][0];
    const uint8_t* cs = &c->src[0][0];
    uint32_t i = lv->cursor;
    for (uint32_t n = 0; n < GRID_CELLS; n++, i = i + 1u < GRID_CELLS ? i + 1u : 0u) {
        uint8_t ch = cc[i];
        if (g->ch[i] == ch && g->attr[i] == ca[i] &&
            ((uint8_t)(ch - GRID_BIG_FIRST) >= GRID_BIG_PIECES || g->src[i] == cs[i])) {
            continue;
        }
        uint32_t r = i / GRID_COLS;
        uint32_t k = i % GRID_COLS;
        if (screen_covered(s, k, r) || (raster && border(k, r))) {
            continue;
        }
        if (!left) {
            lv->cursor = (uint16_t)i;
            lv->copying = 1;
            return;
        }
        grid_canvas_put(g, c, k, r);
        left--;
    }
    lv->copying = 0;
    if (art) {
        lv->art = lab_draw_look(lv->l, lv->sc, art_page(lv), lv->hl);
        lv->art_done = (uint8_t)lab_draw_page(lv->l, lv->sc, art_page(lv), lv->hl,
                                              theme_get(lv->l->theme), g, s, LAB_VIEW_ART_ROW,
                                              &left);
    }
}

const view lab_view_view = {"LAB", enter, NULL, input, compose, changed};

LIVE_COLD void lab_view_sent(lab_view* lv, uint32_t notes, uint32_t now) {
    lv->sent = (uint16_t)(notes & (LAB_SONG_DROPPED | LAB_SONG_APPROX));
    lv->sent_until = lv->sent ? now + LAB_VIEW_SENT_MS : now;
}

LIVE_COLD const char* lab_view_hint_text(const lab_view* lv, uint32_t now, char* out) {
    if (!lv->sent || (int32_t)(lv->sent_until - now) <= 0) {
        return HINTS[hint_phase(now)];
    }
    int dropped = (lv->sent & LAB_SONG_DROPPED) &&
                  (!(lv->sent & LAB_SONG_APPROX) || !(hint_phase(now) & 1u));
    char* e = append(out, dropped ? "NOT SENT: " : "APPROX: ");
    lab_song_words(lv->sent, dropped, e, GRID_COLS + 1u - (uint32_t)(e - out));
    return out;
}

LIVE_COLD void lab_view_pull(lab_view* lv, screen* s, const uint8_t sound[LAB_SOUND_BYTES],
                             uint32_t instr, uint32_t notes) {
    uint8_t before[LAB_EDIT_BYTES];
    begin(lv, before);
    live_sound_apply(lv->l, sound);
    lv->run = LAB_VIEW_NO_RUN;
    record(lv, before, LAB_VIEW_NO_RUN);
    static const char HEX[] = "0123456789ABCDEF";
    char t[SCREEN_TOAST_CHARS + 1u];
    char* e = t;
    if (!(notes & LAB_SONG_APPROX)) {
        e = append(e, "PULLED ");
    }
    *e++ = 'I';
    *e++ = HEX[instr >> 4 & 15u];
    *e++ = HEX[instr & 15u];
    if (notes & LAB_SONG_APPROX) {
        e = append(e, " ~");
        lab_song_words(notes, 0, e, SCREEN_TOAST_CHARS + 1u - (uint32_t)(e - t));
    } else {
        *e = 0;
    }
    lv->sent = 0u;
    screen_toast(s, t, SCREEN_TOAST_MS);
}
