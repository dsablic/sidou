// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "boot.h"

#include <stddef.h>

#include "font.h"
#include "logo.h"
#include "panel.h"
#include "widget.h"

#define TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
#define TEXT_COL 2u
#define LOAD_ROW 8u
#define RUN_ROW 13u
/* The title card: on black, the logo between two ramp lines. */
#define TITLE_ATTR GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK)
#define TITLE_LINE_TOP 6u
#define TITLE_LOGO_ROW 8u
#define TITLE_LINE_BOTTOM 14u
#define TITLE_NAME_ROW 16u
#define TITLE_VERSION_ROW 18u
#define TITLE_KEY_ROW 25u

static const char LOAD_CMD[] = "LOAD\"SIDOU\",8,1";
static const char RUN_CMD[] = "RUN";
#define LOAD_CHARS (sizeof LOAD_CMD - 1u)
#define RUN_CHARS (sizeof RUN_CMD - 1u)

/* The loading stripes' colours (the prototype's). */
static const uint8_t STRIPES[9] = {GRID_LIGHT_BLUE,  GRID_YELLOW, GRID_RED,
                                   GRID_CYAN,        GRID_WHITE,  GRID_PURPLE,
                                   GRID_LIGHT_GREEN, GRID_BLUE,   GRID_ORANGE};

void boot_init(boot* b, const live* l, const char* version, int full) {
    static const boot empty;
    *b = empty;
    b->l = l;
    b->version = version;
    b->full = (uint8_t)(full != 0);
    b->title = (uint8_t)!b->full;
}

boot_phase boot_phase_at(const boot* b, uint32_t ms) {
    if (!b->full) {
        return ms < BOOT_SHORT_TITLE_MS ? BOOT_TITLE : BOOT_DONE;
    }
    if (ms >= BOOT_TITLE_MS) {
        return ms < BOOT_TITLE_MS + BOOT_FULL_TITLE_MS ? BOOT_TITLE : BOOT_DONE;
    }
    return ms >= BOOT_READY_MS    ? BOOT_READY
           : ms >= BOOT_LOAD_MS   ? BOOT_LOADING
           : ms >= BOOT_SEARCH_MS ? BOOT_SEARCHING
           : ms >= BOOT_TYPE_MS   ? BOOT_TYPING
                                  : BOOT_PROMPT;
}

static int notes_playing(const live* l) {
    for (uint32_t k = 0; l && k < LIVE_VOICES; k++) {
        if (l->voice[k].gate) {
            return 1;
        }
    }
    return 0;
}

boot_step boot_update(boot* b, uint32_t now_ms) {
    if (!b->started) {
        b->started = 1;
        b->start = now_ms;
    }
    b->at = now_ms - b->start;
    if (b->skip || notes_playing(b->l)) {
        return BOOT_END;
    }
    boot_phase p = boot_phase_at(b, b->at);
    if (p == BOOT_DONE) {
        return BOOT_END;
    }
    uint8_t title = (uint8_t)(p == BOOT_TITLE);
    if (title != b->title) {
        b->title = title;
        return BOOT_REVEAL;
    }
    return BOOT_GO_ON;
}

/* A boot that has just started (or restarted as the full one): HOME means the full one. The
 * key scan's press edge and held state of a key held at power-on can reach the views one key
 * scan apart, in either order (platform/fm1/input.c reads them separately), so the edge after
 * the held state must not count as a skip. */
static int home_window(const boot* b) {
    return !b->started || b->at < BOOT_HOLD_MS;
}

static void go_full(boot* b) {
    b->full = 1;
    b->started = 0; /* from the start, at the next frame */
}

int boot_capture(void* ctx, const screen_event* e) {
    boot* b = ctx;
    if (e->kind == SCREEN_PRESS && e->id == PANEL_BTN_HOME && home_window(b)) {
        go_full(b);
    } else {
        b->skip = 1;
    }
    return 1;
}

void boot_held(boot* b, uint32_t held) {
    if ((held >> PANEL_BTN_HOME & 1u) && !b->full && home_window(b)) {
        go_full(b);
    }
}

/* ---- the prompt ---- */

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* s centred on row (inside the border), then the column after it. */
static uint32_t center(grid* g, uint32_t row, const char* s, uint8_t attr) {
    uint32_t n = length(s);
    uint32_t col = n < GRID_COLS ? (GRID_COLS - n) / 2u : 0u;
    return grid_text(g, col, row, s, attr);
}

/* The first n characters of s at (col, row). */
static void typed(grid* g, uint32_t row, const char* s, uint32_t n) {
    for (uint32_t i = 0; i < n && s[i]; i++) {
        grid_put(g, TEXT_COL + i, row, (uint8_t)s[i], TEXT);
    }
}

/* A step of xorshift32: the stripes are the same for the same step. */
static uint32_t next(uint32_t* x) {
    *x ^= *x << 13;
    *x ^= *x >> 17;
    *x ^= *x << 5;
    return *x;
}

/* The border in stripes: each cell row of it two colours split at a random height (a lower
 * eighths glyph, FONT_V0..V8: the top in the background colour, the bottom in the glyph's). */
