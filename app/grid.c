// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "grid.h"

#include <stddef.h>

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

/* The region's cells, to be sent again. */
static void region_stale(grid* g) {
    for (uint32_t r = g->region_row; r < (uint32_t)g->region_row + g->region_h; r++) {
        for (uint32_t c = g->region_col; c < (uint32_t)g->region_col + g->region_w; c++) {
            mark_stale(g, r * GRID_COLS + c);
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
    if (g->region_bitmap) {
        region_stale(g);
    }
    g->region_bitmap = b;
    g->region_col = (uint8_t)col;
    g->region_row = (uint8_t)row;
    g->region_w = (uint8_t)w;
    g->region_h = (uint8_t)h;
    g->region_bx = (uint8_t)bx;
    g->region_by = (uint8_t)by;
    region_stale(g);
    return 0;
}

void grid_region_clear(grid* g) {
    if (g->region_bitmap) {
        region_stale(g);
        g->region_bitmap = NULL;
    }
}

/* The run that holds pixel x of row y, and how many pixels of it are left from x on. */
static const uint8_t* bitmap_seek(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t* left) {
    const uint8_t* p = b->data + b->rows[y];
    uint32_t end = (uint32_t)(*p >> 4) + 1u; /* the pixel after the run */
    while (x >= end) {
        end += (uint32_t)(*++p >> 4) + 1u;
    }
    *left = end - x;
    return p;
}

void grid_bitmap_row(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    if (!n) {
        return;
    }
    uint32_t left;
    const uint8_t* p = bitmap_seek(b, x, y, &left);
    for (;;) {
        uint8_t c = *p & 15u;
        uint32_t k = left < n ? left : n;
        n -= k;
        while (k--) {
            *out++ = c;
        }
        if (!n) {
            return;
        }
        left = (uint32_t)(*++p >> 4) + 1u;
    }
}

/* As grid_bitmap_row, as the panel's pixels. */
static void bitmap_pixels(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n,
                          const uint16_t pal[16], uint16_t* out) {
    uint32_t left;
    const uint8_t* p = bitmap_seek(b, x, y, &left);
    for (;;) {
        uint16_t c = pal[*p & 15u];
        uint32_t k = left < n ? left : n;
        n -= k;
        while (k--) {
            *out++ = c;
        }
        if (!n) {
            return;
        }
        left = (uint32_t)(*++p >> 4) + 1u;
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
        while (c < GRID_COLS && !dirty(g, base + c)) {
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
        for (; c < GRID_COLS && dirty(g, base + c); c++) {
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
#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define PAIR(left, right) ((uint32_t)(left) << 16 | (uint32_t)(right))
#else
#define PAIR(left, right) ((uint32_t)(right) << 16 | (uint32_t)(left))
#endif

/* A pair's two bits (the left one the higher) as which of its pixels are set. */
static const uint32_t PAIR_MASK[4] = {PAIR(0u, 0u), PAIR(0u, 0xFFFFu), PAIR(0xFFFFu, 0u),
                                      PAIR(0xFFFFu, 0xFFFFu)};

/* The region's cells [k0, k1) of a run (k counted from the run's first cell): the bitmap's
 * pixels, then the glyphs' foreground and the overlays over them. */
static void render_region(const grid* g, const grid_run* run, uint32_t k0, uint32_t k1,
                          const uint16_t pal[16], uint16_t* px) {
    const grid_bitmap* b = g->region_bitmap;
    uint32_t w = run->len * GRID_CELL_PX; /* pixels a row */
    uint32_t x = (g->region_bx + run->col + k0 - g->region_col) * GRID_CELL_PX;
    uint32_t y = (g->region_by + run->row - g->region_row) * GRID_CELL_PX;
    for (uint32_t r = 0; r < 8u; r++) {
        bitmap_pixels(b, x, y + r, (k1 - k0) * GRID_CELL_PX, pal, px + r * w + k0 * GRID_CELL_PX);
    }
    uint32_t i = run->row * GRID_COLS + run->col + k0;
    for (uint32_t k = k0; k < k1; k++, i++) {
        const uint8_t* glyph = font_glyphs[g->ch[i]];
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

void grid_render_run(const grid* g, const grid_run* run, const uint16_t pal[16], uint16_t* px) {
    uint32_t stride = run->len * GRID_CELL_PX / 2u; /* pairs a pixel row */
    uint32_t i = run->row * GRID_COLS + run->col;
    pixel_pair* out = (pixel_pair*)(void*)px;
    /* The region's cells in the run, [k0, k1), counted from its first cell: none if k0 == k1. */
    uint32_t k0 = 0;
    uint32_t k1 = 0;
    if (g->region_bitmap && run->row >= g->region_row &&
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
        const uint8_t* glyph = font_glyphs[g->ch[i]];
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
