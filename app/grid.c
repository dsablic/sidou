// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "grid.h"

#include <stddef.h>
#include <stdint.h>

#include "bitmap_codec.h"
#include "font.h"

/* docs/art/README.md: the C64 colours after Pepto's measurements, as RGB565. */
const uint16_t grid_palette[16] = {
    0x0000, 0xFFFF, 0x69A5, 0x7536, 0x69F0, 0x5C68, 0x314F, 0xBE2D,
    0x6A64, 0x41C0, 0x9B2B, 0x4228, 0x6B6D, 0x9E90, 0x6AF6, 0x94B2,
};

#define ALL_ROWS ((1u << GRID_ROWS) - 1u)

static void mark_stale(grid* g, uint32_t i) {
    g->stale[i >> 3] |= (uint8_t)(1u << (i & 7u));
    g->rows |= 1u << (i / GRID_COLS);
}

static int dirty(const grid* g, uint32_t i) {
    return g->ch[i] != g->shown_ch[i] || g->attr[i] != g->shown_attr[i] ||
           (g->stale[i >> 3] >> (i & 7u) & 1u);
}

/* A word of the cell arrays (each starts on a word boundary: grid.h). */
typedef uint32_t __attribute__((may_alias)) cell_word;
_Static_assert(offsetof(grid, ch) % 4u == 0u && offsetof(grid, attr) % 4u == 0u &&
                   offsetof(grid, shown_ch) % 4u == 0u && offsetof(grid, shown_attr) % 4u == 0u,
               "grid.h: the cell arrays must start on word boundaries");

/* Nonzero if any of the four cells from i (a multiple of 4) is dirty (dirty()). */
static uint32_t dirty4(const grid* g, uint32_t i) {
    const cell_word* ch = (const cell_word*)(const void*)&g->ch[i];
    const cell_word* sch = (const cell_word*)(const void*)&g->shown_ch[i];
    const cell_word* at = (const cell_word*)(const void*)&g->attr[i];
    const cell_word* sat = (const cell_word*)(const void*)&g->shown_attr[i];
    return (*ch ^ *sch) | (*at ^ *sat) | (uint32_t)(g->stale[i >> 3] >> (i & 7u) & 15u);
}

void grid_invalidate(grid* g) {
    for (uint32_t i = 0; i < sizeof g->stale; i++) {
        g->stale[i] = 0xFFu;
    }
    g->rows = ALL_ROWS;
}

void grid_init(grid* g, uint8_t ch, uint8_t attr) {
    for (uint32_t i = 0; i < GRID_CELLS; i++) {
        g->ch[i] = g->shown_ch[i] = ch;
        g->attr[i] = g->shown_attr[i] = attr;
        g->ovl[i] = 0;
    }
    /* Slot 0 on top. */
    for (uint32_t s = 0; s < GRID_OVERLAYS; s++) {
        g->ovl_free[s] = (uint8_t)(GRID_OVERLAYS - 1u - s);
    }
    g->ovl_nfree = (uint8_t)GRID_OVERLAYS;
    g->hold = 0;
    g->scan_row = 0;
    g->scan_col = 0;
    g->region_bitmap = NULL;
    g->region_col = g->region_row = g->region_w = g->region_h = 0;
    g->region_bx = g->region_by = 0;
    g->region_painter = NULL;
    g->region_ctx = NULL;
    g->region_peek = NULL;
    g->region_run = 0;
    for (uint32_t i = 0; i < GRID_CELLS; i++) {
        g->src[i] = ' ';
    }
    g->big_touched = 0;
    g->big_guard = 0;
    grid_invalidate(g);
}

void grid_put(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    uint32_t i = row * GRID_COLS + col;
    /* Marks against the composed cell, not the shown one. Comparing with shown_ch/shown_attr
     * saves nothing while a screen clears before it redraws (compose_status): the clear
     * differs from the panel on every text row either way. It measured 1 % more main-loop
     * instructions per composed frame (host/crates/emu-gate/tests/screen.rs). */
    if (g->ch[i] != ch || g->attr[i] != attr) {
        g->ch[i] = ch;
        g->attr[i] = attr;
        g->rows |= 1u << row;
    }
}

void grid_row(grid* g, uint32_t row, const uint8_t ch[GRID_COLS], const uint8_t attr[GRID_COLS]) {
    if (row >= GRID_ROWS) {
        return;
    }
    uint8_t* c = &g->ch[row * GRID_COLS];
    uint8_t* a = &g->attr[row * GRID_COLS];
    uint32_t changed = 0;
    for (uint32_t i = 0; i < GRID_COLS; i++) {
        changed |= (uint32_t)(c[i] ^ ch[i]) | (uint32_t)(a[i] ^ attr[i]);
        c[i] = ch[i];
        a[i] = attr[i];
    }
    if (changed) {
        g->rows |= 1u << row;
    }
}

