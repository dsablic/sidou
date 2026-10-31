// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "fx/fx_slots.h"
#include "panel.h"
#include "sid/sid_character.h"
#include "views.h"

static const uint8_t KNOB_COL[4] = {0, 8, 15, 22};
static const uint8_t KNOB_W[4] = {8, 7, 7, 8};
static const char* const TAB_NAME[CHIP_VIEW_SECTIONS] = {"CHIP", "DIST", "MOD", "FX"};
static const uint8_t TAB_COL[CHIP_VIEW_SECTIONS] = {0, 8, 15, 22};
static const uint8_t TAB_W[CHIP_VIEW_SECTIONS] = {8, 7, 7, 8};
#define PROFILE_COL 13u
#define SUMMARY_VALUE_COL 9u

_Static_assert(CHIP_VIEW_TAB_ROW + 1u < CHIP_VIEW_CHOICE_ROW &&
                   CHIP_VIEW_CHOICE_ROW + 3u <= CHIP_VIEW_NAMES_ROW &&
                   CHIP_VIEW_NAMES_ROW < CHIP_VIEW_LABEL_ROW &&
                   CHIP_VIEW_VALUE_ROW + 3u == CHIP_VIEW_BAR_ROW &&
                   CHIP_VIEW_TAG_ROW + 1u < CHIP_VIEW_SUMMARY_ROW &&
                   CHIP_VIEW_SUMMARY_ROW + 2u * CHIP_VIEW_SECTIONS_SHOWN <= CHIP_VIEW_NOTE_ROW &&
                   CHIP_VIEW_NOTE_ROW < GRID_ROWS,
               "the page's bands follow each other");
_Static_assert(LIVE_SLOT_FIELDS == 5u && LIVE_SLOT_MIX == 4u,
               "Amount, A, B, Mix after the algorithm");

typedef struct {
    const char* name[3];
} controls;

static const controls DIST[] = {
    {{NULL, NULL, NULL}},
    {{"DRIVE", NULL, NULL}},
    {{"DRIVE", "TONE", "BIAS"}},
    {{"DRIVE", "OCT", "BIAS"}},
    {{"BITS", "RATE", NULL}},
    {{"TONE", "HUM", "NOISE"}},
};
static const controls MOD[] = {
    {{NULL, NULL, NULL}},
    {{"DEPTH", "RATE", "ENS"}},
    {{"DEPTH", "RATE", "FDBK"}},
    {{"DEPTH", "RATE", "FDBK"}},
    {{"DEPTH", "RATE", "SHAPE"}},
    {{"WIDTH", "RATE", "SHAPE"}},
    {{"WOW", "FLUT", NULL}},
};
static const controls FX[] = {
    {{NULL, NULL, NULL}},
    {{"FDBK", "TIME", "DAMP"}},
    {{"FDBK", "TIME", "DAMP"}},
    {{"DECAY", "DAMP", "DIFF"}},
    {{"DECAY", "DAMP", "BAND"}},
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

typedef chip_view_canvas canvas;

static void put(canvas* c, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        c->ch[row][col] = ch;
        c->attr[row][col] = attr;
    }
}

static void fill(canvas* c, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                 uint8_t attr) {
    uint32_t col1 = col + w < GRID_COLS ? col + w : GRID_COLS;
    uint32_t row1 = row + h < GRID_ROWS ? row + h : GRID_ROWS;
    for (uint32_t r = row; r < row1; r++) {
        for (uint32_t k = col; k < col1; k++) {
            c->ch[r][k] = ch;
            c->attr[r][k] = attr;
        }
    }
}

static uint32_t text(canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++) {
        uint8_t ch = (uint8_t)*s;
        put(c, col++, row, ch >= 'a' && ch <= 'z' ? (uint8_t)(ch - 'a' + 'A') : ch, attr);
    }
    return col;
}

static void big(canvas* c, uint32_t col, uint32_t row, uint32_t scale, const char* s,
                uint8_t attr) {
    char up[16];
    uint32_t n = 0;
    for (; s[n] && n + 1u < sizeof up; n++) {
        up[n] = s[n] >= 'a' && s[n] <= 'z' ? (char)(s[n] - 'a' + 'A') : s[n];
    }
    up[n] = 0;
    (void)grid_canvas_text_big(c, col, row, scale, up, attr);
}

static uint32_t profile_of(const live* l) {
    return l->profile_req;
}

int chip_view_song_tab(const chip_view* cv) {
    return cv->kr && cv->kr->song && cv->tab == 0u;
}

static uint32_t picked_user_slot(const chip_view* cv);

int chip_view_user(const chip_view* cv) {
    return (int)picked_user_slot(cv);
}

