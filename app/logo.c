// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "logo.h"

#include "font.h"
#include "screen.h"
#include "sine.h"

/* ---- Pulse Train ----
 *
 * 15 x 5 cells, 120 x 40 pixels. The letters are a 5 x 7 face at 4x: each letter pixel a
 * 4 x 4 block, so the blocks are the cells' quadrants. Letter k starts at pixel 24 k. Pixel
 * rows 0..27 are seven bands of 4 rows, each band 3 rows of colour and a slot, coloured
 * through the raster ramp. A cell row holds two bands: the top one as a font glyph
 * (FONT_SLOT_*) in the band's colour, the bottom one as an overlay in the next band's. The
 * pulse wave below them is an overlay too: 2 pixels thick on rows 31-32 (high) or 36-37
 * (low), with 2-pixel edges between, in cyan. */

#define PT_COLS 15u
#define PT_ROWS 5u
#define PT_BANDS 7u
#define PT_LETTERS 5u
#define PT_PITCH_Q 6u /* quadrants from one letter to the next (24 pixels) */
#define PT_PERIOD 24u /* the pulse wave's period, pixels */
#define PT_HIGH 31u   /* its top row high, and low */
#define PT_LOW 36u
#define PT_COLOR GRID_CYAN
/* Every 4 frames: the stripes on frames 2 mod 4, the pulse on 0 mod 4. */
#define PT_STEP 4u
/* The pulse moves this many pixels a step (25 pixels a second), and its duty cycle sweeps
 * 14/256 of a turn a step (2.2 rad/s). */
#define PT_MOVE 4u
#define PT_SWEEP 14u

/* S I D O U, rows top first, bit 4 the leftmost pixel. */
static const uint8_t PT_FACE[PT_LETTERS][PT_BANDS] = {
    {0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E}, /* S */
    {0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F}, /* I */
    {0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E}, /* D */
    {0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E}, /* O */
    {0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E}, /* U */
};

static const uint8_t PT_SLOT[4] = {' ', FONT_SLOT_L, FONT_SLOT_R, FONT_SLOT_LR};

/* Band b's two quadrants in cell column cx: bit 0 the left, bit 1 the right. */
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

/* The pulse wave's pixels in each of its pixel columns, as a mask of its rows 31..38 (bit 0
 * row 31): the level's two rows, and rows 31..37 where the level changes (the column of the
 * change and the one before it). */
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
    const uint8_t edge = 0x7Fu; /* rows 31..37 */
    for (uint32_t x = 0; x < PT_W; x++) {
        col[x] = high[x] ? 0x03u : (uint8_t)(0x03u << (PT_LOW - PT_HIGH));
        if ((x > 0 && high[x - 1] != high[x]) || (x + 1 < PT_W && high[x + 1] != high[x])) {
            col[x] |= edge;
        }
    }
}

static void pt_draw(grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    uint32_t stripe = (frame + 2u) / PT_STEP % PT_BANDS;
    uint32_t pulse = frame / PT_STEP;
    uint32_t shift = pulse * PT_MOVE % PT_PERIOD;
    /* duty 12 +- 7 of 24 pixels: 0.5 +- 0.3 */
    uint32_t duty = (uint32_t)(12 + sine_scaled(pulse * PT_SWEEP, 7));
    const uint8_t blank = GRID_ATTR(bg, bg);
    uint8_t wave[PT_W];
    pt_wave(wave, shift, duty);
    for (uint32_t cy = 0; cy < PT_ROWS; cy++) {
        for (uint32_t cx = 0; cx < PT_COLS; cx++) {
            uint32_t c = col + cx;
            uint32_t r = row + cy;
            uint8_t mask[8] = {0};
            uint32_t over = 0; /* any overlay pixel */
            uint8_t over_color = PT_COLOR;
            uint32_t top = 2u * cy; /* the band in the cell's top half */
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
                /* The cell's pixel rows against the wave's rows 31..38. */
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

/* Measured by host/crates/sidou-tests/tests/app/logo.rs: at most 42 cells on a stripe frame
 * and 30 on a pulse frame; 55 overlays. */
const logo logo_pulse_train = {"PULSE TRAIN", PT_COLS, PT_ROWS, 55, 42, pt_draw};

const logo* const logo_default = &logo_pulse_train;

void logo_draw(const logo* l, grid* g, uint32_t col, uint32_t row, uint32_t frame, uint8_t bg) {
    l->draw(g, col, row, frame, bg);
}

uint32_t logo_center_col(const logo* l) {
    return (GRID_COLS - l->cols) / 2u;
}
