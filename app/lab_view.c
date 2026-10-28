// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_view.h"

#include <string.h>

#include "chip_view.h"
#include "font.h"
#include "lab_draw.h"
#include "live_view.h"
#include "panel.h"
#include "theme.h"
#include "views.h"

/* The knobs' columns, as LIVE's knob row and CHIP's have them: 8, 7, 7 and 8 cells side by
 * side, a bar a cell shorter than its column. */
static const uint8_t KNOB_COL[4] = {0, 8, 15, 22};
static const uint8_t KNOB_W[4] = {8, 7, 7, 8};
/* The header's page number, `2/5`, at 2x on the right. */
#define PLACE_COL 24u
/* ALG's legend on the choice's first row, ENVELOPE's ALL SOUNDS on its second. */
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

static const char* const PAGE_NAMES[LAB_PAGES] = {"OSC", "ENVELOPE", "FILTER", "MOD", "FX"};

/* OSC's choices: $d404's waveform, ring and sync bits, and their names. */
static const uint8_t WAVES[LAB_VIEW_WAVES] = {0x10, 0x20, 0x40, 0x80, 0x30, 0x50,
                                              0x60, 0x70, 0x14, 0x22, 0x42};
static const char* const WAVE_NAMES[LAB_VIEW_WAVES] = {
    "TRIANGLE",  "SAW",       "PULSE",       "NOISE",    "TRI+SAW",  "TRI+PULSE",
    "SAW+PULSE", "TRI+SAW+PUL", "TRI RING", "SAW SYNC", "PULSE SYNC"};
/* The bits a choice sets: the waveform, ring and sync (never the gate or the test bit). */
#define WAVE_BITS 0xF6u
/* FILTER's choices, as the filter mode ($d418 bits 4-6, shifted down): LP, BP, HP, NOTCH. */
static const uint8_t MODES[4] = {1, 2, 4, 5};
static const char* const MODE_NAMES[8] = {"OFF", "LP",    "BP",    "LP+BP",
                                          "HP",  "NOTCH", "BP+HP", "ALL"};
static const char* const ARP_NAMES[5] = {"ARP OFF", "ARP UP", "ARP DOWN", "ARP UP-DOWN",
                                         "ARP PLAYED"};
static const char* const VELOCITY_NAMES[4] = {"OFF", "SUS", "CUT", "PW"};
/* The hint line's phrases: every LAB gesture, a phrase at a time. */
static const char* const HINTS[LAB_VIEW_HINTS] = {
    "EDIT A/B  +LFO UNDO  +ARP VARY",
    "EDIT +SCL ROLL  +SAVE NAME",
    "SAVE KEEP  SELECT NEXT PAGE",
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
    for (uint32_t i = 0; i < 8u; i++) { /* the waveform alone, without its ring or sync */
        if (WAVES[i] == (c & 0xF0u)) {
            return i;
        }
    }
    return 0;
}

/* ---- text ---- */

/* n as decimal digits into out (a sign first with sign: "+20", "-5", "0"); returns the end. */
static char* number(char* out, int32_t n, int sign) {
    if (n < 0) {
        *out++ = '-';
        n = -n;
    } else if (sign && n > 0) {
        *out++ = '+';
    }
    char d[6];
    uint32_t k = 0;
    uint32_t u = (uint32_t)n;
    do {
        d[k++] = (char)('0' + u % 10u);
        u /= 10u;
    } while (u);
    while (k) {
        *out++ = d[--k];
    }
    *out = 0;
    return out;
}

/* tenths of a unit as at most 3 characters: "0", "0.5", "9.9", then whole: "12". */
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

static void copy(char* out, const char* s) {
    uint32_t n = 0;
    for (; s[n] && n + 1u < LAB_VIEW_TEXT; n++) {
        out[n] = s[n];
    }
    out[n] = 0;
}