uint32_t grid_text(grid* g, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    while (*s && col < GRID_COLS) {
        grid_put(g, col++, row, (uint8_t)*s++, attr);
    }
    return col;
}

void grid_fill(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
               uint8_t attr) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    uint32_t c1 = w < GRID_COLS - col ? col + w : GRID_COLS;
    uint32_t r1 = h < GRID_ROWS - row ? row + h : GRID_ROWS;
    for (uint32_t r = row; r < r1; r++) {
        uint32_t changed = 0; /* against the composed cells, as in grid_put */
        for (uint32_t i = r * GRID_COLS + col; i < r * GRID_COLS + c1; i++) {
            changed |= (uint32_t)(g->ch[i] ^ ch) | (uint32_t)(g->attr[i] ^ attr);
            g->ch[i] = ch;
            g->attr[i] = attr;
        }
        if (changed) {
            g->rows |= 1u << r;
        }
    }
}

void grid_box(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
              int round) {
    if (w < 2u || h < 2u) {
        return;
    }
    uint32_t right = col + w - 1u;
    uint32_t bottom = row + h - 1u;
    grid_fill(g, col + 1u, row, w - 2u, 1, FONT_BOX_H, attr);
    grid_fill(g, col + 1u, bottom, w - 2u, 1, FONT_BOX_H, attr);
    grid_fill(g, col, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    grid_fill(g, right, row + 1u, 1, h - 2u, FONT_BOX_V, attr);
    grid_put(g, col, row, round ? FONT_ROUND_TL : FONT_BOX_TL, attr);
    grid_put(g, right, row, round ? FONT_ROUND_TR : FONT_BOX_TR, attr);
    grid_put(g, col, bottom, round ? FONT_ROUND_BL : FONT_BOX_BL, attr);
    grid_put(g, right, bottom, round ? FONT_ROUND_BR : FONT_BOX_BR, attr);
}

uint32_t grid_text_big(grid* g, uint32_t col, uint32_t row, uint32_t scale, const char* s,
                       uint8_t attr) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    if (col >= GRID_COLS || scale < 2u || scale > 3u) {
        return col;
    }
    if (n * scale > GRID_COLS - col) {
        n = (GRID_COLS - col) / scale;
    }
    uint32_t first = scale == 2u ? GRID_BIG_FIRST : GRID_BIG_FIRST + 4u;
    for (uint32_t y = 0; y < scale && row + y < GRID_ROWS; y++) {
        uint32_t r = row + y;
        if (g->big_guard >> r & 1u) {
            continue;
        }
        uint32_t cell = r * GRID_COLS + col;
        uint8_t* ch = &g->ch[cell];
        uint8_t* at = &g->attr[cell];
        uint8_t* src = &g->src[cell];
        uint8_t* stale = &g->stale[cell >> 3];
        uint32_t bit = cell & 7u;
        uint32_t changed = 0;
        uint32_t touched = 0;
        for (uint32_t i = 0; i < n; i++) {
            uint8_t real = (uint8_t)s[i];
            uint8_t piece = (uint8_t)(first + y * scale);
            for (uint32_t x = 0; x < scale; x++, piece++) {
                changed |= (uint32_t)(*ch ^ piece) | (uint32_t)(*at ^ attr);
                *ch++ = piece;
                *at++ = attr;
                if (*src != real) {
                    *src = real;
                    *stale |= (uint8_t)(1u << bit);
                    touched = 1;
                }
                src++;
                if (++bit == 8u) {
                    bit = 0;
                    stale++;
                }
            }
        }
        if (touched) {
            g->big_touched = 1;
        }
        if (changed | touched) {
            g->rows |= 1u << r;
        }
    }
    return col + n * scale;
}

void grid_canvas_text(grid_canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = attr;
    }
}

uint32_t grid_canvas_text_big(grid_canvas* c, uint32_t col, uint32_t row, uint32_t scale,
                              const char* s, uint8_t attr) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    if (col >= GRID_COLS || scale < 2u || scale > 3u) {
        return col;
    }
    if (n * scale > GRID_COLS - col) {
        n = (GRID_COLS - col) / scale;
    }
    uint32_t first = scale == 2u ? GRID_BIG_FIRST : GRID_BIG_FIRST + 4u;
    for (uint32_t y = 0; y < scale && row + y < GRID_ROWS; y++) {
        uint8_t* ch = &c->ch[row + y][col];
        uint8_t* at = &c->attr[row + y][col];
        uint8_t* src = &c->src[row + y][col];
        uint8_t piece = (uint8_t)(first + y * scale);
        for (uint32_t i = 0; i < n; i++) {
            for (uint32_t x = 0; x < scale; x++) {
                *ch++ = (uint8_t)(piece + x);
                *at++ = attr;
                *src++ = (uint8_t)s[i];
            }
        }
    }
    return col + n * scale;
}

