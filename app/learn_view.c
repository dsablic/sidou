// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "learn_view.h"

#include <stddef.h>

#include "chip_view.h"
#include "font.h"
#include "widget.h"

/* The theme's values on its background: the frame stands out from the labels under it. */
#define ATTR_OF(l) THEME_ATTR(theme_get((l)->theme), value)
#define TEXT_COL (LEARN_VIEW_COL + 2u)
/* A toast's text, terminator included. */
#define LINE (SCREEN_TOAST_CHARS + 1u)

_Static_assert(LEARN_VIEW_COL + LEARN_VIEW_W <= GRID_COLS &&
                   LEARN_VIEW_ROW + LEARN_VIEW_H <= SCREEN_TOAST_ROW,
               "the band stays on the grid and above the toast");
_Static_assert(LEARN_VIEW_CHARS * 2u + 2u == LEARN_VIEW_W, "a cell of margin either side");

/* s after buf's text, buf holding at most cap - 1 characters. */
static void append(char* buf, uint32_t cap, const char* s) {
    uint32_t n = 0;
    while (buf[n]) {
        n++;
    }
    while (*s && n + 1u < cap) {
        buf[n++] = *s++;
    }
    buf[n] = 0;
}

/* v (0..999) in decimal after buf's text, buf holding at most cap - 1 characters. */
static void append_number(char* buf, uint32_t cap, uint32_t v) {
    uint32_t n = 0;
    while (buf[n]) {
        n++;
    }
    for (uint32_t p = v >= 100u ? 100u : v >= 10u ? 10u : 1u; p && n + 1u < cap; p /= 10u) {
        buf[n++] = (char)('0' + v / p % 10u);
    }
    buf[n] = 0;
}

_Static_assert(LIVE_PARAM_BLEED - LIVE_PARAM_PROFILE + 1 == LIVE_CHAR_FIELDS,
               "a name per character parameter");

void learn_view_param_name(uint32_t param, char* buf) {
    static const char* const KNOBS[] = {"CUTOFF", "RESO", "PULSE"};
    static const char* const ENVELOPE[] = {"ATTACK", "DECAY", "SUSTAIN", "RELEASE"};
    static const char* const CHARACTER[LIVE_CHAR_FIELDS] = {"PROFILE", "AGE", "DAC", "THUMP",
                                                           "BLEED"};
    static const char* const SLOTS[LIVE_SLOTS] = {"DIST ", "MOD ", "FX "};
    static const char* const FIELDS[LIVE_SLOT_FIELDS] = {"ALG", "AMT", "A", "B", "MIX"};
    buf[0] = 0;
    if (param <= LIVE_PARAM_PULSE_WIDTH) {
        append(buf, LEARN_VIEW_NAME, KNOBS[param]);
    } else if (param >= LIVE_PARAM_ATTACK && param <= LIVE_PARAM_RELEASE) {
        append(buf, LEARN_VIEW_NAME, ENVELOPE[param - LIVE_PARAM_ATTACK]);
    } else if (param >= LIVE_PARAM_PROFILE && param <= LIVE_PARAM_BLEED) {
        append(buf, LEARN_VIEW_NAME, CHARACTER[param - LIVE_PARAM_PROFILE]);
    } else if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t i = param - LIVE_PARAM_SLOT;
        append(buf, LEARN_VIEW_NAME, SLOTS[i / LIVE_SLOT_FIELDS]);
        append(buf, LEARN_VIEW_NAME, FIELDS[i % LIVE_SLOT_FIELDS]);
    } else if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
        append(buf, LEARN_VIEW_NAME, chip_view_expert_name(param - LIVE_PARAM_EXPERT));
    } else if (param >= LIVE_PARAM_EXPERT_END && param < LIVE_PARAM_COUNT) {
        static const char* const TAIL[LIVE_PARAM_TAIL] = {"PWM DEPTH", "PWM RATE", "TUNE"};
        append(buf, LEARN_VIEW_NAME, TAIL[param - LIVE_PARAM_EXPERT_END]);
    } else { /* not on a knob: the panel never picks it */
        append(buf, LEARN_VIEW_NAME, "PARAM ");
        append_number(buf, LEARN_VIEW_NAME, param);
    }
}

