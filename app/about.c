// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "about.h"

#include <stddef.h>

#include "credits.h"
#include "font.h"
#include "logo.h"
#include "sine.h"

#define BLACK GRID_ATTR(GRID_WHITE, GRID_BLACK)

/* The rows. */
#define RAMP_TOP ABOUT_BAR_ROWS
#define HEADING_ROW 12u
#define LINES_ROW 14u
#define RAMP_BOTTOM 22u
#define STAR_Y0 88u /* pixel rows 88..175: cell rows 11..21 */
#define STAR_H 88u
#define SCROLL_ROW 25u /* the scroller's middle row; it bobs 2 rows up and down */
#define SCROLL_AMP 2
#define SCROLL_ROWS 5u

/* The bars: 5 bands of 4 pixels (half a cell row), centred between half rows 2 and 17. */
#define BAR_BANDS 5u
#define BAR_MID 10
#define BAR_AMP 7
/* A turn is 256: 3 a frame is 1.8 rad/s; the second bar runs 98 behind (2.4 rad). */
#define BAR_SPEED 3u
#define BAR_OFFSET 98u
static const uint8_t BARS[ABOUT_BARS][BAR_BANDS] = {
    {GRID_LIGHT_BLUE, GRID_CYAN, GRID_WHITE, GRID_CYAN, GRID_LIGHT_BLUE},
    {GRID_RED, GRID_LIGHT_RED, GRID_YELLOW, GRID_LIGHT_RED, GRID_RED},
};

/* The credits' fade, dark to full: the heading, a line naming someone, the other lines. */
static const uint8_t FADE_HEADING[4] = {GRID_BLUE, GRID_PURPLE, GRID_LIGHT_BLUE, GRID_CYAN};
static const uint8_t FADE_NAME[4] = {GRID_DARK_GREY, GRID_GREY, GRID_LIGHT_GREY, GRID_WHITE};
static const uint8_t FADE_TEXT[4] = {GRID_DARK_GREY, GRID_DARK_GREY, GRID_GREY, GRID_LIGHT_GREY};

/* The stars' layers, far to near. */
static const uint8_t STAR_COLOR[3] = {GRID_DARK_GREY, GRID_GREY, GRID_WHITE};

/* The scroller's colour by row, top to bottom. */
static const uint8_t SCROLL_COLOR[SCROLL_ROWS] = {GRID_LIGHT_RED, GRID_YELLOW, GRID_LIGHT_GREEN,
                                                  GRID_CYAN, GRID_LIGHT_BLUE};

static const char SCROLL[] =
    "SIDOU ... THE FM-1 PLAYS LIKE A SID CHIP ... THREE VOICES, FOUR WAVES, ONE FILTER, "
    "NO SAMPLES ... EVERY PIXEL HERE IS A CELL ON A 30X30 GRID ... FREE SOFTWARE UNDER "
    "GPL-3.0 ... GREETINGS TO EVERY SID MUSICIAN ... HOME TO LEAVE ...      ";
#define SCROLL_CHARS (sizeof SCROLL - 1u)

/* The page before the credits. */
static const char* const FIRST_LINES[CREDITS_LINES] = {
    "SID SOUNDS FOR THE FM-1", "", "FREE SOFTWARE", "GPL-3.0-OR-LATER", NULL, NULL, NULL, NULL};

_Static_assert(ABOUT_LOGO_ROW + ABOUT_LOGO_ROWS <= ABOUT_BAR_ROWS,
               "the logo's slot is in the band of bars, above the first ramp line");
_Static_assert(ABOUT_ROW_FRAMES + CREDITS_LINES * ABOUT_ROW_DELAY <= ABOUT_PAGE_FRAMES,
               "a page's last row is gone before the next page");
_Static_assert(ABOUT_ROW_DELAY % 4u == 0u && ABOUT_ROW_FRAMES % 2u == 0u,
               "the rows change on even frames, two at most at once");

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

static void center(grid* g, uint32_t row, const char* s, uint8_t attr) {
    uint32_t n = length(s);
    grid_text(g, n < GRID_COLS ? (GRID_COLS - n) / 2u : 0u, row, s, attr);
}

static uint8_t bar_target(uint32_t b, uint32_t frame) {
    return (uint8_t)(BAR_MID + sine_scaled(frame * BAR_SPEED + b * BAR_OFFSET, BAR_AMP));
}

void about_init(about* a, const char* version) {
    a->version = version;
    a->frame = 0;
    for (uint32_t b = 0; b < ABOUT_BARS; b++) {
        a->bar[b] = bar_target(b, 0);
    }
}