static uint32_t picked_user_slot(const chip_view* cv) {
    if (!chip_view_song_tab(cv)) {
        return cv->l->user_applied;
    }
    uint32_t n = cv->kr->song_user;
    return n && cv->kr->song_want.profile == cv->l->user_base[n - 1u] ? n : 0u;
}

static void shown(const chip_view* cv, chip_setup* s, uint8_t* model) {
    const live* l = cv->l;
    if (chip_view_song_tab(cv)) {
        *s = cv->kr->song_want;
        const sid_chip_profile* p = sid_chip_profile_get(s->profile);
        *model = p ? p->model : (uint8_t)SID_MODEL_6581;
        return;
    }
    chip_setup_of(l, s);
    s->profile = (uint8_t)profile_of(l);
    *model = (uint8_t)l->model_req;
}

const char* chip_view_expert_name(uint32_t index) {
    static const char* const NAMES[] = {
        "6581 CURVE", "6581 RANGE", "8580 CURVE", "NONLINEARITY", "FILTER BIAS",
        "FILTER BLEED", "DAC KINK", "DAC LEAKAGE", "COMBINED", "NOISE WRITE",
        "OUTPUT THUMP", "EXTERNAL LP", "EXTERNAL HP", "VOICE DC"
    };
    _Static_assert(sizeof NAMES / sizeof NAMES[0] == SID_EXPERT_COUNT, "every expert has a popup name");
    return index < SID_EXPERT_COUNT ? NAMES[index] : "";
}

static uint32_t expert_number(char* out, uint32_t value) {
    char reverse[10];
    uint32_t n = 0;
    do {
        reverse[n++] = (char)('0' + value % 10u);
        value /= 10u;
    } while (value);
    for (uint32_t i = 0; i < n; i++) {
        out[i] = reverse[n - 1u - i];
    }
    return n;
}

static void expert_copy(char* out, const char* value) {
    uint32_t n = 0;
    for (; n < SCREEN_POPUP_CHARS && value[n]; n++) {
        out[n] = value[n];
    }
    out[n] = 0;
}

__attribute__((noinline, unused)) static void format_expert(uint32_t index, uint32_t value,
                                                          float physical, char* out) {
    const char* unit = "%";
    int decimals = 0;
    if (index >= SID_EXPERT_COUNT) {
        out[0] = 0;
        return;
    }
    if (value > SID_CHARACTER_MAX) {
        value = SID_CHARACTER_MAX;
    }
    if (index == SID_EXPERT_NOISE_WRITEBACK) {
        uint32_t taps = (value * 8u + 126u) / 127u;
        if (!taps) {
            expert_copy(out, "OFF");
        } else {
            uint32_t n = expert_number(out, taps);
            expert_copy(out + n, taps == 1u ? " TAP" : " TAPS");
        }
        return;
    }
    float scale = 100.0f;
    switch (index) {
    case SID_EXPERT_COMBINED: {
        static const char* const CHOICES[3] = {"WEAK", "MED", "STRONG"};
        const char* choice = CHOICES[(uint32_t)physical];
        expert_copy(out, choice);
        return;
    }
    case SID_EXPERT_FILTER_BIAS:
    case SID_EXPERT_THUMP:
    case SID_EXPERT_VOICE_DC:
        decimals = 1;
        unit = "DC";
        break;
    case SID_EXPERT_DAC_KINK:
        decimals = 1;
        unit = "R";
        break;
    case SID_EXPERT_DAC_LEAKAGE:
        scale = 10000.0f;
        decimals = 1;
        break;
    case SID_EXPERT_EXTERNAL_LP:
    case SID_EXPERT_EXTERNAL_HP:
        scale = 1.0f;
        unit = "HZ";
        break;
    default: break;
    }
    uint32_t scaled = (uint32_t)(physical * scale + 0.5f);
    uint32_t n = expert_number(out, decimals ? scaled / 100u : scaled);
    if (decimals) {
        out[n++] = '.';
        out[n++] = (char)('0' + scaled / 10u % 10u);
        out[n++] = (char)('0' + scaled % 10u);
    }
    for (; *unit; unit++) {
        out[n++] = *unit;
    }
    out[n] = 0;
}

void chip_view_expert_text(uint32_t index, uint32_t value, char* out) {
    if (index >= SID_EXPERT_COUNT) {
        out[0] = 0;
        return;
    }
    if (value > SID_CHARACTER_MAX) {
        value = SID_CHARACTER_MAX;
    }
    sid_character c;
    sid_character_init(&c);
    sid_character_expert_set(&c, SID_MODEL_6581, index, value);
    format_expert(index, value, sid_character_expert_value(&c, SID_MODEL_6581, index), out);
}