int grid_canvas_differs(const grid* g, const grid_canvas* c, uint32_t col, uint32_t row) {
    uint32_t i = row * GRID_COLS + col;
    uint8_t ch = c->ch[row][col];
    return g->ch[i] != ch || g->attr[i] != c->attr[row][col] ||
           ((uint8_t)(ch - GRID_BIG_FIRST) < GRID_BIG_PIECES && g->src[i] != c->src[row][col]);
}

void grid_canvas_put(grid* g, const grid_canvas* c, uint32_t col, uint32_t row) {
    uint32_t i = row * GRID_COLS + col;
    uint8_t ch = c->ch[row][col];
    grid_put(g, col, row, ch, c->attr[row][col]);
    if ((uint8_t)(ch - GRID_BIG_FIRST) < GRID_BIG_PIECES && g->src[i] != c->src[row][col]) {
        g->src[i] = c->src[row][col];
        g->big_touched = 1;
        mark_stale(g, i);
    }
}

void grid_canvas_row(grid* g, const grid_canvas* c, uint32_t row) {
    if (row >= GRID_ROWS) {
        return;
    }
    uint8_t* gc = &g->ch[row * GRID_COLS];
    uint8_t* ga = &g->attr[row * GRID_COLS];
    const uint8_t* cc = c->ch[row];
    const uint8_t* ca = c->attr[row];
    uint32_t changed = 0;
    uint32_t pieces = 0;
    for (uint32_t i = 0; i < GRID_COLS; i++) {
        changed |= (uint32_t)(gc[i] ^ cc[i]) | (uint32_t)(ga[i] ^ ca[i]);
        pieces |= (uint32_t)((uint8_t)(cc[i] - GRID_BIG_FIRST) < GRID_BIG_PIECES);
        gc[i] = cc[i];
        ga[i] = ca[i];
    }
    if (changed) {
        g->rows |= 1u << row;
    }
    if (pieces) {
        for (uint32_t i = 0; i < GRID_COLS; i++) {
            uint32_t k = row * GRID_COLS + i;
            if ((uint8_t)(cc[i] - GRID_BIG_FIRST) < GRID_BIG_PIECES && g->src[k] != c->src[row][i]) {
                g->src[k] = c->src[row][i];
                g->big_touched = 1;
                mark_stale(g, k);
            }
        }
    }
}

void grid_put_top(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    uint32_t i = row * GRID_COLS + col;
    if (g->ch[i] != GRID_PIECE_PLAIN || g->attr[i] != attr) {
        g->ch[i] = (uint8_t)GRID_PIECE_PLAIN;
        g->attr[i] = attr;
        g->rows |= 1u << row;
    }
    if (g->src[i] != ch) {
        g->src[i] = ch;
        g->big_touched = 1;
        mark_stale(g, i);
    }
}

void grid_fill_top(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                   uint8_t attr) {
    for (uint32_t r = row; r < row + h; r++) {
        for (uint32_t c = col; c < col + w; c++) {
            grid_put_top(g, c, r, ch, attr);
        }
    }
}

void grid_big_guard(grid* g, uint32_t rows) {
    g->big_guard = rows;
}

int grid_big_touched(grid* g) {
    int t = g->big_touched;
    g->big_touched = 0;
    return t;
}

void grid_unstale(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h) {
    uint32_t c1 = col < GRID_COLS && w < GRID_COLS - col ? col + w : GRID_COLS;
    uint32_t r1 = row < GRID_ROWS && h < GRID_ROWS - row ? row + h : GRID_ROWS;
    for (uint32_t r = row; r < r1; r++) {
        for (uint32_t c = col; c < c1; c++) {
            uint32_t i = r * GRID_COLS + c;
            g->stale[i >> 3] &= (uint8_t)~(1u << (i & 7u));
        }
    }
}

int grid_overlay(grid* g, uint32_t col, uint32_t row, const uint8_t mask[8], uint8_t color) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return 0;
    }
    uint32_t i = row * GRID_COLS + col;
    uint32_t s;
    if (g->ovl[i]) {
        s = g->ovl[i] - 1u;
        int same = g->ovl_color[s] == color;
        for (uint32_t y = 0; y < 8u && same; y++) {
            same = g->ovl_mask[s][y] == mask[y];
        }
        if (same) {
            return 0;
        }
    } else {
        if (g->ovl_nfree == 0) {
            return -1;
        }
        s = g->ovl_free[--g->ovl_nfree];
        g->ovl[i] = (uint8_t)(s + 1u);
    }
    for (uint32_t y = 0; y < 8u; y++) {
        g->ovl_mask[s][y] = mask[y];
    }
    g->ovl_color[s] = color;
    mark_stale(g, i);
    return 0;
}

static void overlay_free(grid* g, uint32_t i) {
    g->ovl_free[g->ovl_nfree++] = (uint8_t)(g->ovl[i] - 1u);
    g->ovl[i] = 0;
    mark_stale(g, i);
}

