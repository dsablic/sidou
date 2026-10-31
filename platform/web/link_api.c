// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stddef.h>
#include <stdint.h>

#include "font.h"
#include "grid.h"

uint8_t* link_font_init(void);
static uint8_t* font;

#define EXPORT(name) __attribute__((export_name(#name)))

#define W (GRID_COLS * GRID_CELL_PX)
#define PAYLOAD_MAX 48u
#define OVERLAY 0x10u
#define BLANK 0x20u

#define HELLO 0x41u
#define COLOURS 0x44u
#define GLYPHS 0x45u
#define CELLS 0x46u
#define PIXELS 0x47u

static grid g;
static uint8_t in_buf[PAYLOAD_MAX];
static uint8_t cell_ch[GRID_CELLS], cell_attr[GRID_CELLS], cell_x[GRID_CELLS];
static uint8_t cell_mask[GRID_CELLS][8];
static uint8_t is_pixel[GRID_CELLS];
static uint8_t pixels[GRID_CELLS][64];
static uint8_t have_hello, font_last, piece_first, pieces;
static uint32_t dirty_rows;
static uint16_t pal565[16];
static uint32_t pal_rgba[16];
static const uint16_t INDEX[16] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15};
static uint16_t run_px[2][GRID_RUN_PIXELS] __attribute__((aligned(4)));
static uint16_t fb565[W * W];
static uint32_t rgba[W * W];

static void set_colour(uint32_t c, uint16_t v) {
    pal565[c] = v;
    uint32_t r = v >> 11, gr = v >> 5 & 63u, b = v & 31u;
    r = r << 3 | r >> 2;
    gr = gr << 2 | gr >> 4;
    b = b << 3 | b >> 2;
    pal_rgba[c] = 0xFF000000u | b << 16 | gr << 8 | r;
}

static void reset(void) {
    for (uint32_t i = 0; i < GRID_CELLS; i++) {
        cell_ch[i] = cell_attr[i] = cell_x[i] = 0;
        is_pixel[i] = 0;
    }
    font_last = FONT_LAST;
    piece_first = GRID_BIG_FIRST;
    pieces = GRID_BIG_PIECES;
    have_hello = 0;
    dirty_rows = (1u << GRID_ROWS) - 1u;
}

static void painter(void* ctx, uint32_t col, uint32_t row, uint8_t px[64]) {
    (void)ctx;
    const uint8_t* s = pixels[row * GRID_COLS + col];
    for (uint32_t k = 0; k < 64u; k++) {
        px[k] = s[k];
    }
}

EXPORT(lv_in) uint8_t* lv_in(void) {
    return in_buf;
}
EXPORT(lv_font) uint8_t* lv_font(void) {
    return font;
}
EXPORT(lv_font_codes) uint32_t lv_font_codes(void) {
    return FONT_LAST + 1u;
}
EXPORT(lv_palette) uint16_t* lv_palette(void) {
    return pal565;
}
EXPORT(lv_reset) void lv_reset(void) {
    reset();
}
EXPORT(lv_touch) void lv_touch(void) {
    for (uint32_t c = 0; c < 16u; c++) {
        set_colour(c, pal565[c]);
    }
    dirty_rows = (1u << GRID_ROWS) - 1u;
}

static int is_piece(uint8_t ch) {
    return (uint8_t)(ch - piece_first) < pieces;
}

static int cells(uint32_t cmd, uint32_t n) {
    const uint8_t* p = in_buf;
    if (n < 5u || p[0] >= GRID_ROWS) {
        return 0;
    }
    uint32_t row = p[0], col = p[1], at = 2;
    dirty_rows |= 1u << row;
    while (at < n) {
        if (col >= GRID_COLS || at + 3u > n) {
            return 0;
        }
        uint32_t i = row * GRID_COLS + col;
        uint8_t ch = p[at], attr = p[at + 1u], x = p[at + 2u];
        at += 3u;
        cell_ch[i] = ch;
        cell_attr[i] = attr;
        cell_x[i] = x;
        for (uint32_t y = 0; y < 8u; y++) {
            cell_mask[i][y] = 0;
        }
        int piece = cmd == CELLS && is_piece(ch);
        if (!piece && (x & OVERLAY)) {
            for (uint32_t y = 0; y < 8u; y++) {
                cell_mask[i][y] = at + y < n ? p[at + y] : 0;
            }
            at += 8u;
        }
        is_pixel[i] = cmd == PIXELS;
        if (cmd == PIXELS) {
            for (uint32_t k = 0; k < 32u; k++) {
                uint8_t b = at + k < n ? p[at + k] : 0;
                pixels[i][2u * k] = b >> 4;
                pixels[i][2u * k + 1u] = b & 15u;
            }
            at += 32u;
            col++;
            break;
        }
        col++;
    }
    return at == n;
}