void chip_view_expert_setup_text(const chip_setup* setup, uint32_t index, char* out) {
    if (index >= SID_EXPERT_COUNT) {
        out[0] = 0;
        return;
    }
    const sid_chip_profile* p = sid_chip_profile_get(setup->profile);
    sid_model model = p ? (sid_model)p->model : SID_MODEL_6581;
    sid_character c;
    sid_character_init(&c);
    c.profile = (uint8_t)(setup->profile + 1u);
    c.age = setup->controls[0];
    c.dac = setup->controls[1];
    c.thump = setup->controls[2];
    c.bleed = setup->controls[3];
    sid_character_expert_set(&c, model, index, setup->expert[index]);
    uint32_t raw = index == SID_EXPERT_NOISE_WRITEBACK ? sid_character_expert_get(&c, model, index) : 0u;
    format_expert(index, raw, sid_character_expert_value(&c, model, index), out);
}

uint32_t chip_view_expert_value(const chip_view* cv, uint32_t index) {
    if (!chip_view_song_tab(cv)) {
        return index < SID_EXPERT_COUNT ? live_param_get(cv->l, LIVE_PARAM_EXPERT + index)
                                        : SID_EXPERT_DEFAULT;
    }
    const chip_setup* s = &cv->kr->song_want;
    const sid_chip_profile* p = sid_chip_profile_get(s->profile);
    sid_model model = p ? (sid_model)p->model : SID_MODEL_6581;
    sid_character c;
    sid_character_init(&c);
    c.profile = (uint8_t)(s->profile + 1u);
    c.age = s->controls[0];
    c.dac = s->controls[1];
    c.thump = s->controls[2];
    c.bleed = s->controls[3];
    if (index < SID_EXPERT_COUNT) {
        sid_character_expert_set(&c, model, index, s->expert[index]);
    }
    return sid_character_expert_get(&c, model, index);
}

static uint8_t help_kind(const chip_view* cv) {
    if (chip_view_song_tab(cv)) {
        return 1;
    }
    if (cv->tab == 1u) {
        return cv->l->two_chips && cv->l->keys_chip == 1u ? 3u : 2u;
    }
    return 0;
}

static void look(const chip_view* cv, uint8_t out[CHIP_VIEW_LOOK]) {
    const live* l = cv->l;
    uint32_t n = 0;
    int song = chip_view_song_tab(cv);
    uint8_t profile = song ? cv->kr->song_want.profile : l->profile_req;
    const uint8_t* character = song ? cv->kr->song_want.controls : l->character;
    const uint8_t* experts = song ? cv->kr->song_want.expert : l->expert;
    const sid_chip_profile* p = song ? sid_chip_profile_get(profile) : NULL;
    out[n++] = profile;
    out[n++] = song ? (p ? p->model : (uint8_t)SID_MODEL_6581) : (uint8_t)l->model_req;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        out[n++] = character[f];
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            out[n++] = l->slot[k][f];
        }
    }
    if (cv->expert) {
        for (uint32_t i = 0; i < 4u; i++) {
            uint32_t index = cv->expert_page * 4u + i;
            out[2u + LIVE_CHAR_FIELDS - 1u + i] =
                index < SID_EXPERT_COUNT ? experts[index] : SID_EXPERT_DEFAULT;
        }
    }
    out[n++] = l->fx_select;
    out[n++] = (uint8_t)(cv->chip | cv->tab << 1 | cv->tabs << 2 |
                         cv->expert << 3 | cv->expert_page << 4);
    out[n++] = l->two_chips ? (uint8_t)(1u + l->keys_chip) : 0u;
    out[n++] = help_kind(cv);
    out[n++] = (uint8_t)(picked_user_slot(cv) | cv->target << 4);
    out[n++] = (uint8_t)(cv->target && (l->user_valid >> (cv->target - 1u) & 1u));
    out[n++] = l->theme;
    out[n] = cv->hl;
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

static uint32_t focused_section(const chip_view* cv) {
    return cv->chip ? 0u : cv->expert ? CHIP_VIEW_EXPERT : 1u + cv->l->fx_select % LIVE_SLOTS;
}

static void profile_word(const chip_view* cv, const char* name, char out[6]) {
    uint32_t n = picked_user_slot(cv);
    uint32_t k = 0;
    if (n) {
        const char* u = "USR ";
        for (; *u; u++) {
            out[k++] = *u;
        }
        out[k++] = (char)('0' + n);
        out[k] = 0;
        return;
    }
    const char* w = name;
    if (name[0] >= '0' && name[0] <= '9') {
        while (*w && *w != ' ') {
            w++;
        }
        w += *w == ' ';
    }
    for (; *w && *w != ' ' && k < 5u; w++) {
        out[k++] = *w >= 'a' && *w <= 'z' ? (char)(*w - 'a' + 'A') : *w;
    }
    out[k] = 0;
}