void grid_overlay_clear(grid* g, uint32_t col, uint32_t row) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    uint32_t i = row * GRID_COLS + col;
    if (g->ovl[i]) {
        overlay_free(g, i);
    }
}

void grid_overlay_color(grid* g, uint32_t col, uint32_t row, uint8_t color) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    uint32_t i = row * GRID_COLS + col;
    if (g->ovl[i] && g->ovl_color[g->ovl[i] - 1u] != color) {
        g->ovl_color[g->ovl[i] - 1u] = color;
        mark_stale(g, i);
    }
}

void grid_overlay_clear_all(grid* g) {
    for (uint32_t i = 0; i < GRID_CELLS && g->ovl_nfree < GRID_OVERLAYS; i++) {
        if (g->ovl[i]) {
            overlay_free(g, i);
        }
    }
}

int grid_overlay_same(const grid* g, uint32_t col, uint32_t row, const uint8_t mask[8],
                      uint8_t color) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return 1;
    }
    uint32_t i = row * GRID_COLS + col;
    if (!g->ovl[i]) {
        uint32_t any = 0;
        for (uint32_t y = 0; y < 8u; y++) {
            any |= mask[y];
        }
        return !any;
    }
    uint32_t s = g->ovl[i] - 1u;
    if (g->ovl_color[s] != color) {
        return 0;
    }
    for (uint32_t y = 0; y < 8u; y++) {
        if (g->ovl_mask[s][y] != mask[y]) {
            return 0;
        }
    }
    return 1;
}

/* Cells [a, b) in screen order to be sent again, eight a store where it can (a view switch
 * clears a full-screen region in the same frame as the next view's first compose). The
 * caller marks their rows. */
static void stale_range(grid* g, uint32_t a, uint32_t b) {
    for (; a < b && (a & 7u); a++) {
        g->stale[a >> 3] |= (uint8_t)(1u << (a & 7u));
    }
    for (; a + 8u <= b; a += 8u) {
        g->stale[a >> 3] = 0xFFu;
    }
    for (; a < b; a++) {
        g->stale[a >> 3] |= (uint8_t)(1u << (a & 7u));
    }
}

/* The region's cells, to be sent again. */
static void region_stale(grid* g) {
    uint32_t r0 = g->region_row;
    uint32_t r1 = r0 + g->region_h;
    if (g->region_w == GRID_COLS) {
        stale_range(g, r0 * GRID_COLS, r1 * GRID_COLS); /* whole rows: one range */
    } else {
        for (uint32_t r = r0; r < r1; r++) {
            uint32_t i = r * GRID_COLS + g->region_col;
            stale_range(g, i, i + g->region_w);
        }
    }
    g->rows |= ((1u << g->region_h) - 1u) << r0;
}

/* The cells in one of the region and the w x h cells from (col, row) but not in both, to be
 * sent again. */
static void region_moved(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h) {
    uint32_t oc = g->region_col;
    uint32_t orow = g->region_row;
    uint32_t c0 = col < oc ? col : oc;
    uint32_t r0 = row < orow ? row : orow;
    uint32_t c1 = col + w > oc + g->region_w ? col + w : oc + g->region_w;
    uint32_t r1 = row + h > orow + g->region_h ? row + h : orow + g->region_h;
    for (uint32_t r = r0; r < r1; r++) {
        for (uint32_t c = c0; c < c1; c++) {
            int in_new = c - col < w && r - row < h;
            int in_old = c - oc < g->region_w && r - orow < g->region_h;
            if (in_new != in_old) {
                mark_stale(g, r * GRID_COLS + c);
            }
        }
    }
}

int grid_region(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                const grid_bitmap* b, uint32_t bx, uint32_t by) {
    uint32_t bw = b ? b->w / GRID_CELL_PX : 0u;
    uint32_t bh = b ? b->h / GRID_CELL_PX : 0u;
    if (!b || w == 0 || h == 0 || col >= GRID_COLS || row >= GRID_ROWS ||
        w > GRID_COLS - col || h > GRID_ROWS - row || bx > bw || w > bw - bx || by > bh ||
        h > bh - by) {
        return -1;
    }
    if (g->region_bitmap == b && g->region_col == col && g->region_row == row &&
        g->region_w == w && g->region_h == h && g->region_bx == bx && g->region_by == by) {
        return 0;
    }
    /* Over the same bitmap cells (each cell in both shows the same cell of it): only the cells
     * it adds or leaves change. */
    int same = g->region_bitmap == b &&
               (int32_t)col - (int32_t)bx == (int32_t)g->region_col - (int32_t)g->region_bx &&
               (int32_t)row - (int32_t)by == (int32_t)g->region_row - (int32_t)g->region_by;
    if (same) {
        region_moved(g, col, row, w, h);
    } else if (g->region_bitmap || g->region_painter) {
        region_stale(g);
    }
    g->region_bitmap = b;
    g->region_painter = NULL;
    g->region_ctx = NULL;
    g->region_col = (uint8_t)col;
    g->region_row = (uint8_t)row;
    g->region_w = (uint8_t)w;
    g->region_h = (uint8_t)h;
    g->region_bx = (uint8_t)bx;
    g->region_by = (uint8_t)by;
    if (!same) {
        region_stale(g);
    }
    return 0;
}

