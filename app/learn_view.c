// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "learn_view.h"

#include <stddef.h>

#include "chip_view.h"
#include "font.h"
#include "widget.h"

/* White on the screen's blue: the frame stands out from the light blue text under it. */
#define ATTR GRID_ATTR(GRID_WHITE, GRID_BLUE)
#define TEXT_COL (LEARN_VIEW_COL + 2u)
/* A toast's text, terminator included. */
#define LINE (SCREEN_TOAST_CHARS + 1u)

_Static_assert(LEARN_VIEW_COL + LEARN_VIEW_W <= GRID_COLS - 2u &&
                   LEARN_VIEW_ROW + LEARN_VIEW_H <= SCREEN_TOAST_ROW,
               "the frame stays inside the border and above the toast");
_Static_assert(LEARN_VIEW_CHARS == LEARN_VIEW_W - 4u, "the text has a cell either side");

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
    static const char* const CHARACTER[LIVE_CHAR_FIELDS] = {"PROFILE", "AGE", "DAC", "THUMP",
                                                           "BLEED"};
    static const char* const SLOTS[LIVE_SLOTS] = {"DIST ", "MOD ", "FX "};
    static const char* const FIELDS[LIVE_SLOT_FIELDS] = {"ALG", "AMT", "A", "B", "MIX"};
    buf[0] = 0;
    if (param <= LIVE_PARAM_PULSE_WIDTH) {
        append(buf, LEARN_VIEW_NAME, KNOBS[param]);
    } else if (param >= LIVE_PARAM_PROFILE && param <= LIVE_PARAM_BLEED) {
        append(buf, LEARN_VIEW_NAME, CHARACTER[param - LIVE_PARAM_PROFILE]);
    } else if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t i = param - LIVE_PARAM_SLOT;
        append(buf, LEARN_VIEW_NAME, SLOTS[i / LIVE_SLOT_FIELDS]);
        append(buf, LEARN_VIEW_NAME, FIELDS[i % LIVE_SLOT_FIELDS]);
    } else { /* not on a knob: the panel never picks it */
        append(buf, LEARN_VIEW_NAME, "PARAM ");
        append_number(buf, LEARN_VIEW_NAME, param);
    }
}

/* The frame's two lines for the state learn is in. */
static void lines(const learn_view* lv, char a[LINE], char b[LINE]) {
    const midi_learn* ml = lv->ml;
    char name[LEARN_VIEW_NAME];
    a[0] = 0;
    b[0] = 0;
    switch (ml->state) {
    case MIDI_LEARN_CC: {
        static const char arrow[] = {' ', (char)FONT_ARROW_LEFT, ' ', 0};
        uint8_t cc = midi_learn_cc_of(ml, ml->param);
        learn_view_param_name(ml->param, name);
        append(a, LINE, name);
        append(a, LINE, arrow);
        append(a, LINE, "SEND A CC");
        if (cc != MIDI_MAP_NO_CC) {
            append(b, LINE, "SAVE: FORGET CC ");
            append_number(b, LINE, cc);
        } else {
            append(b, LINE, "HOME: CANCEL");
        }
        break;
    }
    case MIDI_LEARN_CONFIRM:
        append(a, LINE, "CLEAR ALL MAPPINGS?");
        append(b, LINE, "SAVE: YES  HOME: NO");
        break;
    default: /* MIDI_LEARN_KNOB */
        append(a, LINE, "TURN A KNOB");
        append(b, LINE, "SAVE: CLEAR ALL");
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
                 fresh ? LEARN_VIEW_W * LEARN_VIEW_H : 0u);
    below->v->compose(s, below->state, g);
    for (uint32_t r = LEARN_VIEW_ROW; r < LEARN_VIEW_ROW + LEARN_VIEW_H; ++r) {
        for (uint32_t c = LEARN_VIEW_COL; c < LEARN_VIEW_COL + LEARN_VIEW_W; ++c) {
            grid_overlay_clear(g, c, r);
        }
    }
    look(lv, lv->drawn);
    char a[LINE];
    char b[LINE];
    lines(lv, a, b);
    widget_frame(g, LEARN_VIEW_COL, LEARN_VIEW_ROW, LEARN_VIEW_W, LEARN_VIEW_H, ATTR,
                 "MIDI LEARN");
    grid_fill(g, LEARN_VIEW_COL + 1u, LEARN_VIEW_ROW + 1u, LEARN_VIEW_W - 2u, 2, ' ', ATTR);
    grid_text(g, TEXT_COL, LEARN_VIEW_ROW + 1u, a, ATTR);
    grid_text(g, TEXT_COL, LEARN_VIEW_ROW + 2u, b, ATTR);
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
        if (ml->result_cc != MIDI_MAP_NO_CC) {
            append(t, LINE, ": CC ");
            append_number(t, LINE, ml->result_cc);
            append(t, LINE, " FORGOTTEN");
        } else {
            append(t, LINE, ": NO CC LEARNED");
        }
        break;
    case MIDI_LEARN_CLEARED: append(t, LINE, "MIDI LEARN CLEARED"); break;
    case MIDI_LEARN_TIMED_OUT: append(t, LINE, "LEARN TIMED OUT"); break;
    default: append(t, LINE, "LEARN CANCELLED"); break;
    }
    screen_toast(lv->s, t, SCREEN_TOAST_MS);
    screen_pop(lv->s, SCREEN_OVER);
    lv->s = NULL;
}

static int capture(void* ctx, const screen_event* e) {
    learn_view* lv = ctx;
    midi_learn* ml = lv->ml;
    if (ml->state == MIDI_LEARN_IDLE) {
        /* A CC ended learn since the last frame (learn_view_frame would close it): it closes
         * now, with its toast. A knob turns as if learn were not there; SAVE and HOME, meant
         * for the frame, do nothing more (SAVE would arm learn again over the toast). */
        finish(lv);
        return e->kind == SCREEN_PRESS && (e->id == PANEL_BTN_HOME || e->id == LEARN_VIEW_BUTTON);
    }
    if (e->kind == SCREEN_PRESS) {
        if (e->id == PANEL_BTN_HOME) {
            midi_learn_cancel(ml);
        } else if (e->id == LEARN_VIEW_BUTTON) {
            midi_learn_button(ml, lv->s->now);
        } else {
            return 0; /* the panel's */
        }
        if (ml->state == MIDI_LEARN_IDLE) {
            finish(lv);
        }
        return 1;
    }
    if (e->id < PANEL_ENC_K1 || e->id > PANEL_ENC_K4) {
        return 0;
    }
    uint32_t param = panel_knob_param(lv->p, lv->l, e->held, e->id);
    if (!midi_learn_knob(ml, param, lv->s->now) && !midi_learn_param(param)) {
        /* On the CHIP view's song tab K1..K4 turn the song's setup, which no CC reaches. */
        int none = lv->p && lv->p->page == PANEL_PAGE_NONE && !(e->held & (1u << PANEL_BTN_EDIT));
        screen_toast(lv->s, none ? "SONG SETUP: NO CC" : "K4: THE PRESET, NO CC",
                     SCREEN_TOAST_MS);
    }
    return 1; /* the knob picks, it does not turn */
}

int learn_view_open(learn_view* lv, screen* s, const panel* p) {
    if (!lv->ml || screen_push(s, &learn_view_view, lv, SCREEN_OVER) != 0) {
        return -1;
    }
    lv->s = s;
    lv->p = p;
    screen_capture(s, capture, lv);
    midi_learn_cancel(lv->ml); /* from idle, whatever it was in */
    (void)midi_learn_take(lv->ml);
    midi_learn_button(lv->ml, s->now);
    return 0;
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