/* The band's three 2x lines, with the same short expert names as CHIP. */
static void lines(const learn_view* lv, char out[3][LEARN_VIEW_CHARS + 1u]) {
    const midi_learn* ml = lv->ml;
    char name[LEARN_VIEW_NAME];
    uint32_t cap = LEARN_VIEW_CHARS + 1u;
    out[0][0] = out[1][0] = out[2][0] = 0;
    switch (ml->state) {
    case MIDI_LEARN_CC: {
        uint8_t cc = midi_learn_cc_of(ml, ml->param);
        learn_view_param_name(ml->param, name);
        static const char* const EXPERT[SID_EXPERT_COUNT] = {
            "81CURVE", "81RANGE", "80CURVE", "NONLIN", "BIAS", "BLEED", "KINK",
            "LEAK", "COMB", "NOISE", "THUMP", "EXT LP", "EXT HP", "VOICEDC"
        };
        const char* shown = ml->param >= LIVE_PARAM_EXPERT && ml->param < LIVE_PARAM_EXPERT_END
                                ? EXPERT[ml->param - LIVE_PARAM_EXPERT]
                            : ml->param == LIVE_PARAM_PWM_DEPTH ? "PW DEPTH" /* "LEARN " fits */
                                                                : name;
        append(out[0], cap, "LEARN ");
        append(out[0], cap, shown);
        append(out[1], cap, "SEND A CC");
        if (cc != MIDI_MAP_NO_CC) {
            append(out[2], cap, cc < 100u ? "SAVE FORGET " : "SAVE FORGET");
            append_number(out[2], cap, cc);
        } else {
            append(out[2], cap, "HOME CANCEL");
        }
        break;
    }
    default: /* MIDI_LEARN_CONFIRM */
        append(out[0], cap, "CLEAR ALL?");
        append(out[1], cap, "SAVE YES");
        append(out[2], cap, "HOME NO");
        break;
    }
}

static void look(const learn_view* lv, uint8_t out[3]) {
    out[0] = lv->ml->state;
    out[1] = lv->ml->param;
    out[2] = midi_learn_cc_of(lv->ml, lv->ml->param);
}

/* The rows of the frame. */
#define FRAME_ROWS (((1u << LEARN_VIEW_H) - 1u) << LEARN_VIEW_ROW)

/* The view under it, as it composes; then the frame over it, clear of the overlays the view
 * under it draws there (About's scroller, the glyph sheet's wave). The frame covers its cells
 * (screen_cover): a view under it that holds to a budget (LIVE, SCENE) leaves them alone and
 * counts what the frame sends this frame: all of it when its text changes or the screen drew
 * over its rows (a switch, the raster), nothing otherwise (it draws the same cells again). */