int grid_region_painter(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                        grid_painter fn, void* ctx, uint32_t run) {
    if (!fn || w == 0 || h == 0 || col >= GRID_COLS || row >= GRID_ROWS ||
        w > GRID_COLS - col || h > GRID_ROWS - row || run == 0 || run > GRID_COLS) {
        return -1;
    }
    if (g->region_painter == fn && g->region_ctx == ctx && g->region_col == col &&
        g->region_row == row && g->region_w == w && g->region_h == h && g->region_run == run) {
        return 0;
    }
    if (g->region_bitmap || g->region_painter) {
        region_stale(g);
    }
    g->region_bitmap = NULL;
    if (g->region_painter != fn || g->region_ctx != ctx) {
        g->region_peek = NULL;
    }
    g->region_painter = fn;
    g->region_ctx = ctx;
    g->region_col = (uint8_t)col;
    g->region_row = (uint8_t)row;
    g->region_w = (uint8_t)w;
    g->region_h = (uint8_t)h;
    g->region_bx = g->region_by = 0;
    g->region_run = (uint8_t)run;
    region_stale(g);
    return 0;
}

void grid_region_clear(grid* g) {
    if (g->region_bitmap || g->region_painter) {
        region_stale(g);
        g->region_bitmap = NULL;
        g->region_painter = NULL;
        g->region_ctx = NULL;
    }
}

void grid_mark(grid* g, uint32_t col, uint32_t row) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        mark_stale(g, row * GRID_COLS + col);
    }
}

int grid_marked(const grid* g, uint32_t col, uint32_t row) {
    return col < GRID_COLS && row < GRID_ROWS && dirty(g, row * GRID_COLS + col);
}

/* The run that holds pixel x of row y, and how many pixels of it are left from x on. */
__attribute__((always_inline)) static inline uint32_t bitmap_seek(
    const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t* at, uint32_t* left, int raw) {
    uint32_t pixel;
    *at = bitmap_row_mode(b->data, b->rows[y], b->w, x, &pixel, raw);
    uint32_t byte = bitmap_run_mode(b->data, at, raw);
    uint32_t end = pixel + (byte >> 4) + 1u;
    while (x >= end) {
        byte = bitmap_run_mode(b->data, at, raw);
        end += (byte >> 4) + 1u;
    }
    *left = end - x;
    return byte;
}

void grid_bitmap_row(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    if (!n) {
        return;
    }
    uint32_t at, left;
    uint32_t byte = bitmap_seek(b, x, y, &at, &left, b->data[15] & 128u);
    const uint8_t* data = b->data;
    for (;;) {
        uint8_t c = (uint8_t)(byte & 15u);
        uint32_t k = left < n ? left : n;
        n -= k;
        while (k--) {
            *out++ = c;
        }
        if (!n) {
            return;
        }
        byte = bitmap_run(data, &at);
        left = (byte >> 4) + 1u;
    }
}


uint32_t grid_pending(const grid* g) {
    uint32_t n = 0;
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (!(g->rows >> r & 1u)) {
            continue;
        }
        for (uint32_t i = r * GRID_COLS; i < (r + 1u) * GRID_COLS; i++) {
            n += (uint32_t)dirty(g, i);
        }
    }
    return n;
}

uint32_t grid_pending_in(const grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h) {
    uint32_t n = 0;
    uint32_t c1 = col + w < GRID_COLS ? col + w : GRID_COLS;
    uint32_t r1 = row + h < GRID_ROWS ? row + h : GRID_ROWS;
    for (uint32_t r = row; r < r1; r++) {
        if (!(g->rows >> r & 1u)) {
            continue;
        }
        for (uint32_t c = col; c < c1; c++) {
            n += (uint32_t)dirty(g, r * GRID_COLS + c);
        }
    }
    return n;
}

void grid_hold(grid* g, uint32_t mask) {
    g->hold = mask & ALL_ROWS;
}