static void draw_tabs(const chip_view* cv, canvas* c, const theme* th) {
    if (cv->expert) {
        fill(c, 0, CHIP_VIEW_TAB_ROW, GRID_COLS, 1, ' ', GRID_ATTR(th->bg, th->hot));
        text(c, 12, CHIP_VIEW_TAB_ROW, "EXPERT", GRID_ATTR(th->bg, th->hot));
        return;
    }
    uint32_t sec = focused_section(cv);
    for (uint32_t i = 0; i < CHIP_VIEW_SECTIONS; i++) {
        uint8_t a = !cv->tabs && sec == i  ? GRID_ATTR(th->bg, th->hot)
                    : cv->tabs && i == 0u ? GRID_ATTR(th->bg, th->label)
                                           : GRID_ATTR(th->label, th->bg);
        uint32_t w = TAB_W[i];
        uint32_t n = grid_text_length(TAB_NAME[i]);
        fill(c, TAB_COL[i], CHIP_VIEW_TAB_ROW, w, 1, ' ', a);
        text(c, TAB_COL[i] + (w - n) / 2u, CHIP_VIEW_TAB_ROW, TAB_NAME[i], a);
    }
}

static void draw_control(canvas* c, const theme* th, uint32_t k, const char* name, uint32_t value,
                         uint32_t max, int used) {
    uint32_t col = KNOB_COL[k];
    uint32_t cells = KNOB_W[k] - 1u;
    uint8_t accent = used ? th->knob[k] : th->dim;
    text(c, col, CHIP_VIEW_LABEL_ROW, name, GRID_ATTR(used ? th->knob_text[k] : th->dim, th->bg));
    char v[4];
    uint32_t n = 0;
    if (value >= 100u) {
        v[n++] = (char)('0' + value / 100u);
    }
    if (value >= 10u) {
        v[n++] = (char)('0' + value / 10u % 10u);
    }
    v[n++] = (char)('0' + value % 10u);
    v[n] = 0;
    uint8_t va = GRID_ATTR(used ? th->value : th->dim, th->bg);
    if (n <= 2u) {
        big(c, col, CHIP_VIEW_VALUE_ROW, 3, v, va);
    } else {
        big(c, col, CHIP_VIEW_VALUE_ROW + 1u, 2, v, va);
    }
    uint32_t px = (value > max ? max : value) * (cells * 8u) / max;
    for (uint32_t i = 0; i < cells; i++) {
        uint32_t f = px >= 8u ? 8u : px;
        px -= f;
        put(c, col + i, CHIP_VIEW_BAR_ROW, (uint8_t)(FONT_BAR0 + f), GRID_ATTR(accent, th->dim));
    }
    char tag[3] = {'K', (char)('1' + k), 0};
    text(c, col, CHIP_VIEW_TAG_ROW, tag, THEME_ATTR(th, hint));
}

static void draw_names(const chip_view* cv, canvas* c, const theme* th, const char* word) {
    const live* l = cv->l;
    uint8_t hint = THEME_ATTR(th, hint);
    if (cv->chip || cv->expert) {
        if (cv->target) {
            char title[20] = "SAVE TO USER n";
            title[13] = (char)('0' + cv->target);
            uint32_t k = text(c, 0, CHIP_VIEW_NAMES_ROW, title, THEME_ATTR(th, hot));
            if (!(l->user_valid >> (cv->target - 1u) & 1u)) {
                text(c, k, CHIP_VIEW_NAMES_ROW, " -", THEME_ATTR(th, hot));
            }
        } else if (cv->expert) {
            text(c, 0, CHIP_VIEW_NAMES_ROW, "DIRECT K1-K4   ALGORITHM PAGE", hint);
        } else {
            text(c, 0, CHIP_VIEW_NAMES_ROW, "MODEL", hint);
            uint32_t k = text(c, PROFILE_COL, CHIP_VIEW_NAMES_ROW, "PROFILE ", hint);
            text(c, k, CHIP_VIEW_NAMES_ROW, word, THEME_ATTR(th, label));
        }
    } else {
        text(c, 0, CHIP_VIEW_NAMES_ROW, "ALGORITHM", hint);
    }
}