/* Bar b moves one band toward its sine on frames 1 + 2b mod 4. */
static void move_bars(about* a, uint32_t f) {
    for (uint32_t b = 0; b < ABOUT_BARS; b++) {
        if (f % 4u != 1u + 2u * b) {
            continue;
        }
        uint8_t want = bar_target(b, f);
        if (want > a->bar[b]) {
            a->bar[b]++;
        } else if (want < a->bar[b]) {
            a->bar[b]--;
        }
    }
}

/* The colour of half row h: the last bar over it wins, black where none is. */
static uint8_t bar_color(const about* a, uint32_t h) {
    uint8_t c = GRID_BLACK;
    for (uint32_t b = ABOUT_BARS; b-- > 0;) {
        int32_t d = (int32_t)h - (int32_t)a->bar[b] + (int32_t)(BAR_BANDS / 2u);
        if (d >= 0 && d < (int32_t)BAR_BANDS) {
            c = BARS[b][d];
        }
    }
    return c;
}

static void bars(const about* a, grid* g) {
    for (uint32_t r = 0; r < ABOUT_BAR_ROWS; r++) {
        uint8_t top = bar_color(a, 2u * r);
        uint8_t bottom = bar_color(a, 2u * r + 1u);
        grid_fill(g, 0, r, GRID_COLS, 1, FONT_V4, GRID_ATTR(bottom, top));
    }
}

static void ramp_line(grid* g, uint32_t row, int reverse) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint32_t k = c * SCREEN_RAMP / GRID_COLS;
        uint8_t fg = screen_ramp[reverse ? SCREEN_RAMP - 1u - k : k];
        grid_put(g, c, row, FONT_BOX_H, GRID_ATTR(fg, GRID_BLACK));
    }
}

/* Row i of the page (0 the heading, 1.. the lines) at frame k of the page: -1 hidden, else
 * its fade level 0..3. Each row runs ABOUT_ROW_DELAY frames behind the one above it: it
 * appears at level 0, brightens a level every other frame, and fades out the same way, so at
 * most two rows change in a frame, all on even frames. */
static int32_t level_at(uint32_t i, uint32_t k) {
    int32_t x = (int32_t)k - (int32_t)(i * ABOUT_ROW_DELAY);
    int32_t shown = (int32_t)ABOUT_ROW_FRAMES;
    if (x < 0 || x >= shown) {
        return -1;
    }
    int32_t in = x / 2;
    int32_t out = (shown - 1 - x) / 2;
    int32_t l = in < out ? in : out;
    return l < 3 ? l : 3;
}

static void credits(const about* a, grid* g, uint32_t f) {
    uint32_t page = f / ABOUT_PAGE_FRAMES % (credits_page_count + 1u);
    uint32_t k = f % ABOUT_PAGE_FRAMES;
    const char* heading;
    const char* const* lines;
    uint8_t names;
    char first[24] = "SIDOU ";
    if (page == 0) {
        uint32_t n = 6;
        for (const char* s = a->version; *s && n + 1u < sizeof first;) {
            first[n++] = *s++;
        }
        first[n] = 0;
        heading = first;
        lines = FIRST_LINES;
        names = 0x01u;
    } else {
        const credits_page* p = &credits_pages[page - 1u];
        heading = p->heading;
        lines = p->line;
        names = p->names;
    }
    int32_t level = level_at(0, k);
    if (level >= 0) {
        center(g, HEADING_ROW, heading, GRID_ATTR(FADE_HEADING[level], GRID_BLACK));
    }
    for (uint32_t i = 0; i < CREDITS_LINES && lines[i]; i++) {
        level = level_at(i + 1u, k);
        if (level >= 0) {
            const uint8_t* fade = names >> i & 1u ? FADE_NAME : FADE_TEXT;
            center(g, LINES_ROW + i, lines[i], GRID_ATTR(fade[level], GRID_BLACK));
        }
    }
}

/* Row r's overlays: a mask and colour a cell, set where the mask has pixels and cleared
 * elsewhere. */
static void overlays(grid* g, uint32_t r, uint8_t mask[GRID_COLS][8], const uint8_t* color) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint8_t any = 0;
        for (uint32_t y = 0; y < 8u; y++) {
            any |= mask[c][y];
        }
        if (any) {
            grid_overlay(g, c, r, mask[c], color[c]);
        } else {
            grid_overlay_clear(g, c, r);
        }
    }
}

static uint32_t next(uint32_t* x) {
    *x ^= *x << 13;
    *x ^= *x >> 17;
    *x ^= *x << 5;
    return *x;
}