static void stripes(grid* g, uint32_t step) {
    uint32_t x = step * 7919u + 1u;
    const uint32_t last = GRID_COLS - 1u;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        uint8_t top = STRIPES[next(&x) % 9u];
        uint8_t bottom = STRIPES[next(&x) % 9u];
        uint8_t ch = (uint8_t)(FONT_V0 + 2u + next(&x) % 5u); /* 2..6 rows of the bottom */
        uint8_t attr = GRID_ATTR(bottom, top);
        if (r == 0 || r == last) {
            grid_fill(g, 0, r, GRID_COLS, 1, ch, attr);
        } else {
            grid_put(g, 0, r, ch, attr);
            grid_put(g, last, r, ch, attr);
        }
    }
}

static void prompt(boot* b, grid* g, int blink) {
    uint32_t t = b->at;
    char line[32];
    widget_screen(g, FONT_FULL, TEXT);
    /* "*** SIDOU <version> ***": at most 28 characters, inside the border. */
    static const char HEAD[] = "*** SIDOU ";
    uint32_t n = 0;
    for (const char* s = HEAD; *s;) {
        line[n++] = *s++;
    }
    for (const char* s = b->version; *s && n < 24u;) {
        line[n++] = *s++;
    }
    for (const char* s = " ***"; *s;) {
        line[n++] = *s++;
    }
    line[n] = 0;
    center(g, 3, line, TEXT);
    center(g, 5,
           b->l && b->l->model == SID_MODEL_8580 ? "CHIP 8580  3 VOICES" : "CHIP 6581  3 VOICES",
           TEXT);
    grid_text(g, TEXT_COL, 7, "READY.", TEXT);

    uint32_t chars = t < BOOT_TYPE_MS ? 0u : (t - BOOT_TYPE_MS) / BOOT_TYPE_CHAR_MS;
    if (chars > LOAD_CHARS) {
        chars = LOAD_CHARS;
    }
    typed(g, LOAD_ROW, LOAD_CMD, chars);
    uint32_t cur_col = TEXT_COL + chars;
    uint32_t cur_row = LOAD_ROW;
    int cursor = t < BOOT_SEARCH_MS;
    if (t >= BOOT_SEARCH_MS) {
        grid_text(g, TEXT_COL, 10, "SEARCHING FOR SIDOU", TEXT);
    }
    if (t >= BOOT_LOAD_MS) {
        grid_text(g, TEXT_COL, 11, "LOADING", TEXT);
    }
    if (t >= BOOT_LOAD_MS && t < BOOT_READY_MS) {
        stripes(g, (t - BOOT_LOAD_MS) / BOOT_STRIPE_MS);
    }
    if (t >= BOOT_READY_MS) {
        grid_text(g, TEXT_COL, 12, "READY.", TEXT);
        uint32_t m = t < BOOT_RUN_MS ? 0u : (t - BOOT_RUN_MS) / BOOT_RUN_CHAR_MS;
        if (m > RUN_CHARS) {
            m = RUN_CHARS;
        }
        typed(g, RUN_ROW, RUN_CMD, m);
        cur_col = TEXT_COL + m;
        cur_row = RUN_ROW;
        cursor = 1;
    }
    if (cursor && blink) {
        grid_put(g, cur_col, cur_row, FONT_FULL, TEXT);
    }
}

/* ---- the title card ---- */

static void ramp_line(grid* g, uint32_t row) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint8_t fg = screen_ramp[c * SCREEN_RAMP / GRID_COLS];
        grid_put(g, c, row, FONT_BOX_H, GRID_ATTR(fg, GRID_BLACK));
    }
}

static void title(boot* b, grid* g, int blink) {
    uint32_t from = b->full ? BOOT_TITLE_MS : 0u;
    uint32_t frame = (b->at - from) / 40u; /* the logo's animation: 25 fps */
    grid_fill(g, 0, 0, GRID_COLS, GRID_ROWS, ' ', TITLE_ATTR);
    ramp_line(g, TITLE_LINE_TOP);
    ramp_line(g, TITLE_LINE_BOTTOM);
    logo_draw(logo_default, g, logo_center_col(logo_default), TITLE_LOGO_ROW, frame, GRID_BLACK);
    center(g, TITLE_NAME_ROW, "SID FOR THE FM-1", TITLE_ATTR);
    char line[32];
    uint32_t n = 0;
    for (const char* s = b->version; *s && n < 16u;) {
        line[n++] = *s++;
    }
    for (const char* s = "  GPL-3.0"; *s;) {
        line[n++] = *s++;
    }
    line[n] = 0;
    center(g, TITLE_VERSION_ROW, line, GRID_ATTR(GRID_GREY, GRID_BLACK));
    if (blink) {
        center(g, TITLE_KEY_ROW, "PRESS ANY KEY", GRID_ATTR(GRID_WHITE, GRID_BLACK));
    }
}

/* ---- the view ---- */

static void boot_enter(screen* s, void* state) {
    screen_capture(s, boot_capture, state);
}

static void boot_compose(screen* s, void* state, grid* g) {
    boot* b = state;
    if (b->title) {
        title(b, g, screen_blink(s));
    } else {
        prompt(b, g, screen_blink(s));
    }
}

/* Composed every frame (no changed function): the cursor, the typing, the stripes and the
 * logo move most frames, and it lasts seconds. */
const view boot_view = {"BOOT", boot_enter, NULL, NULL, boot_compose, NULL};