__attribute__((noinline, unused)) static void draw_expert(chip_view* cv, canvas* c,
                                                        const theme* th, sid_model model,
                                                        const chip_setup* setup) {
    static const char* const LABEL[] = {
        "81CURVE", "81RANGE", "80CURVE", "NONLIN", "BIAS", "BLEED", "KINK", "LEAK",
        "COMB", "NOISE", "THUMP", "EXT LP", "EXT HP", "VOICEDC"
    };
    _Static_assert(sizeof LABEL / sizeof LABEL[0] == SID_EXPERT_COUNT, "every expert has a page label");
    char title[] = "EXPERT 1/4";
    title[7] = (char)('1' + cv->expert_page);
    title[9] = (char)('0' + CHIP_VIEW_EXPERT_PAGES);
    big(c, 0, CHIP_VIEW_CHOICE_ROW, 3, title, THEME_ATTR(th, hot));
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t index = cv->expert_page * 4u + k;
        if (index >= SID_EXPERT_COUNT) {
            continue;
        }
        uint32_t col = k % 2u * 15u;
        uint32_t row = 9u + k / 2u * 8u;
        int active = sid_character_expert_active(model, index);
        uint8_t accent = GRID_ATTR(active ? th->knob[k] : th->dim, th->bg);
        char tag[] = "K1";
        tag[1] = (char)('1' + k);
        text(c, col, row - 1u, tag, THEME_ATTR(th, hint));
        big(c, col, row, 2, LABEL[index], accent);
        char value[16];
        if (active) {
            chip_view_expert_setup_text(setup, index, value);
        } else {
            value[0] = 'N';
            value[1] = '/';
            value[2] = 'A';
            value[3] = 0;
        }
        uint32_t scale = grid_text_length(value) <= 4u ? 3u : 2u;
        big(c, col, row + 3u, scale, value, GRID_ATTR(active ? th->value : th->dim, th->bg));
    }
}

static void highlight(chip_view* cv) {
    uint32_t k = cv->hl - 1u;
    if (k >= 4u || cv->expert) {
        return;
    }
    chip_view_canvas* c = &cv->canvas->cv;
    const theme* th = theme_get(cv->l->theme);
    for (uint32_t col = KNOB_COL[k]; col < KNOB_COL[k] + KNOB_W[k] && col < GRID_COLS; col++) {
        if (c->ch[CHIP_VIEW_LABEL_ROW][col] == ' ') {
            break;
        }
        uint8_t* a = &c->attr[CHIP_VIEW_LABEL_ROW][col];
        if (*a != GRID_ATTR(th->dim, th->bg)) {
            *a = GRID_ATTR(th->bg, th->knob_text[k]);
        }
    }
}

static void build(chip_view* cv) {
    chip_view_canvas* c = &cv->canvas->cv;
    const live* l = cv->l;
    const theme* th = theme_get(l->theme);
    look(cv, cv->drawn);
    chip_setup s;
    uint8_t model;
    shown(cv, &s, &model);
    uint32_t sec = focused_section(cv);
    const uint8_t blank = THEME_ATTR(th, value);
    memset(c->ch, ' ', sizeof c->ch);
    memset(c->attr, blank, sizeof c->attr);
    char title[7] = "CHIP A";
    title[5] = cv->tab ? 'B' : 'A';
    big(c, 0, 0, 2, title, GRID_ATTR(cv->tabs ? th->hot : th->value, th->bg));
    if (l->two_chips && l->keys_chip == cv->tab) {
        text(c, GRID_COLS - 6u, 0, cv->tab ? "KEYS:B" : "KEYS:A", THEME_ATTR(th, hint));
    }
    draw_tabs(cv, c, th);
    draw_names(cv, c, th, picked_user_slot(cv) ? "USER" : sid_chip_profile_word(s.profile));
    if (cv->expert) {
        draw_expert(cv, c, th, (sid_model)model, &s);
    } else if (sec == 0u) {
        const sid_chip_profile* p = sid_chip_profile_get(s.profile);
        big(c, 0, CHIP_VIEW_CHOICE_ROW, 3, model == SID_MODEL_8580 ? "8580" : "6581",
            THEME_ATTR(th, value));
        char word[6];
        profile_word(cv, p ? p->name : "?", word);
        big(c, PROFILE_COL, CHIP_VIEW_CHOICE_ROW, 3, word, THEME_ATTR(th, hot));
        static const char* const NAMES[LIVE_CHAR_FIELDS - 1u] = {"AGE", "DAC", "THUMP", "BLEED"};
        for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
            draw_control(c, th, f, NAMES[f], s.controls[f], SID_CHARACTER_MAX, 1);
        }
    } else {
        uint32_t k = sec - 1u;
        const uint8_t* slot = l->slot[k];
        uint32_t alg = slot[LIVE_SLOT_ALGORITHM];
        big(c, 0, CHIP_VIEW_CHOICE_ROW, 3, views_algorithm_name(k, alg),
            alg ? THEME_ATTR(th, value) : THEME_ATTR(th, dim));
        for (uint32_t f = 0; f < 4u; f++) {
            char name[CHIP_VIEW_LABEL];
            uint32_t field = LIVE_SLOT_AMOUNT + f;
            int used = chip_view_label(k, alg, field, name);
            draw_control(c, th, f, name, slot[field], LIVE_SLOT_MAX, used);
        }
    }
    uint32_t line = 0;
    for (uint32_t i = 0; !cv->expert && i < CHIP_VIEW_SECTIONS; i++) {
        if (i == sec) {
            continue;
        }
        uint32_t row = CHIP_VIEW_SUMMARY_ROW + 2u * line++;
        big(c, 0, row, 2, TAB_NAME[i], THEME_ATTR(th, label));
        if (i == 0u) {
            const sid_chip_profile* p = sid_chip_profile_get(s.profile);
            char both[16];
            uint32_t n = 0;
            const char* m = model == SID_MODEL_8580 ? "8580" : "6581";
            for (; m[n]; n++) {
                both[n] = m[n];
            }
            both[n++] = ' ';
            char word[6];
            profile_word(cv, p ? p->name : "?", word);
            for (uint32_t j = 0; word[j] && n + 1u < sizeof both; j++) {
                both[n++] = word[j];
            }
            both[n] = 0;
            big(c, SUMMARY_VALUE_COL, row, 2, both, THEME_ATTR(th, value));
        } else {
            uint32_t alg = l->slot[i - 1u][LIVE_SLOT_ALGORITHM];
            big(c, SUMMARY_VALUE_COL, row, 2, views_algorithm_name(i - 1u, alg),
                alg ? THEME_ATTR(th, value) : THEME_ATTR(th, dim));
        }
    }
    static const char* const NOTE[4] = {"", "SONG SETUP  SLOTS SHARED", "FOLLOWS KEYS  SLOTS SHARED",
                                        "SLOTS SHARED WITH CHIP A"};
    text(c, 0, CHIP_VIEW_NOTE_ROW, NOTE[help_kind(cv)], THEME_ATTR(th, hint));
    cv->built = 1;
    cv->copying = 1;
}