/* One-pixel stars (two pixels for the near layer) drifting left, on blank cells only. */
static void stars(grid* g, uint32_t f) {
    uint16_t sx[ABOUT_STARS];
    uint8_t sy[ABOUT_STARS];
    uint32_t seed = 7u;
    for (uint32_t i = 0; i < ABOUT_STARS; i++) {
        uint32_t layer = i % 3u;
        uint32_t x0 = next(&seed) % 240u;
        uint32_t moved = (layer == 0 ? f / 4u : layer == 1 ? f / 2u : f) % 240u;
        sx[i] = (uint16_t)((x0 + 240u - moved) % 240u);
        sy[i] = (uint8_t)(STAR_Y0 + next(&seed) % STAR_H);
    }
    for (uint32_t r = STAR_Y0 / 8u; r < (STAR_Y0 + STAR_H) / 8u; r++) {
        uint8_t mask[GRID_COLS][8] = {{0}};
        uint8_t color[GRID_COLS] = {0};
        uint8_t layer_at[GRID_COLS] = {0};
        for (uint32_t i = 0; i < ABOUT_STARS; i++) {
            if (sy[i] / 8u != r) {
                continue;
            }
            uint32_t c = sx[i] / 8u;
            if (g->ch[r * GRID_COLS + c] != ' ') {
                continue; /* the credits' text */
            }
            uint32_t layer = i % 3u;
            uint32_t bit = 0x80u >> (sx[i] % 8u);
            if (layer == 2u) {
                bit |= bit >> 1; /* its second pixel, within the cell */
            }
            mask[c][sy[i] % 8u] |= (uint8_t)bit;
            if (layer >= layer_at[c]) {
                layer_at[c] = (uint8_t)layer;
                color[c] = STAR_COLOR[layer];
            }
        }
        overlays(g, r, mask, color);
    }
}

/* The scroller: the text moves left ABOUT_SCROLL_PX pixels a frame; each screen column shows
 * the right part of one letter and the left part of the next, each in the row its letter has
 * on the sine (by its x on the screen, the wave moving as well). The wave is two screens long
 * (a turn every 448 pixels, travelling 4/256 of a turn a frame), so neighbouring letters
 * mostly share a row and words read whole. */
static void scroller(grid* g, uint32_t f) {
    uint32_t s = f * ABOUT_SCROLL_PX % (SCROLL_CHARS * 8u);
    uint32_t first = s / 8u;
    uint32_t shift = s % 8u;
    /* The rows of the 31 letters on the screen. */
    uint8_t row[GRID_COLS + 1u];
    for (uint32_t j = 0; j <= GRID_COLS; j++) {
        uint32_t x = j * 8u + 8u - shift; /* the letter's screen x, plus 8 */
        row[j] = (uint8_t)(SCROLL_ROW + sine_scaled(x * 4u / 7u + f * 4u, SCROLL_AMP));
    }
    for (uint32_t r = SCROLL_ROW - (uint32_t)SCROLL_AMP; r <= SCROLL_ROW + (uint32_t)SCROLL_AMP;
         r++) {
        uint8_t mask[GRID_COLS][8] = {{0}};
        uint8_t color[GRID_COLS];
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            color[c] = SCROLL_COLOR[r - (SCROLL_ROW - (uint32_t)SCROLL_AMP)];
            const uint8_t* left = font_glyphs[(uint8_t)SCROLL[(first + c) % SCROLL_CHARS]];
            const uint8_t* right = font_glyphs[(uint8_t)SCROLL[(first + c + 1u) % SCROLL_CHARS]];
            for (uint32_t y = 0; y < 8u; y++) {
                if (row[c] == r) {
                    mask[c][y] |= (uint8_t)(left[y] << shift);
                }
                if (shift && row[c + 1u] == r) {
                    mask[c][y] |= (uint8_t)(right[y] >> (8u - shift));
                }
            }
        }
        overlays(g, r, mask, color);
    }
}

static void about_compose(screen* s, void* state, grid* g) {
    about* a = state;
    uint32_t f = a->frame;
    /* Still while a wipe or a raster runs: they are sized for a view that does not move
     * (app/screen.h), and the cracktro's own cells on top would overrun the SPI budget. */
    if (!screen_animating(s)) {
        a->frame++;
        move_bars(a, f);
    }
    grid_fill(g, 0, 0, GRID_COLS, GRID_ROWS, ' ', BLACK);
    bars(a, g);
    logo_draw(logo_default, g, logo_center_col(logo_default), ABOUT_LOGO_ROW, f, GRID_BLACK);
    ramp_line(g, RAMP_TOP, 0);
    ramp_line(g, RAMP_BOTTOM, 1);
    credits(a, g, f);
    stars(g, f);
    scroller(g, f);
}

/* Composed every frame (no changed function). */
const view about_view = {"ABOUT", NULL, NULL, NULL, about_compose, NULL};