EXPORT(lv_frame) int lv_frame(uint32_t cmd, uint32_t n) {
    const uint8_t* p = in_buf;
    if (n > PAYLOAD_MAX) {
        return 0;
    }
    switch (cmd) {
    case HELLO:
        if (n < 22u || p[2] != GRID_COLS || p[3] != GRID_ROWS || p[4] != 8u || p[5] != 8u ||
            p[6] != 16u) {
            return 0;
        }
        reset();
        font_last = p[8] < FONT_LAST ? p[8] : FONT_LAST;
        piece_first = p[9];
        pieces = p[10] <= GRID_BIG_PIECES ? p[10] : 0;
        have_hello = 1;
        return 1;
    case COLOURS:
        for (uint32_t c = 0; c < 16u && 2u * c + 1u < n; c++) {
            set_colour(c, (uint16_t)(p[2u * c] | p[2u * c + 1u] << 8));
        }
        dirty_rows = (1u << GRID_ROWS) - 1u;
        return 1;
    case GLYPHS: {
        if (n < 1u) {
            return 0;
        }
        for (uint32_t k = 0; 1u + 8u * k + 8u <= n; k++) {
            uint32_t code = p[0] + k;
            if (code <= FONT_LAST) {
                for (uint32_t y = 0; y < 8u; y++) {
                    font[code * 8u + y] = p[1u + 8u * k + y];
                }
            }
        }
        dirty_rows = (1u << GRID_ROWS) - 1u;
        return 1;
    }
    case CELLS:
    case PIXELS:
        return have_hello && cells(cmd, n);
    default:
        return 0;
    }
}

static void put_row(uint32_t r) {
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint32_t i = r * GRID_COLS + c;
        uint8_t ch = cell_ch[i], attr = cell_attr[i], x = cell_x[i];
        int overlay = (x & OVERLAY) != 0;
        if (!is_pixel[i] && is_piece(ch)) {
            ch = (uint8_t)(GRID_BIG_FIRST + (uint8_t)(ch - piece_first));
            g.src[i] = x;
            if (x > font_last) {
                attr = (uint8_t)(attr & 0xF0u) | (attr >> 4);
            }
            overlay = 0;
        } else if (ch > font_last) {
            ch = BLANK;
            if (!is_pixel[i]) {
                attr = (uint8_t)(attr & 0xF0u) | (attr >> 4);
            }
        } else if (!is_pixel[i] && (uint8_t)(ch - GRID_BIG_FIRST) < GRID_BIG_PIECES) {
            g.src[i] = ch;
            ch = GRID_PIECE_PLAIN;
        }
        g.ch[i] = ch;
        g.attr[i] = attr;
        if (overlay) {
            grid_overlay(&g, c, r, cell_mask[i], x & 15u);
        }
    }
}

EXPORT(lv_render) uint32_t* lv_render(void) {
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (!(dirty_rows >> r & 1u)) {
            continue;
        }
        put_row(r);
        grid_run run = {(uint8_t)r, 0, GRID_COLS};
        g.region_painter = NULL;
        g.region_bitmap = NULL;
        grid_render_run(&g, &run, INDEX, run_px[0]);
        int any = 0;
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            any |= is_pixel[r * GRID_COLS + c];
        }
        if (any) {
            g.region_painter = painter;
            g.region_col = 0;
            g.region_row = 0;
            g.region_w = GRID_COLS;
            g.region_h = GRID_ROWS;
            g.region_run = GRID_COLS;
            grid_render_run(&g, &run, INDEX, run_px[1]);
            g.region_painter = NULL;
        }
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            grid_overlay_clear(&g, c, r);
            const uint16_t* src = run_px[is_pixel[r * GRID_COLS + c]];
            for (uint32_t y = 0; y < 8u; y++) {
                uint32_t o = (r * 8u + y) * W + c * 8u;
                for (uint32_t x = 0; x < 8u; x++) {
                    uint32_t k = src[y * W + c * 8u + x] & 15u;
                    fb565[o + x] = pal565[k];
                    rgba[o + x] = pal_rgba[k];
                }
            }
        }
    }
    dirty_rows = 0;
    return rgba;
}

EXPORT(lv_fb565) uint16_t* lv_fb565(void) {
    return fb565;
}

EXPORT(lv_init) void lv_init(void) {
    font = link_font_init();
    grid_init(&g, BLANK, 0);
    for (uint32_t c = 0; c < 16u; c++) {
        set_colour(c, grid_palette[c]);
    }
    reset();
}
