// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui.h"

#include "font.h"
#include "grid.h"
#include "lcd.h"

#ifndef SIDOU_VERSION
#define SIDOU_VERSION "0.0.0"
#endif

#define TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
#define BAR_COL 14u
#define BAR_CELLS 14u

volatile uint8_t ui_screen;

static grid screen;
static uint8_t composed = UI_STATUS; /* the screen in the grid */
/* grid_palette in the panel's byte order. */
static uint16_t palette[16];
static uint32_t last_compose;
/* A run rendered into lcd_buffer(), waiting for the panel. */
static grid_run run;
static uint8_t pending;

/* The effect slots (core/fx/fx_slots.h), as the screen names them (at most 9 cells). */
static const char* const SLOT_NAMES[LIVE_SLOTS] = {"DIST", "MOD", "FX"};
static const char* const DIST_NAMES[] = {"OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LO-FI"};
static const char* const MOD_NAMES[] = {"OFF",     "CHORUS",   "FLANGER", "PHASER",
                                        "TREMOLO", "AUTO-PAN", "WOW"};
static const char* const FX_NAMES[] = {"OFF", "DELAY", "PING-PONG", "REVERB", "LUSH"};

static const char* algorithm_name(uint32_t slot, uint32_t alg) {
    static const char* const* const NAMES[LIVE_SLOTS] = {DIST_NAMES, MOD_NAMES, FX_NAMES};
    return alg < live_slot_algorithms(slot) ? NAMES[slot][alg] : "?";
}

static const char* const NOTE_NAMES[12] = {"C",  "C#", "D",  "D#", "E",  "F",
                                           "F#", "G",  "G#", "A",  "A#", "B"};

/* Text inside the border. */
static void put(uint32_t row, uint32_t col, const char* s) {
    while (*s && col < GRID_COLS - 1u) {
        grid_put(&screen, col++, row, (uint8_t)*s++, TEXT);
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
        grid_put(&screen, col + width - 1u - i, row, i < n ? (uint8_t)buf[i] : (uint8_t)' ',
                 TEXT);
    }
}

static void put_bar(uint32_t row, uint32_t value, uint32_t max) {
    uint32_t px = value * (BAR_CELLS * 8u) / max;
    for (uint32_t i = 0; i < BAR_CELLS; i++) {
        uint32_t fill = px >= 8u ? 8u : px;
        grid_put(&screen, BAR_COL + i, row, (uint8_t)(FONT_BAR0 + fill), TEXT);
        px -= fill;
    }
}

static void put_param(uint32_t row, const char* label, uint32_t value, uint32_t max) {
    put(row, 2, label);
    put_num(row, 9, 4, (int32_t)value, 0);
    put_bar(row, value, max);
}

static void compose_status(const live* l) {
    const uint32_t last = GRID_COLS - 1u;
    grid_fill(&screen, 0, 0, GRID_COLS, 1, FONT_FULL, TEXT);
    grid_fill(&screen, 0, last, GRID_COLS, 1, FONT_FULL, TEXT);
    grid_fill(&screen, 0, 1, 1, last - 1u, FONT_FULL, TEXT);
    grid_fill(&screen, last, 1, 1, last - 1u, FONT_FULL, TEXT);
    grid_fill(&screen, 1, 1, last - 1u, last - 1u, ' ', TEXT);
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

    /* The effect slots: '>' marks the one EDIT + K1-K4 and ENV act on (SCL moves it). */
    put(18, 2, "EFFECTS");
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint32_t row = 19u + k;
        if (k == l->fx_select) {
            put(row, 2, ">");
        }
        put(row, 3, SLOT_NAMES[k]);
        put(row, 8, algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]));
        put(row, 19, "MIX");
        put_num(row, 23, 3, l->slot[k][LIVE_SLOT_MIX], 0);
    }
    const uint8_t* sel = l->slot[l->fx_select];
    put(22, 3, "AMT");
    put_num(22, 7, 3, sel[LIVE_SLOT_AMOUNT], 0);
    put(22, 12, "A");
    put_num(22, 14, 3, sel[LIVE_SLOT_A], 0);
    put(22, 19, "B");
    put_num(22, 21, 3, sel[LIVE_SLOT_B], 0);

    put(26, 2, "READY.");
}