static void compose(screen* s, void* state, grid* g) {
    learn_view* lv = state;
    const screen_entry* below = &s->stack[s->depth - 2u];
    uint8_t now[3];
    look(lv, now);
    int fresh = now[0] != lv->drawn[0] || now[1] != lv->drawn[1] || now[2] != lv->drawn[2] ||
                (s->redraw && (s->redraw_rows & FRAME_ROWS));
    screen_cover(s, LEARN_VIEW_COL, LEARN_VIEW_ROW, LEARN_VIEW_W, LEARN_VIEW_H,
                 fresh ? LEARN_VIEW_FRAME_CELLS : 0u);
    below->v->compose(s, below->state, g);
    for (uint32_t r = LEARN_VIEW_ROW; r < LEARN_VIEW_ROW + LEARN_VIEW_H; ++r) {
        for (uint32_t c = LEARN_VIEW_COL; c < LEARN_VIEW_COL + LEARN_VIEW_W; ++c) {
            grid_overlay_clear(g, c, r);
        }
    }
    look(lv, lv->drawn);
    char text[3][LEARN_VIEW_CHARS + 1u];
    lines(lv, text);
    const theme* th = theme_get(lv->l->theme);
    grid_fill(g, LEARN_VIEW_COL, LEARN_VIEW_ROW, LEARN_VIEW_W, LEARN_VIEW_H, ' ', THEME_ATTR(th, value));
    for (uint32_t i = 0; i < 3u; i++) {
        uint32_t n = 0;
        while (text[i][n]) {
            n++;
        }
        while (n < LEARN_VIEW_CHARS) {
            text[i][n++] = ' ';
        }
        text[i][n] = 0;
        grid_text_big(g, 1, LEARN_VIEW_ROW + 2u * i, 2, text[i],
                      i == 0u ? THEME_ATTR(th, hot) : THEME_ATTR(th, value));
        widget_blank_pieces(g, 1, LEARN_VIEW_ROW + 2u * i, LEARN_VIEW_CHARS * 2u);
    }
    for (uint32_t r = LEARN_VIEW_ROW; r < LEARN_VIEW_ROW + LEARN_VIEW_H; r++) {
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            if (g->ch[r * GRID_COLS + c] == ' ') {
                grid_put(g, c, r, ' ', THEME_ATTR(th, value));
            }
        }
    }
}

static int changed(screen* s, void* state) {
    const learn_view* lv = state;
    const screen_entry* below = &s->stack[s->depth - 2u];
    if (!below->v->changed || below->v->changed(s, below->state)) {
        return 1;
    }
    uint8_t now[3];
    look(lv, now);
    return now[0] != lv->drawn[0] || now[1] != lv->drawn[1] || now[2] != lv->drawn[2];
}

/* What the capture leaves goes to the CHIP view under the frame, as if the frame were not
 * there: SELECT moves its focus, ALGORITHM steps its choice. Only to CHIP: the menu's
 * ALGORITHM would open a view over the frame, SCENE's SELECT wipe in a piece under it. */
static int input(screen* s, void* state, const screen_event* e) {
    (void)state;
    const screen_entry* below = &s->stack[s->depth - 2u];
    return below->v == &chip_view_view ? below->v->input(s, below->state, e) : 0;
}

const view learn_view_view = {"LEARN", NULL, NULL, input, compose, changed};

void learn_view_init(learn_view* lv, live* l, midi_learn* ml) {
    lv->l = l;
    lv->ml = ml;
    lv->s = NULL;
    lv->p = NULL;
    lv->drawn[0] = lv->drawn[1] = lv->drawn[2] = 0;
}

/* Learn has ended: its toast, and the view closes (which releases the capture). */
static void finish(learn_view* lv) {
    if (!lv->s || screen_top(lv->s) != &learn_view_view) {
        return;
    }
    midi_learn* ml = lv->ml;
    char t[LINE];
    char name[LEARN_VIEW_NAME];
    t[0] = 0;
    learn_view_param_name(ml->param, name);
    switch (midi_learn_take(ml)) {
    case MIDI_LEARN_LEARNED: {
        static const char arrow[] = {' ', (char)FONT_ARROW_RIGHT, ' ', 0};
        append(t, LINE, "LEARNED CC ");
        append_number(t, LINE, ml->result_cc);
        append(t, LINE, arrow);
        append(t, LINE, name);
        break;
    }
    case MIDI_LEARN_FORGOT:
        append(t, LINE, name);
        append(t, LINE, ": CC ");
        append_number(t, LINE, ml->result_cc);
        append(t, LINE, " FORGOTTEN");
        break;
    case MIDI_LEARN_CLEARED: append(t, LINE, "MIDI LEARN CLEARED"); break;
    case MIDI_LEARN_TIMED_OUT: append(t, LINE, "LEARN TIMED OUT"); break;
    default: append(t, LINE, "LEARN CANCELLED"); break;
    }
    screen_toast(lv->s, t, SCREEN_TOAST_MS);
    screen_pop(lv->s, SCREEN_OVER);
    lv->s->redraw = 1;
    lv->s->redraw_rows |= FRAME_ROWS;
    lv->s = NULL;
}

