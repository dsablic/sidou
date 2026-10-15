// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui.h"

#include "grid.h"
#include "lcd.h"
#include "views.h"

#ifndef SIDOU_VERSION
#define SIDOU_VERSION "0.0.0"
#endif

/* "V" SIDOU_VERSION ends before column 28 (app/views.c) and must stay inside the 14 cells from
 * column 14 that host/crates/emu-gate/tests/screen.rs (VERSION_CELLS) leaves out of the golden
 * images; longer, its start column would also wrap below zero. */
#define VERSION_CELLS 14u
_Static_assert(sizeof SIDOU_VERSION <= VERSION_CELLS, "SIDOU_VERSION is too long for row 2");

/* A run (one row of up to GRID_COLS cells) is rendered into one LCD buffer. */
_Static_assert(GRID_RUN_PIXELS <= LCD_BUFFER_PIXELS, "a grid run does not fit an LCD buffer");

static grid cells;
static views ui;
/* grid_palette in the panel's byte order. */
static uint16_t palette[16];
static uint32_t last_compose;
/* A run rendered into lcd_buffer(), waiting for the panel. */
static grid_run run;
static uint8_t pending;

/* The next changed run rendered into the free buffer, unless one waits already. 0: nothing to
 * send. */
static int prepare(void) {
    if (!pending && grid_next_run(&cells, &run)) {
        grid_render_run(&cells, &run, palette, lcd_buffer());
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

void ui_init(live* l) {
    for (uint32_t i = 0; i < 16u; i++) {
        palette[i] = LCD_SWAP(grid_palette[i]);
    }
    grid_init(&cells, ' ', VIEWS_TEXT);
    views_init(&ui, &cells, l, "V" SIDOU_VERSION);
    views_frame(&ui, 0);
    flush();
    lcd_on();
}

void ui_poll(uint32_t now_ms) {
    if (now_ms - last_compose >= UI_PERIOD_MS) {
        last_compose = now_ms;
        views_frame(&ui, now_ms);
    }
    if (!pending && !(cells.rows & ~cells.hold)) {
        return; /* nothing to send: most calls, between frames and on an idle screen */
    }
    if (prepare() && !lcd_busy()) {
        send();
        prepare(); /* rendered while this one transfers */
    }
}

void ui_buttons(panel* p, uint32_t pressed, uint32_t held) {
    views_buttons(&ui, p, pressed, held);
}

void ui_encoder(panel* p, uint32_t enc, int32_t steps) {
    views_encoder(&ui, p, enc, steps);
}

void ui_update(uint32_t step, int32_t code) {
    grid_hold(&cells, 0); /* a wipe's held rows go out too */
    views_update(&ui, step, code);
    flush();
}