/* The pulse width as one side of the square in percent, as LIVE's K3 says it ("50%"). */
static void pulse_percent(char* out, uint32_t pw) {
    uint32_t side = pw > 2048u ? 4096u - pw : pw;
    char* e = number(out, (int32_t)((side * 99u + 2047u) / 4095u), 0);
    e[0] = '%';
    e[1] = 0;
}

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
            char* e = number(value, (int32_t)((v * 100u + 2048u) / 4096u), 0);
            e[0] = '%';
            e[1] = 0;
        } else if (k == 2u) {
            copy(name, "PWM HZ");
            tenths(value, (v + 1u) / 2u); /* 0.05 Hz a step */
        } else {
            copy(name, "TUNE");
            (void)number(value, (int32_t)v - LIVE_TUNE_CENTS, 1);
        }
        return 1;
    case LAB_PAGE_ENVELOPE: {
        /* SUS: SUSTAIN would fill K3's column to K4's name */
        static const char* const NAMES[4] = {"ATTACK", "DECAY", "SUS", "RELEASE"};
        copy(name, NAMES[k & 3u]);
        (void)number(value, (int32_t)v, 0);
        return 1;
    }
    case LAB_PAGE_FILTER:
        if (k == 3u) {
            return 0; /* free (docs/plans/2026-10-25-lab-plan.md Task 18) */
        }
        if (k < 2u) {
            copy(name, k ? "RESO" : "CUTOFF");
            live_view_knob_text(l, k, value); /* LIVE's: the cutoff the chip has now */
            if (k == 0u) {
                *fill = l->cutoff_out;
            }
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
            copy(name, "VELO");
            copy(value, VELOCITY_NAMES[v & 3u]);
        } else {
            copy(name, k == 1u ? "DEPTH" : "DELAY");
            (void)number(value, (int32_t)v, 0);
        }
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
        /* A mix no choice has (MIDI's): its parts, "SAW+NOI SYNC". */
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
    default: {
        uint32_t slot = l->fx_select % LIVE_SLOTS;
        copy(out, views_algorithm_name(slot, l->slot[slot][LIVE_SLOT_ALGORITHM]));
        return;
    }
    }
}

/* ---- the canvas ---- */

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

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* 1 while the choice is off (the filter, ARP, the slot): it shows dim. */
static int choice_off(const lab_view* lv) {
    const live* l = lv->l;
    switch (lv->page) {
    case LAB_PAGE_FILTER: return (l->filter_mode >> 4 & 7u) == 0u;
    case LAB_PAGE_MOD: return !l->arp_on;
    case LAB_PAGE_FX: return l->slot[l->fx_select % LIVE_SLOTS][LIVE_SLOT_ALGORITHM] == 0u;
    default: return 0;
    }
}

