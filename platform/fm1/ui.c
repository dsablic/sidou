// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui.h"

#include "lcd.h"

/* font/font8x8_basic.h (public domain, Daniel Hepper): the glyphs of U+0000..U+007F, one
 * byte per row, bit 0 the leftmost pixel. Vendored unchanged; it defines the array, so only
 * this file includes it. */
#include "font/font8x8_basic.h"

#ifndef SIDOU_VERSION
#define SIDOU_VERSION "0.0.0"
#endif

#define COLS (LCD_W / 8u)
#define ROWS (LCD_H / 8u)
#define ROWS_PER_STRIP (LCD_STRIP_ROWS / 8u)

/* Cell codes past ASCII: a bar cell filled n of 8 pixel columns (BAR0 + n), and the border. */
#define BAR0 0x80u
#define BORDER 0x89u

#define BAR_COL 14u
#define BAR_CELLS 14u

static uint8_t want[ROWS][COLS];  /* the screen as it should be */
static uint8_t shown[ROWS][COLS]; /* the screen as the panel has it */
static uint32_t last_compose;
static uint32_t dirty; /* bit s: strip s differs from what the panel has */

static const char* const NOTE_NAMES[12] = {"C",  "C#", "D",  "D#", "E",  "F",
                                           "F#", "G",  "G#", "A",  "A#", "B"};

static void glyph(uint8_t c, uint8_t rows[8]) {
    for (uint32_t y = 0; y < 8u; y++) {
        if (c < 0x80u) {
            rows[y] = (uint8_t)font8x8_basic[c][y];
        } else if (c == BORDER) {
            rows[y] = 0xFFu;
        } else if (c <= BAR0 + 8u) {
            /* rows 1..6, the leftmost n columns */
            rows[y] = y >= 1u && y <= 6u ? (uint8_t)((1u << (c - BAR0)) - 1u) : 0u;
        } else {
            rows[y] = 0;
        }
    }
}

static void put(uint32_t row, uint32_t col, const char* s) {
    while (*s && col < COLS - 1u) {
        want[row][col++] = (uint8_t)*s++;
    }
}