static void expire(chip_view* cv, const screen* s) {
    if (cv->hl && (int32_t)(s->now - cv->hl_until) >= 0) {
        cv->hl = 0;
    }
}

void chip_view_init(chip_view* cv, live* l, song_view* shared) {
    cv->l = l;
    cv->canvas = shared;
    cv->kr = NULL;
    cv->chip = 1;
    cv->tabs = 0;
    cv->tab = 0;
    cv->expert = 0;
    cv->expert_page = 0;
    cv->built = 0;
    cv->copying = 0;
    cv->cursor = 0;
    cv->held = 0;
    cv->users = NULL;
    cv->target = 0;
    cv->confirm = 0;
    cv->msg[0] = 0;
    cv->hl = 0;
    cv->hl_until = 0;
}

void chip_view_flash(chip_view* cv, uint32_t k, uint32_t now_ms) {
    cv->hl = (uint8_t)(k + 1u);
    cv->hl_until = now_ms + SCREEN_POPUP_MS;
}

void chip_view_set_users(chip_view* cv, user_profiles* users) {
    cv->users = users;
}

void chip_view_set_keys(chip_view* cv, keys_route* kr) {
    cv->kr = kr;
}

uint32_t chip_view_focus(const chip_view* cv) {
    return cv->tabs ? CHIP_VIEW_TABS : focused_section(cv);
}

static void message(chip_view* cv, const char* a, uint32_t n, const char* b) {
    uint32_t i = 0;
    for (; *a && i + 1u < CHIP_VIEW_MSG; a++) {
        cv->msg[i++] = *a;
    }
    if (i + 1u < CHIP_VIEW_MSG) {
        cv->msg[i++] = (char)('0' + n);
    }
    for (; *b && i + 1u < CHIP_VIEW_MSG; b++) {
        cv->msg[i++] = *b;
    }
    cv->msg[i] = 0;
}

__attribute__((noinline, unused)) static chip_view_save_result save_user(chip_view* cv,
                                                                       uint32_t now_ms) {
    live* l = cv->l;
    uint32_t slot1 = cv->target ? cv->target : picked_user_slot(cv);
    if (!slot1) {
        return CHIP_VIEW_SAVE_NONE;
    }
    uint32_t slot = slot1 - 1u;
    if ((l->user_valid >> slot & 1u) && cv->confirm != slot1) {
        cv->confirm = (uint8_t)slot1;
        message(cv, "OVERWRITE USER ", slot1, "?");
        return CHIP_VIEW_SAVE_MSG;
    }
    cv->confirm = 0;
    chip_setup s;
    uint8_t model;
    shown(cv, &s, &model);
    int done = user_profile_save(cv->users, slot, s.profile, s.controls, s.expert, now_ms);
    l->settings_dirty |= 2u;
    if (done) {
        message(cv, "SAVED TO USER ", slot1, "");
        return CHIP_VIEW_SAVE_MSG;
    }
    return CHIP_VIEW_SAVE_PENDING;
}

chip_view_save_result chip_view_save(chip_view* cv, uint32_t now_ms) {
    if (!cv->users || (!cv->chip && !cv->expert) || cv->tabs) {
        return CHIP_VIEW_SAVE_NONE;
    }
    return save_user(cv, now_ms);
}