/* Card k in its knob's column: its name small in the knob's readable colour (reversed while it
 * is lit), its value in white at 3x (2x when it takes 3 characters), a bar in the knob's colour
 * and the knob's name. A card the page has no use for is dark grey. */
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
                 : lv->hl == k + 1u ? GRID_ATTR(th->bg, th->knob_text[k])
                                    : GRID_ATTR(th->knob_text[k], th->bg);
    text(c, col, LAB_VIEW_LABEL_ROW, name, na);
    uint8_t va = GRID_ATTR(used ? th->value : th->dim, th->bg);
    if (length(value) <= 2u) {
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

/* The hint phrase that shows at now (ms). */
static uint32_t hint_phase(uint32_t now) {
    return now / LAB_VIEW_HINT_MS % LAB_VIEW_HINTS;
}

/* What the canvas shows, hashed (FNV-1a): the page, the theme, what is lit, the hint's phrase,
 * the sound (its every field, its name and mark), and what the cards and the choice read
 * besides it (the envelope mode, the selected slot, the cutoff the chip has now). */
static uint32_t look(const lab_view* lv, uint32_t now) {
    const live* l = lv->l;
    uint8_t b[LAB_SOUND_BYTES + LIVE_PRESET_NAME_CHARS + 12u];
    live_sound_get(l, b);
    uint32_t n = LAB_SOUND_BYTES;
    char name[LIVE_PRESET_NAME_CHARS + 2u];
    live_preset_name(l, name);
    for (uint32_t i = 0; name[i] && i < LIVE_PRESET_NAME_CHARS + 1u; i++) {
        b[n++] = (uint8_t)name[i];
    }
    b[n++] = lv->page;
    b[n++] = l->theme;
    b[n++] = lv->hl;
    b[n++] = (uint8_t)hint_phase(now);
    b[n++] = l->envelope_mode;
    b[n++] = l->fx_select;
    b[n++] = (uint8_t)l->model;
    b[n++] = (uint8_t)l->cutoff_out;
    b[n++] = (uint8_t)(l->cutoff_out >> 8);
    uint32_t h = 2166136261u;
    for (uint32_t i = 0; i < n; i++) {
        h = (h ^ b[i]) * 16777619u;
    }
    return h;
}

/* The whole page into the canvas, and what it shows remembered. */
static void build(lab_view* lv, uint32_t now) {
    canvas* c = &lv->canvas->cv;
    const live* l = lv->l;
    const theme* th = theme_get(l->theme);
    lv->look = look(lv, now);
    memset(c->ch, ' ', sizeof c->ch);
    memset(c->attr, THEME_ATTR(th, value), sizeof c->attr);
    /* The header: the page's name and its place, at 2x. */
    (void)grid_canvas_text_big(c, 0, 0, 2, PAGE_NAMES[lv->page], THEME_ATTR(th, hot));
    char place[4] = {(char)('1' + lv->page), '/', (char)('0' + LAB_PAGES), 0};
    (void)grid_canvas_text_big(c, PLACE_COL, 0, 2, place, THEME_ATTR(th, hint));
    /* The sound's name and its mark, at 2x. */
    char name[LIVE_PRESET_NAME_CHARS + 2u];
    live_preset_name(l, name);
    (void)grid_canvas_text_big(c, 0, LAB_VIEW_NAME_ROW, 2, name, THEME_ATTR(th, value));
    lab_draw_guides(c, l, lv->page, lv->hl, th, LAB_VIEW_ART_ROW);
    /* The choice at 2x (lit in reverse video), ALG small on the right. */
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
    text(c, 0, LAB_VIEW_HINT_ROW, HINTS[hint_phase(now)], THEME_ATTR(th, hint));
    lv->built = 1;
    lv->copying = 1;
}

/* A highlight whose time is up goes: the look changes with it, so the page is built again. */
static void expire(lab_view* lv, const screen* s) {
    if (lv->hl && (int32_t)(s->now - lv->hl_until) >= 0) {
        lv->hl = 0;
    }
}

/* ---- input ---- */

static int32_t clamp(int32_t v, int32_t lo, int32_t hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

/* A parameter moved by delta, clamped to its range. */
static void add(live* l, uint32_t param, int32_t delta) {
    int32_t v = (int32_t)live_param_get(l, param) + delta;
    live_param_set(l, param, (uint32_t)clamp(v, 0, (int32_t)live_param_max(param)));
}

/* OSC's K1: LIVE's pulse walk (LIVE_PULSE_POSITIONS of LIVE_PULSE_STEP, one side of the
 * square), for any waveform; on a mono-sync sound without the pulse bit the sync ratio's
 * semitones, LIVE's K3 on SYNC LEAD. */
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

/* Card k turned by d detents: each by its detent (docs/plans/2026-10-25-lab-plan.md "The
 * pages"), as LIVE where LIVE has the control. */
static void card_turn(lab_view* lv, uint32_t k, int32_t d) {
    live* l = lv->l;
    d = clamp(d, -64, 64);
    switch (lv->page) {
    case LAB_PAGE_OSC:
        if (k == 0u) {
            pulse_turn(l, d);
        } else {
            static const uint8_t PARAM[3] = {LIVE_PARAM_PWM_DEPTH, LIVE_PARAM_PWM_RATE,
                                             LIVE_PARAM_TUNE};
            static const uint8_t DETENT[3] = {32, 1, 1};
            add(l, PARAM[k - 1u], d * DETENT[k - 1u]);
        }
        break;
    case LAB_PAGE_ENVELOPE: add(l, LIVE_PARAM_ATTACK + k, d); break;
    case LAB_PAGE_FILTER:
        if (k < 2u) {
            live_knob(l, k ? LIVE_KNOB_RESONANCE : LIVE_KNOB_CUTOFF, d);
        } else if (k == 2u) {
            add(l, LIVE_PARAM_FILTER_VOICES, d);
        }
        break;
    case LAB_PAGE_MOD:
        add(l, k == 3u ? (uint32_t)LIVE_PARAM_VELOCITY_TARGET : LIVE_PARAM_VIBRATO_RATE + k, d);
        break;
    default: live_slot_turn(l, l->fx_select % LIVE_SLOTS, LIVE_SLOT_AMOUNT + k, d); break;
    }
}

/* ALGORITHM: the page's choice stepped, clamped. */
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
    case LAB_PAGE_FILTER: {
        /* From a mode no choice has (OFF, LP+BP, BP+HP, ALL): the next one the turn's way. */
        int32_t mode = l->filter_mode >> 4 & 7u;
        int32_t at = -1;
        for (int32_t i = 0; i < 4; i++) {
            if (MODES[i] == mode) {
                at = i;
            }
        }
        if (at < 0) {
            int32_t below = 0; /* the choices under the mode: it sits between below - 1 and below */
            while (below < 4 && MODES[below] < mode) {
                below++;
            }
            if ((below == 0 && steps < 0) || (below == 4 && steps > 0)) {
                break; /* nothing that way */
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

/* SELECT the pages, K1..K4 the cards, ALGORITHM the choice; ENV, LFO and FX their pages. */
static int input(screen* s, void* state, const screen_event* e) {
    (void)s;
    lab_view* lv = state;
    if (e->kind == SCREEN_PRESS) {
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
    if (e->id == PANEL_ENC_SELECT) {
        lv->page = (uint8_t)clamp((int32_t)lv->page + e->steps, 0, LAB_PAGES - 1);
        return 1;
    }
    if (e->id >= PANEL_ENC_K1 && e->id <= PANEL_ENC_K4) {
        card_turn(lv, e->id - PANEL_ENC_K1, e->steps);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        choice_turn(lv, e->steps);
        return 1;
    }
    return 0;
}

/* ---- keeping ---- */

/* "U03" for slot n (0..15) at out, the end. */
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

/* The automatic name for slot n: "LAB 03". */
static void auto_name(char* out, uint32_t n) {
    char* e = append(out, "LAB ");
    e[0] = (char)('0' + (n + 1u) / 10u);
    e[1] = (char)('0' + (n + 1u) % 10u);
    e[2] = 0;
}

void lab_view_save(lab_view* lv, user_sounds* us, screen* s) {
    live* l = lv->l;
    uint32_t n = l->user_sound ? l->user_sound - 1u : USER_SOUNDS;
    char name[LAB_SOUND_NAME + 1u];
    if (l->user_sound) {
        memcpy(name, l->base_name, sizeof name);
    } else {
        for (n = 0; n < USER_SOUNDS && (l->sounds_valid >> n & 1u); n++) {
        }
        if (n == USER_SOUNDS) {
            screen_toast(s, "USER FULL: EDIT+SAVE", SCREEN_TOAST_MS);
            return;
        }
        auto_name(name, n);
    }
    uint8_t sound[LAB_SOUND_BYTES];
    live_sound_get(l, sound);
    uint8_t fresh = l->user_sound != n + 1u;
    int r = user_sound_save(us, n, name, sound, s->now);
    if (r < 0) {
        screen_toast(s, "SAVING: WAIT", SCREEN_TOAST_MS);
        return;
    }
    lv->kept_new = fresh;
    if (r > 0) { /* no store: kept in RAM, nothing to wait for */
        lab_view_kept(lv, us, n, 0, s);
    }
}

void lab_view_kept(const lab_view* lv, const user_sounds* us, uint32_t slot, int gaps, screen* s) {
    char name[LAB_SOUND_NAME + 1u];
    uint8_t sound[LAB_SOUND_BYTES];
    if (!user_sound_read(us, slot, name, sound)) { /* no store: the sound as live has it */
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

/* ---- the view ---- */

void lab_view_init(lab_view* lv, live* l, song_view* shared, const scope* sc) {
    memset(lv, 0, sizeof *lv);
    lv->l = l;
    lv->canvas = shared;
    lv->sc = sc;
}

void lab_view_page_set(lab_view* lv, uint32_t page) {
    lv->page = (uint8_t)(page < LAB_PAGES ? page : LAB_PAGE_OSC);
}

uint32_t lab_view_panel_page(const lab_view* lv) {
    return PANEL_PAGE_LAB_FIRST + lv->page;
}

void lab_view_flash(lab_view* lv, uint32_t k, uint32_t now_ms) {
    lv->hl = (uint8_t)(k + 1u);
    lv->hl_until = now_ms + SCREEN_POPUP_MS;
}

/* Built anew at its first compose: what changed while it was away. */
static void enter(screen* s, void* state) {
    (void)s;
    lab_view* lv = state;
    lv->built = 0;
    lv->cursor = 0;
    lv->hl = 0;
    lv->art_done = 0; /* a switch cleared the overlays */
}

/* The drawing's look, and whether it is drawn whole. */
static int art_stale(const lab_view* lv) {
    return !lv->art_done || lab_draw_look(lv->l, lv->sc, lv->page, lv->hl) != lv->art;
}

static int changed(screen* s, void* state) {
    lab_view* lv = state;
    expire(lv, s);
    return lv->canvas->which != SONG_VIEW_CANVAS_LAB || !lv->built || lv->copying ||
           look(lv, s->now) != lv->look || art_stale(lv);
}

/* 1 for a cell of the screen's outer ring. */
static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

/* The canvas onto the grid, as CHIP copies its own: under a wipe the rows it reveals whole;
 * otherwise the changed cells from where the last frame stopped, within the frame's budget,
 * leaving alone what is covered this frame and the border while the raster draws over it. */
static void compose(screen* s, void* state, grid* g) {
    lab_view* lv = state;
    if (song_view_acquire_canvas(lv->canvas, SONG_VIEW_CANVAS_LAB)) {
        lv->built = 0;
        lv->cursor = 0;
    }
    expire(lv, s);
    int text = !lv->built || look(lv, s->now) != lv->look;
    int art = art_stale(lv);
    if (text && art && lv->built) { /* one a frame, by turns */
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
    /* The drawing, in what is left of the frame's cells. */
    if (art) {
        lv->art = lab_draw_look(lv->l, lv->sc, lv->page, lv->hl);
        lv->art_done = (uint8_t)lab_draw_page(lv->l, lv->sc, lv->page, lv->hl,
                                              theme_get(lv->l->theme), g, s, LAB_VIEW_ART_ROW,
                                              &left);
    }
}

const view lab_view_view = {"LAB", enter, NULL, input, compose, changed};