int grid_next_run(grid* g, grid_run* run) {
    while (g->rows & ~g->hold) {
        if (g->scan_row >= GRID_ROWS) {
            g->scan_row = 0;
            g->scan_col = 0;
        }
        uint32_t row = g->scan_row;
        uint32_t bit = 1u << row;
        if (!(g->rows & bit) || (g->hold & bit)) {
            g->scan_row++;
            g->scan_col = 0;
            continue;
        }
        uint32_t base = row * GRID_COLS;
        uint32_t c = g->scan_col;
        while (c < GRID_COLS) {
            uint32_t i = base + c;
            /* Four clean cells at a time from a word boundary: most of a row is clean. */
            if (!(i & 3u) && c + 4u <= GRID_COLS && !dirty4(g, i)) {
                c += 4u;
                continue;
            }
            if (dirty(g, i)) {
                break;
            }
            c++;
        }
        if (c == GRID_COLS) {
            /* Clean from scan_col on. Only a scan of the whole row may clear its bit: cells
             * left of scan_col may have changed since the scan passed them. */
            if (g->scan_col == 0) {
                g->rows &= ~bit;
                g->scan_row++;
            }
            g->scan_col = 0;
            continue;
        }
        uint32_t start = c;
        /* A painter region's cells, [p0, p1) of the row, end the run at its cap. */
        uint32_t p0 = 0, p1 = 0, left = GRID_COLS;
        if (g->region_painter && row - g->region_row < g->region_h) {
            p0 = g->region_col;
            p1 = p0 + g->region_w;
            left = g->region_run;
        }
        for (; c < GRID_COLS && dirty(g, base + c); c++) {
            if (c - p0 < p1 - p0 && !left--) {
                break;
            }
            uint32_t i = base + c;
            g->shown_ch[i] = g->ch[i];
            g->shown_attr[i] = g->attr[i];
            g->stale[i >> 3] &= (uint8_t)~(1u << (i & 7u));
        }
        g->scan_col = (uint8_t)c;
        run->row = (uint8_t)row;
        run->col = (uint8_t)start;
        run->len = (uint8_t)(c - start);
        return 1;
    }
    return 0;
}

/* Two pixels a 32-bit store, the left one at the lower address (PAIR): a cell row is four
 * stores. The pairs alias the uint16_t pixels (may_alias). */
typedef uint32_t __attribute__((may_alias)) pixel_pair;
/* A glyph's 8 rows as two words (font_glyphs is 4-byte aligned, font.h). */
typedef uint32_t __attribute__((may_alias)) glyph_word;
#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define PAIR(left, right) ((uint32_t)(left) << 16 | (uint32_t)(right))
#else
#define PAIR(left, right) ((uint32_t)(right) << 16 | (uint32_t)(left))
#endif

/* k pixels of colour c (a pair: two pixels of it) from o, two a 32-bit store where the pairs
 * are 4-byte aligned; the pixel after them. */
static inline uint16_t* fill_pixels(uint16_t* o, uint32_t k, uint32_t c) {
    uint16_t* e = o + k;
    if (((uintptr_t)o & 2u) && o < e) {
        *o++ = (uint16_t)c;
    }
    for (; o + 2 <= e; o += 2) {
        *(pixel_pair*)(void*)o = c;
    }
    if (o < e) {
        *o++ = (uint16_t)c;
    }
    return o;
}

/* As grid_bitmap_row, as the panel's pixels into out (4-byte aligned), each colour given as a
 * pair (pairs[c], two pixels of it). While the longest run still fits before the end, runs are
 * taken whole with no clamp, and a run of 1 or 2 pixels (a dithered picture is mostly those)
 * is two stores with no loop; the first run (entered part way) and the last ones are clamped. */
static void bitmap_raw_pixels(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n,
                              const uint32_t pairs[16], uint16_t* restrict out) {
    uint32_t pixel;
    uint32_t at = bitmap_row_mode(b->data, b->rows[y], b->w, x, &pixel, 1);
    const uint8_t* p = b->data + (at >> 1);
    uint32_t end_pixel = pixel + (uint32_t)(*p >> 4) + 1u;
    while (x >= end_pixel) {
        end_pixel += (uint32_t)(*++p >> 4) + 1u;
    }
    uint32_t left = end_pixel - x;
    uint16_t* const end = out + n;
    uint16_t* o = fill_pixels(out, left < n ? left : n, pairs[*p & 15u]);
    uint16_t* const fast = n > 16u ? end - 16 : out;
    while (o <= fast) {
        uint32_t byte = *++p;
        uint32_t c = pairs[byte & 15u];
        uint32_t m = byte >> 4;
        if (m < 2u) {
            o[0] = (uint16_t)c;
            o[m] = (uint16_t)c;
            o += m + 1u;
        } else {
            o = fill_pixels(o, m + 1u, c);
        }
    }
    while (o < end) {
        uint32_t byte = *++p;
        uint32_t k = (byte >> 4) + 1u;
        uint32_t room = (uint32_t)(end - o);
        o = fill_pixels(o, k < room ? k : room, pairs[byte & 15u]);
    }
}