uint32_t chip_view_page(const chip_view* cv) {
    if (!cv->chip) {
        if (cv->expert) {
            return chip_view_song_tab(cv) ? PANEL_PAGE_NONE : PANEL_PAGE_EXPERT_FIRST + cv->expert_page;
        }
        return PANEL_PAGE_SLOT;
    }
    return chip_view_song_tab(cv) ? PANEL_PAGE_NONE : PANEL_PAGE_CHARACTER;
}

static void enter(screen* s, void* state) {
    (void)s;
    chip_view* cv = state;
    cv->built = 0;
    cv->cursor = 0;
    cv->target = 0;
    cv->confirm = 0;
}

static uint32_t step_clamped(uint32_t v, int32_t steps, uint32_t max) {
    int32_t n = (int32_t)v + steps;
    return n < 0 ? 0u : (uint32_t)n > max ? max : (uint32_t)n;
}

__attribute__((noinline, unused)) static void target_step(chip_view* cv, int32_t steps) {
    cv->target = (uint8_t)step_clamped(cv->target, steps, LIVE_USER_PROFILES);
    cv->confirm = 0;
}

__attribute__((noinline, unused)) static void target_clear(chip_view* cv) {
    cv->target = 0;
    cv->confirm = 0;
}

__attribute__((noinline, unused)) static void pick(screen* s, chip_view* cv, int32_t steps) {
    live* l = cv->l;
    int song = chip_view_song_tab(cv);
    int32_t last = (int32_t)(SID_CHIPS + LIVE_USER_PROFILES) - 1;
    uint32_t was = picked_user_slot(cv);
    int32_t at = was ? (int32_t)(SID_CHIPS - 1u + was) : (int32_t)(song ? cv->kr->song_want.profile : profile_of(l));
    int32_t original = at;
    int32_t dir = steps < 0 ? -1 : 1;
    for (int32_t n = steps < 0 ? -steps : steps; n > 0; n--) {
        int32_t k = at;
        int32_t skipped = -1;
        for (;;) {
            k += dir;
            if (k < 0 || k > last) {
                k = -1;
                break;
            }
            if (k < (int32_t)SID_CHIPS || (l->user_valid >> (k - (int32_t)SID_CHIPS) & 1u)) {
                break;
            }
            if (skipped < 0) {
                skipped = k - (int32_t)SID_CHIPS;
            }
        }
        if (k < 0) {
            if (skipped >= 0) {
                message(cv, "USER ", (uint32_t)skipped + 1u, " EMPTY");
                screen_toast(s, cv->msg, SCREEN_TOAST_MS);
            }
            break;
        }
        at = k;
    }
    if (at == original) {
        return;
    }
    if (at < (int32_t)SID_CHIPS) {
        if (song) {
            chip_setup want = cv->kr->song_want;
            if (want.profile != (uint32_t)at || was) {
                memset(want.expert, SID_EXPERT_DEFAULT, sizeof want.expert);
            }
            want.profile = (uint8_t)at;
            if (keys_route_song_edit(cv->kr, &want, KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_EXPERT)) {
                cv->kr->song_user = 0;
            }
        } else {
            live_param_set(l, LIVE_PARAM_PROFILE, (uint32_t)at);
        }
    } else if (song) {
        uint32_t n = (uint32_t)at - SID_CHIPS;
        chip_setup want = cv->kr->song_want;
        want.profile = l->user_base[n];
        for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
            want.controls[f] = l->user_ctl[n][f];
        }
        memcpy(want.expert, l->user_expert[n], sizeof want.expert);
        if (keys_route_song_edit(cv->kr, &want,
                                 KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_CONTROLS | KEYS_ROUTE_EDIT_EXPERT)) {
            cv->kr->song_user = (uint8_t)(n + 1u);
        }
    } else {
        live_param_set(l, LIVE_PARAM_PROFILE, (uint32_t)at);
    }
}

__attribute__((noinline, unused)) static int expert_knob(chip_view* cv, const screen_event* e) {
    if ((e->held & (1u << PANEL_BTN_EDIT)) || !chip_view_song_tab(cv)) {
        return 0;
    }
    uint32_t index = cv->expert_page * 4u + e->id - PANEL_ENC_K1;
    if (index < SID_EXPERT_COUNT) {
        chip_setup want = cv->kr->song_want;
        int32_t d = e->steps > 127 ? 127 : e->steps < -127 ? -127 : e->steps;
        want.expert[index] = (uint8_t)step_clamped(chip_view_expert_value(cv, index), d, 127u);
        keys_route_song_edit(cv->kr, &want, 1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + index));
    }
    return 1;
}