/* Knob enc's parameter (with the buttons held), picked: 1, or 0 and a toast when it has no
 * CC (the CHIP view's song tab turns the song's setup, which no CC reaches; K3 on a sync sound
 * is the ratio, the panel's alone). */
static int pick(learn_view* lv, screen* s, const panel* p, uint32_t enc, uint32_t held) {
    uint32_t param = panel_knob_param(p, lv->l, held, enc);
    if (midi_learn_open(lv->ml, param, s->now)) {
        return 1;
    }
    int none = p && p->page == PANEL_PAGE_NONE && !(held & (1u << PANEL_BTN_EDIT));
    screen_toast(s, none ? "SONG SETUP: NO CC" : "RATIO: NO CC", SCREEN_TOAST_MS);
    return 0;
}

static int capture(void* ctx, const screen_event* e) {
    learn_view* lv = ctx;
    if (!lv->s || screen_top(lv->s) != &learn_view_view) {
        return 0;
    }
    midi_learn* ml = lv->ml;
    if (ml->state == MIDI_LEARN_IDLE) {
        /* A CC ended learn since the last frame (learn_view_frame would close it): it closes
         * now, with its toast. A knob turns as if learn were not there; HOME, meant for the
         * frame, does nothing more. */
        finish(lv);
        return e->kind == SCREEN_PRESS && e->id == PANEL_BTN_HOME;
    }
    if (e->kind == SCREEN_PRESS) {
        if (e->id != PANEL_BTN_HOME) {
            return 0; /* the panel's */
        }
        midi_learn_cancel(ml);
        finish(lv);
        return 1;
    }
    if (e->id < PANEL_ENC_K1 || e->id > PANEL_ENC_K4) {
        return 0;
    }
    (void)pick(lv, lv->s, lv->p, e->id, e->held);
    return 1; /* the knob picks, it does not turn */
}

int learn_view_open(learn_view* lv, screen* s, const panel* p, uint32_t enc, uint32_t held) {
    if (!lv->ml || lv->s) {
        return -1;
    }
    midi_learn_cancel(lv->ml); /* from idle, whatever it was in */
    (void)midi_learn_take(lv->ml);
    if (!pick(lv, s, p, enc, held) || screen_push(s, &learn_view_view, lv, SCREEN_OVER) != 0) {
        midi_learn_cancel(lv->ml);
        (void)midi_learn_take(lv->ml);
        return -1;
    }
    lv->s = s;
    lv->p = p;
    screen_capture(s, capture, lv);
    return 0;
}

int learn_view_is_open(const learn_view* lv) {
    return lv->s != NULL;
}

void learn_view_save(learn_view* lv) {
    midi_learn* ml = lv->ml;
    if (!lv->s) {
        return;
    }
    if (ml->state == MIDI_LEARN_CONFIRM) {
        midi_learn_confirm(ml);
    } else if (ml->state == MIDI_LEARN_CC && !midi_learn_forget_picked(ml, lv->s->now)) {
        char t[LINE];
        t[0] = 0;
        learn_view_param_name(ml->param, t);
        append(t, LINE, ": NO CC");
        screen_toast(lv->s, t, SCREEN_TOAST_MS); /* nothing to forget: the frame stays */
        return;
    }
    if (ml->state == MIDI_LEARN_IDLE) {
        finish(lv);
    }
}

void learn_view_ask_clear(learn_view* lv) {
    if (lv->s && lv->ml->state != MIDI_LEARN_IDLE) {
        midi_learn_ask_clear(lv->ml, lv->s->now);
    }
}

void learn_view_frame(learn_view* lv, screen* s, uint32_t now_ms) {
    if (!lv->ml || screen_top(s) != &learn_view_view) {
        return;
    }
    midi_learn_tick(lv->ml, now_ms);
    if (lv->ml->state == MIDI_LEARN_IDLE) {
        finish(lv);
    }
}