static void bitmap_pixels(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n,
                          const uint32_t pairs[16], uint16_t* restrict out) {
    if (b->data[15] & 128u) {
        bitmap_raw_pixels(b, x, y, n, pairs, out);
        return;
    }
    uint32_t at, left;
    uint32_t first = bitmap_seek(b, x, y, &at, &left, 0);
    const uint8_t* data = b->data;
    uint16_t* const end = out + n;
    uint16_t* o = fill_pixels(out, left < n ? left : n, pairs[first & 15u]);
    uint16_t* const fast = n > 16u ? end - 16 : out;
    while (o <= fast) {
        uint32_t byte = bitmap_code_run(data, &at);
        uint32_t c = pairs[byte & 15u];
        uint32_t m = byte >> 4; /* the run's length - 1 */
        if (m < 2u) {
            o[0] = (uint16_t)c;
            o[m] = (uint16_t)c;
            o += m + 1u;
        } else {
            o = fill_pixels(o, m + 1u, c);
        }
    }
    while (o < end) {
        uint32_t byte = bitmap_code_run(data, &at);
        uint32_t k = (byte >> 4) + 1u;
        uint32_t room = (uint32_t)(end - o);
        o = fill_pixels(o, k < room ? k : room, pairs[byte & 15u]);
    }
}

/* A pair's two bits (the left one the higher) as which of its pixels are set. */
static const uint32_t PAIR_MASK[4] = {PAIR(0u, 0u), PAIR(0u, 0xFFFFu), PAIR(0xFFFFu, 0u),
                                      PAIR(0xFFFFu, 0xFFFFu)};

/* The region's cells [k0, k1) of a run (k counted from the run's first cell): the bitmap's
 * pixels, then the glyphs' foreground and the overlays over them. */
static void render_region(const grid* g, const grid_run* run, uint32_t k0, uint32_t k1,
                          const uint16_t pal[16], uint16_t* px) {
    uint32_t w = run->len * GRID_CELL_PX; /* pixels a row */
    {
        const grid_bitmap* b = g->region_bitmap;
        /* Palette pairs and painter pixels never live together. */
        union {
            uint32_t pairs[16];
            uint8_t cell[64];
        } scratch;
        if (b) {
            uint32_t x = (g->region_bx + run->col + k0 - g->region_col) * GRID_CELL_PX;
            uint32_t y = (g->region_by + run->row - g->region_row) * GRID_CELL_PX;
            for (uint32_t c = 0; c < 16u; c++) {
                scratch.pairs[c] = PAIR(pal[c], pal[c]);
            }
            for (uint32_t r = 0; r < 8u; r++) {
                bitmap_pixels(b, x, y + r, (k1 - k0) * GRID_CELL_PX, scratch.pairs,
                              px + r * w + k0 * GRID_CELL_PX);
            }
        } else {
            /* A painter: each cell's 64 colours, then the panel's pixels. */
            for (uint32_t k = k0; k < k1; k++) {
                g->region_painter(g->region_ctx, run->col + k, run->row, scratch.cell);
                uint16_t* o = px + k * GRID_CELL_PX;
                for (uint32_t r = 0; r < 8u; r++, o += w) {
                    const uint8_t* c = scratch.cell + r * 8u;
                    for (uint32_t j = 0; j < 8u; j++) {
                        o[j] = pal[c[j] & 15u];
                    }
                }
            }
        }
    }
    uint32_t i = run->row * GRID_COLS + run->col + k0;
    for (uint32_t k = k0; k < k1; k++, i++) {
        const uint8_t* glyph = font_glyph(g->ch[i]);
        uint16_t fg = pal[g->attr[i] & 15u];
        static const uint8_t none[8];
        const uint8_t* mask = none;
        uint16_t oc = 0;
        if (g->ovl[i]) {
            uint32_t s = g->ovl[i] - 1u;
            mask = g->ovl_mask[s];
            oc = pal[g->ovl_color[s] & 15u];
        }
        uint16_t* o = px + k * GRID_CELL_PX;
        for (uint32_t r = 0; r < 8u; r++, o += w) {
            uint32_t bits = glyph[r];
            uint32_t over = mask[r];
            if (!(bits | over)) {
                continue; /* the bitmap alone: most of a picture's cells */
            }
            for (uint32_t j = 0; j < 8u; j++) {
                uint32_t bit = 0x80u >> j;
                if (over & bit) {
                    o[j] = oc;
                } else if (bits & bit) {
                    o[j] = fg;
                }
            }
        }
    }
}

/* Pixel p (0..23) of a glyph drawn at scale 1, 2 or 3: the pixel of the 8 x 8 glyph it shows (p,
 * p / 2, p / 3 as a multiply: exact for p < 24). */
static uint32_t big_src(uint32_t p, uint32_t scale) {
    return scale == 1u ? p : scale == 2u ? p >> 1 : p * 11u >> 5;
}

/* The cell at i, a piece (0..GRID_BIG_PIECES - 1), as pixels at o with w pixels to a row: its
 * part of its character's glyph pixel-doubled or tripled, or (the last) the glyph as it is. */