/* The 256 glyphs in a 16 x 16 block with hex labels, the 16 colours, and a triangle wave
 * drawn as pixel overlays over a row of line glyphs. On black, so that switching between it
 * and the status screen changes every cell. */
static void compose_charset(void) {
    static const char HEX[] = "0123456789ABCDEF";
    const uint8_t frame = GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK);
    const uint8_t label = GRID_ATTR(GRID_YELLOW, GRID_BLACK);
    grid_fill(&screen, 0, 0, GRID_COLS, GRID_ROWS, ' ', frame);
    grid_box(&screen, 0, 0, GRID_COLS, GRID_ROWS, frame, 1);
    grid_text(&screen, 2, 0, " CHARSET ", GRID_ATTR(GRID_WHITE, GRID_BLACK));
    for (uint32_t i = 0; i < 16u; i++) {
        grid_put(&screen, 7u + i, 2, (uint8_t)HEX[i], label);
        grid_put(&screen, 4, 4u + i, (uint8_t)HEX[i], label);
        for (uint32_t c = 0; c < 16u; c++) {
            grid_put(&screen, 7u + c, 4u + i, (uint8_t)(i * 16u + c),
                     GRID_ATTR(GRID_WHITE, GRID_BLACK));
        }
        grid_put(&screen, 7u + i, 22, FONT_FULL, GRID_ATTR(i, GRID_BLACK));
        grid_put(&screen, 7u + i, 23, FONT_D8, GRID_ATTR(i, GRID_BLACK));
        grid_put(&screen, 7u + i, 26, FONT_BOX_H, GRID_ATTR(GRID_DARK_GREY, GRID_BLACK));
        /* 4 pixels per cell rise or fall: a 32-pixel period, rows 1..6 */
        uint8_t mask[8] = {0};
        for (uint32_t x = 0; x < 8u; x++) {
            uint32_t p = (i * 8u + x) % 32u;
            uint32_t y = p < 16u ? 1u + p * 5u / 15u : 6u - (p - 16u) * 5u / 15u;
            mask[y] |= (uint8_t)(0x80u >> x);
        }
        grid_overlay(&screen, 7u + i, 26, mask, GRID_LIGHT_GREEN);
    }
    grid_text(&screen, 2, 22, "COL", label);
    grid_text(&screen, 2, 26, "OVL", label);
}

static void compose(const live* l) {
    uint8_t want = ui_screen;
    if (want != composed) {
        for (uint32_t r = 0; r < GRID_ROWS; r++) {
            for (uint32_t c = 0; c < GRID_COLS; c++) {
                grid_overlay_clear(&screen, c, r);
            }
        }
        composed = want;
    }
    if (want == UI_CHARSET) {
        compose_charset();
    } else {
        compose_status(l);
    }
}

/* The next changed run rendered into the free buffer, unless one waits already. 0: nothing to
 * send. */
static int prepare(void) {
    if (!pending && grid_next_run(&screen, &run)) {
        grid_render_run(&screen, &run, palette, lcd_buffer());
        pending = 1;
    }
    return pending;
}

static void send(void) {
    lcd_send(run.col * GRID_CELL_PX, run.row * GRID_CELL_PX, run.len * GRID_CELL_PX,
             GRID_CELL_PX);
    pending = 0;
}

/* Everything that changed, waiting for the panel (blocking). */
static void flush(void) {
    while (prepare()) {
        send();
    }
    lcd_sync();
}

void ui_init(const live* l) {
    for (uint32_t i = 0; i < 16u; i++) {
        palette[i] = LCD_SWAP(grid_palette[i]);
    }
    grid_init(&screen, ' ', TEXT);
    compose(l);
    flush();
    lcd_on();
}

void ui_poll(const live* l, uint32_t now_ms) {
    if (now_ms - last_compose >= UI_PERIOD_MS) {
        last_compose = now_ms;
        compose(l);
    }
    if (prepare() && !lcd_busy()) {
        send();
        prepare(); /* rendered while this one transfers */
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
    flush();
}
