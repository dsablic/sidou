// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "logo.h"

#include <stddef.h>

#include "font.h"
#include "screen.h"
#include "sine.h"

#define PT_COLS 15u
#define PT_ROWS 5u
#define PT_BANDS 7u
#define PT_LETTERS 5u
#define PT_PITCH_Q 6u
#define PT_PERIOD 24u
#define PT_HIGH 31u
#define PT_LOW 36u
#define PT_COLOR GRID_CYAN
#define PT_STEP 4u
#define PT_MOVE 4u
#define PT_SWEEP 14u

static const uint8_t PT_FACE[PT_LETTERS][PT_BANDS] = {
    {0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E},
    {0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F},
    {0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E},
    {0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E},
    {0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E},
};

static const uint8_t PT_SLOT[4] = {' ', FONT_SLOT_L, FONT_SLOT_R, FONT_SLOT_LR};

static uint32_t pt_quads(uint32_t cx, uint32_t b) {
    uint32_t out = 0;
    for (uint32_t q = 0; q < 2u; q++) {
        uint32_t qx = cx * 2u + q;
        uint32_t letter = qx / PT_PITCH_Q;
        uint32_t x = qx % PT_PITCH_Q;
        if (letter < PT_LETTERS && x < 5u && (PT_FACE[letter][b] >> (4u - x) & 1u)) {
            out |= 1u << q;
        }
    }
    return out;
}

#define PT_W (PT_COLS * 8u)
static void pt_wave(uint8_t col[PT_W], uint32_t shift, uint32_t duty) {
    uint8_t high[PT_W];
    uint32_t p = shift % PT_PERIOD;
    for (uint32_t x = 0; x < PT_W; x++) {
        high[x] = p < duty;
        if (++p == PT_PERIOD) {
            p = 0;
        }
    }
    const uint8_t edge = 0x7Fu;
    for (uint32_t x = 0; x < PT_W; x++) {
        col[x] = high[x] ? 0x03u : (uint8_t)(0x03u << (PT_LOW - PT_HIGH));
        if ((x > 0 && high[x - 1] != high[x]) || (x + 1 < PT_W && high[x + 1] != high[x])) {
            col[x] |= edge;
        }
    }
}

static void pt_draw(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    grid_region_clear(g);
    uint32_t stripe = (frame + 2u) / PT_STEP % PT_BANDS;
    uint32_t pulse = frame / PT_STEP;
    uint32_t shift = pulse * PT_MOVE % PT_PERIOD;
    uint32_t duty = (uint32_t)(12 + sine_scaled(pulse * PT_SWEEP, 7));
    const uint8_t blank = GRID_ATTR(bg, bg);
    uint8_t wave[PT_W];
    pt_wave(wave, shift, duty);
    for (uint32_t cy = 0; cy < PT_ROWS; cy++) {
        for (uint32_t cx = 0; cx < PT_COLS; cx++) {
            uint32_t c = col + cx;
            uint32_t r = row + cy;
            uint8_t mask[8] = {0};
            uint32_t over = 0;
            uint8_t over_color = PT_COLOR;
            uint32_t top = 2u * cy;
            if (top < PT_BANDS) {
                uint32_t q = pt_quads(cx, top);
                uint8_t fg = screen_ramp[(top + PT_BANDS - stripe) % PT_BANDS];
                grid_put(g, c, r, PT_SLOT[q], q ? GRID_ATTR(fg, bg) : blank);
            } else {
                grid_put(g, c, r, ' ', blank);
            }
            if (top + 1u < PT_BANDS) {
                uint32_t q = pt_quads(cx, top + 1u);
                uint8_t bits = (uint8_t)((q & 1u ? 0xF0u : 0u) | (q & 2u ? 0x0Fu : 0u));
                mask[4] = mask[5] = mask[6] = bits;
                over = bits;
                over_color = screen_ramp[(top + 1u + PT_BANDS - stripe) % PT_BANDS];
            } else {
                for (uint32_t x = 0; x < 8u; x++) {
                    uint32_t rows = wave[cx * 8u + x];
                    for (uint32_t y = 0; y < 8u; y++) {
                        uint32_t py = cy * 8u + y;
                        if (py >= PT_HIGH && py < PT_HIGH + 8u && (rows >> (py - PT_HIGH) & 1u)) {
                            mask[y] |= (uint8_t)(0x80u >> x);
                        }
                    }
                }
                for (uint32_t y = 0; y < 8u; y++) {
                    over |= mask[y];
                }
            }
            if (over) {
                grid_overlay(g, c, r, mask, over_color);
            } else {
                grid_overlay_clear(g, c, r);
            }
        }
    }
}

const logo logo_pulse_train = {"PULSE TRAIN", PT_COLS, PT_ROWS, 55, 42, pt_draw, NULL};

#define CC_BAND_ROWS 8u
#define MC_BAND_ROWS 9u

static void cc_blank(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t bg) {
    grid_fill(g, col, row, w, h, ' ', GRID_ATTR(GRID_WHITE, bg));
    if (g->ovl_nfree == GRID_OVERLAYS) {
        return;
    }
    uint32_t c1 = col + w < GRID_COLS ? col + w : GRID_COLS;
    uint32_t r1 = row + h < GRID_ROWS ? row + h : GRID_ROWS;
    for (uint32_t r = row; r < r1; r++) {
        for (uint32_t c = col; c < c1; c++) {
            if (g->ovl[r * GRID_COLS + c]) {
                grid_overlay_clear(g, c, r);
            }
        }
    }
}

static void cc_draw(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    (void)frame;
    cc_blank(g, col, row, GRID_COLS, CC_BAND_ROWS, bg);
    grid_region(g, col, row, GRID_COLS, CC_BAND_ROWS, &logo_cracktro_chrome_bitmap, 0,
                LOGO_CHROME_BAND_ROW);
}

static void cc_scene(grid* g, uint32_t frame) {
    (void)frame;
    cc_blank(g, 0, 0, GRID_COLS, GRID_ROWS, GRID_BLACK);
    grid_region(g, 0, 0, GRID_COLS, GRID_ROWS, &logo_cracktro_chrome_bitmap, 0, 0);
}

static void mc_draw(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    (void)frame;
    cc_blank(g, col, row, GRID_COLS, MC_BAND_ROWS, bg);
    grid_region(g, col, row, GRID_COLS, MC_BAND_ROWS, &logo_metal_chrome_bitmap, 0,
                LOGO_METAL_BAND_ROW);
}

static void mc_scene(grid* g, uint32_t frame) {
    (void)frame;
    cc_blank(g, 0, 0, GRID_COLS, GRID_ROWS, GRID_BLACK);
    grid_region(g, 0, 0, GRID_COLS, GRID_ROWS, &logo_metal_chrome_bitmap, 0, 0);
}

const logo logo_cracktro_chrome = {"CRACKTRO CHROME", GRID_COLS, CC_BAND_ROWS, 0, 0, cc_draw,
                                   cc_scene};
const logo logo_metal_chrome = {"METAL CHROME", GRID_COLS, MC_BAND_ROWS, 0, 0, mc_draw,
                                mc_scene};

const logo* const logo_default = &logo_metal_chrome;

void logo_draw(const logo* l, grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    l->draw(g, col, row, frame, bg);
}

uint32_t logo_center_col(const logo* l) {
    return (GRID_COLS - l->cols) / 2u;
}