__attribute__((noinline, unused)) static void render_piece(const grid* g, uint32_t i,
                                                           uint32_t piece, uint16_t fg,
                                                           uint16_t bg, uint16_t* o,
                                                           uint32_t w) {
    uint32_t scale = piece < 4u ? 2u : piece < 13u ? 3u : 1u;
    uint32_t q = piece < 4u ? piece : piece < 13u ? piece - 4u : 0u; /* y * scale + x */
    uint32_t y0 = big_src(q, scale) * 8u;          /* the cell's pixel row in the character */
    uint32_t x0 = (q - big_src(q, scale) * scale) * 8u;
    const uint8_t* glyph = font_glyph(g->src[i]);
    uint32_t sx[8];
    for (uint32_t j = 0; j < 8u; j++) {
        sx[j] = 7u - big_src(x0 + j, scale);
    }
    for (uint32_t y = 0; y < 8u; y++, o += w) {
        uint32_t bits = glyph[big_src(y0 + y, scale)];
        for (uint32_t j = 0; j < 8u; j++) {
            o[j] = bits >> sx[j] & 1u ? fg : bg;
        }
    }
}

void grid_render_run(const grid* g, const grid_run* run, const uint16_t pal[16], uint16_t* px) {
    uint32_t stride = run->len * GRID_CELL_PX / 2u; /* pairs a pixel row */
    uint32_t i = run->row * GRID_COLS + run->col;
    pixel_pair* out = (pixel_pair*)(void*)px;
    /* The region's cells in the run, [k0, k1), counted from its first cell: none if k0 == k1. */
    uint32_t k0 = 0;
    uint32_t k1 = 0;
    if ((g->region_bitmap || g->region_painter) && run->row >= g->region_row &&
        run->row < (uint32_t)g->region_row + g->region_h) {
        uint32_t c0 = g->region_col;
        uint32_t c1 = c0 + g->region_w;
        uint32_t a = run->col > c0 ? run->col : c0;
        uint32_t e = (uint32_t)run->col + run->len < c1 ? (uint32_t)run->col + run->len : c1;
        if (a < e) {
            k0 = a - run->col;
            k1 = e - run->col;
            render_region(g, run, k0, k1, pal, px);
        }
    }
    for (uint32_t k = 0; k < run->len; k++, i++) {
        if (k >= k0 && k < k1) {
            continue;
        }
        uint32_t piece = (uint32_t)g->ch[i] - GRID_BIG_FIRST;
        if (piece < GRID_BIG_PIECES) {
            render_piece(g, i, piece, pal[g->attr[i] & 15u], pal[g->attr[i] >> 4],
                         px + k * GRID_CELL_PX, run->len * GRID_CELL_PX);
            continue;
        }
        const uint8_t* glyph = font_glyph(g->ch[i]);
        uint32_t fg = pal[g->attr[i] & 15u];
        uint32_t bg = pal[g->attr[i] >> 4];
        /* The cell's colours by a pair's two glyph bits. */
        const uint32_t lut[4] = {PAIR(bg, bg), PAIR(bg, fg), PAIR(fg, bg), PAIR(fg, fg)};
        pixel_pair* o = out + k * (GRID_CELL_PX / 2u);
        if (!g->ovl[i]) {
            for (uint32_t y = 0; y < 8u; y++, o += stride) {
                uint32_t bits = glyph[y];
                o[0] = lut[bits >> 6];
                o[1] = lut[bits >> 4 & 3u];
                o[2] = lut[bits >> 2 & 3u];
                o[3] = lut[bits & 3u];
            }
            continue;
        }
        uint32_t s = g->ovl[i] - 1u;
        const uint8_t* mask = g->ovl_mask[s];
        uint32_t c = pal[g->ovl_color[s] & 15u];
        uint32_t oc = PAIR(c, c);
        /* A row of the glyph's background alone (a scope's cells are mostly blank): the
         * overlay over the background, by a pair's two mask bits. */
        const uint32_t olut[4] = {PAIR(bg, bg), PAIR(bg, c), PAIR(c, bg), oc};
        const glyph_word* gw = (const glyph_word*)(const void*)glyph;
        if (!(gw[0] | gw[1])) { /* a blank glyph (a scope's cells): the overlay alone */
            for (uint32_t y = 0; y < 8u; y++, o += stride) {
                uint32_t over = mask[y];
                o[0] = olut[over >> 6];
                o[1] = olut[over >> 4 & 3u];
                o[2] = olut[over >> 2 & 3u];
                o[3] = olut[over & 3u];
            }
            continue;
        }
        for (uint32_t y = 0; y < 8u; y++, o += stride) {
            uint32_t bits = glyph[y];
            uint32_t over = mask[y];
            if (!bits) {
                o[0] = olut[over >> 6];
                o[1] = olut[over >> 4 & 3u];
                o[2] = olut[over >> 2 & 3u];
                o[3] = olut[over & 3u];
                continue;
            }
            for (uint32_t j = 0; j < 4u; j++) {
                uint32_t sh = 6u - 2u * j;
                uint32_t m = PAIR_MASK[over >> sh & 3u];
                o[j] = (lut[bits >> sh & 3u] & ~m) | (oc & m);
            }
        }
    }
}