__attribute__((noinline, unused)) static int song_knob(chip_view* cv, const screen_event* e) {
    chip_setup want = cv->kr->song_want;
    uint32_t f = e->id - PANEL_ENC_K1;
    int32_t d = e->steps > 64 ? 64 : e->steps < -64 ? -64 : e->steps;
    chip_setup_control(&want, f, (uint8_t)step_clamped(want.controls[f], d * PANEL_CHARACTER_DETENT,
                                                     SID_CHARACTER_MAX));
    keys_route_song_edit(cv->kr, &want, 1u << f);
    return 1;
}

static int input(screen* s, void* state, const screen_event* e) {
    chip_view* cv = state;
    live* l = cv->l;
    if (e->kind != SCREEN_TURN) {
        return 0;
    }
    if (e->id == PANEL_ENC_SELECT) {
        uint32_t at = cv->tabs ? 0u : 1u + focused_section(cv);
        uint32_t f = step_clamped(at, e->steps, CHIP_VIEW_EXPERT + 1u);
        cv->tabs = (uint8_t)(f == 0u);
        cv->chip = (uint8_t)(f <= 1u);
        cv->expert = (uint8_t)(f == CHIP_VIEW_EXPERT + 1u);
        if (!cv->chip && !cv->expert) {
            live_fx_select(l, f - 2u);
        }
        if ((cv->target | cv->confirm) && f != 1u && !cv->expert) {
            target_clear(cv);
        }
        return 1;
    }
    if (e->id >= PANEL_ENC_K1 && e->id <= PANEL_ENC_K4) {
        if (!cv->chip) {
            return cv->expert ? expert_knob(cv, e) : 0;
        }
        if ((e->held & (1u << PANEL_BTN_EDIT)) || !chip_view_song_tab(cv)) {
            return 0;
        }
        return song_knob(cv, e);
    }
    if (e->id != PANEL_ENC_ALGORITHM) {
        return 0;
    }
    if (cv->tabs) {
        cv->tab = (uint8_t)step_clamped(cv->tab, e->steps, 1u);
        return 1;
    }
    if (!cv->chip) {
        if (cv->expert) {
            if (e->held & (1u << PANEL_BTN_EDIT)) {
                target_step(cv, e->steps);
            } else {
                cv->expert_page = (uint8_t)step_clamped(cv->expert_page, e->steps,
                                                       CHIP_VIEW_EXPERT_PAGES - 1u);
            }
            return 1;
        }
        int32_t to = (int32_t)l->slot[l->fx_select][LIVE_SLOT_ALGORITHM] + e->steps;
        live_slot_pick(l, l->fx_select, to < 0 ? 0u : (uint32_t)to);
    } else if (e->held & (1u << PANEL_BTN_EDIT)) {
        target_step(cv, e->steps);
    } else if (chip_view_song_tab(cv)) {
        pick(s, cv, e->steps);
    } else if (!l->user_applied && (int32_t)profile_of(l) + e->steps < (int32_t)SID_CHIPS) {
        uint32_t before = profile_of(l);
        uint32_t next = step_clamped(before, e->steps, SID_CHIPS - 1u);
        if (next != before) {
            live_param_set(l, LIVE_PARAM_PROFILE, next);
        }
    } else {
        pick(s, cv, e->steps);
    }
    return 1;
}

static int changed(screen* s, void* state) {
    chip_view* cv = state;
    expire(cv, s);
    return cv->canvas->which != SONG_VIEW_CANVAS_CHIP || !cv->built || cv->copying ||
           looks_changed(cv);
}

static int border(uint32_t c, uint32_t r) {
    return r == 0 || r == GRID_ROWS - 1u || c == 0 || c == GRID_COLS - 1u;
}

static void compose(screen* s, void* state, grid* g) {
    chip_view* cv = state;
    if (song_view_acquire_canvas(cv->canvas, SONG_VIEW_CANVAS_CHIP)) {
        cv->built = 0;
        cv->cursor = 0;
    }
    expire(cv, s);
    if (!cv->built || looks_changed(cv)) {
        build(cv);
        if (cv->hl) {
            highlight(cv);
        }
    }
    const chip_view_canvas* c = &cv->canvas->cv;
    uint32_t whole = s->redraw && s->wipe != SCREEN_CUT ? s->redraw_rows : 0u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (whole >> r & 1u) {
            grid_canvas_row(g, c, r);
        }
    }
    int raster = s->raster_frames && s->wipe == SCREEN_CUT;
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < CHIP_VIEW_FRAME_CELLS ? CHIP_VIEW_FRAME_CELLS - used : 0u;
    const uint8_t* cc = &c->ch[0][0];
    const uint8_t* ca = &c->attr[0][0];
    const uint8_t* cs = &c->src[0][0];
    uint32_t i = cv->cursor;
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
            cv->cursor = (uint16_t)i;
            cv->held++;
            cv->copying = 1;
            return;
        }
        grid_canvas_put(g, c, k, r);
        left--;
    }
    cv->copying = 0;
}

const view chip_view_view = {"CHIP", enter, NULL, input, compose, changed};