/* value right-aligned in width cells. */
static void put_num(uint32_t row, uint32_t col, uint32_t width, int32_t value, int sign) {
    char buf[12];
    uint32_t n = 0;
    uint32_t v = (uint32_t)(value < 0 ? -value : value);
    do {
        buf[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v && n < 10u);
    if (sign) {
        buf[n++] = value < 0 ? '-' : '+';
    }
    for (uint32_t i = 0; i < width; i++) {
        want[row][col + width - 1u - i] = i < n ? (uint8_t)buf[i] : (uint8_t)' ';
    }
}

static void put_bar(uint32_t row, uint32_t value, uint32_t max) {
    uint32_t px = value * (BAR_CELLS * 8u) / max;
    for (uint32_t i = 0; i < BAR_CELLS; i++) {
        uint32_t fill = px >= 8u ? 8u : px;
        want[row][BAR_COL + i] = (uint8_t)(BAR0 + fill);
        px -= fill;
    }
}

static void put_param(uint32_t row, const char* label, uint32_t value, uint32_t max) {
    put(row, 2, label);
    put_num(row, 9, 4, (int32_t)value, 0);
    put_bar(row, value, max);
}

static void compose(const live* l) {
    for (uint32_t r = 0; r < ROWS; r++) {
        for (uint32_t c = 0; c < COLS; c++) {
            int edge = r == 0u || r == ROWS - 1u || c == 0u || c == COLS - 1u;
            want[r][c] = edge ? (uint8_t)BORDER : (uint8_t)' ';
        }
    }
    put(2, 2, "SIDOU");
    put(2, 28u - (sizeof SIDOU_VERSION), "V" SIDOU_VERSION);

    put(5, 2, "CHIP");
    put(5, 9, l->model == SID_MODEL_8580 ? "8580" : "6581");
    put(6, 2, "PRESET");
    const live_preset* p = live_preset_get(l->preset);
    put(6, 9, p ? p->name : "?");
    put(7, 2, "OCTAVE");
    put_num(7, 9, 2, l->octave, 1);

    put_param(10, "CUTOFF", l->cutoff, LIVE_CUTOFF_MAX);
    put_param(11, "RESO", l->resonance, LIVE_RESONANCE_MAX);
    put_param(12, "PULSE", l->pulse_width, LIVE_PULSE_WIDTH_MAX);
    put_param(13, "VOLUME", l->volume, 15);

    put(16, 2, "NOTES");
    uint32_t col = 9;
    for (int v = 0; v < LIVE_VOICES; v++) {
        uint8_t n = l->voice[v].note;
        if (!l->voice[v].gate || n == LIVE_NO_NOTE) {
            continue;
        }
        const char* name = NOTE_NAMES[n % 12u];
        put(16, col, name);
        uint32_t oc = col + (name[1] ? 2u : 1u);
        if (n < 12u) {
            put(16, oc, "-1");
        } else {
            put_num(16, oc, 1, n / 12 - 1, 0);
        }
        col += 5u;
    }

    put(26, 2, "READY.");
}

static void draw_strip(uint32_t s) {
    uint16_t* px = lcd_strip_buffer();
    for (uint32_t cr = 0; cr < ROWS_PER_STRIP; cr++) {
        uint32_t row = s * ROWS_PER_STRIP + cr;
        for (uint32_t c = 0; c < COLS; c++) {
            uint8_t g[8];
            glyph(want[row][c], g);
            for (uint32_t y = 0; y < 8u; y++) {
                uint16_t* o = px + (cr * 8u + y) * LCD_W + c * 8u;
                uint32_t bits = g[y];
                for (uint32_t x = 0; x < 8u; x++, bits >>= 1) {
                    o[x] = (bits & 1u) ? UI_FOREGROUND : UI_BACKGROUND;
                }
            }
            shown[row][c] = want[row][c];
        }
    }
    lcd_strip_send(s);
}

static int strip_dirty(uint32_t s) {
    for (uint32_t r = s * ROWS_PER_STRIP; r < (s + 1u) * ROWS_PER_STRIP; r++) {
        for (uint32_t c = 0; c < COLS; c++) {
            if (want[r][c] != shown[r][c]) {
                return 1;
            }
        }
    }
    return 0;
}

/* After compose: the strips whose cells changed. */
static void mark_dirty(void) {
    for (uint32_t s = 0; s < LCD_STRIPS; s++) {
        if (strip_dirty(s)) {
            dirty |= 1u << s;
        }
    }
}

void ui_init(const live* l) {
    compose(l);
    for (uint32_t s = 0; s < LCD_STRIPS; s++) {
        draw_strip(s);
    }
    lcd_sync();
    lcd_on();
}

void ui_poll(const live* l, uint32_t now_ms) {
    if (now_ms - last_compose >= UI_PERIOD_MS) {
        last_compose = now_ms;
        compose(l);
        mark_dirty();
    }
    if (!dirty || lcd_busy()) {
        return;
    }
    for (uint32_t s = 0; s < LCD_STRIPS; s++) {
        if (dirty & (1u << s)) {
            dirty &= ~(1u << s);
            draw_strip(s);
            return;
        }
    }
}

void ui_update(uint32_t step, int32_t code) {
    static const char* const STEP[6] = {"", "PACKAGE", "CHECK HEAD", "LOADER", "CONFIRM",
                                        "RESTART"};
    for (uint32_t r = 19; r <= 21u; r++) {
        put(r, 1, "                            ");
    }
    put(19, 2, "UPDATE");
    if (step < 9u) {
        put(20, 2, STEP[step < 6u ? step : 0u]);
    } else if (code == 1) {
        put(20, 2, "DRY RUN OK");
    } else {
        put(20, 2, "FAILED");
        put_num(20, 9, 3, code, 1);
    }
    put(26, 2, "      ");
    mark_dirty();
    for (uint32_t s = 0; s < LCD_STRIPS; s++) {
        if (dirty & (1u << s)) {
            dirty &= ~(1u << s);
            draw_strip(s);
        }
    }
    lcd_sync();
}
