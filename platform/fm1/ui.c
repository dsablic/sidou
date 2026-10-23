// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ui.h"

#include "audio.h"
#include "grid.h"
#include "lcd.h"
#include "midi.h"
#include "settings.h"
#include "store_fm1.h"
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

/* In POOL (app.ld): the grid is big and the RAM region the tighter of the two. */
static grid cells __attribute__((section(".pool")));
static views ui;
/* What persists (app/settings.h): on the device's store, ticked in the views' frame. */
static settings persisted;
static uint8_t settings_forced, song_forced;

static void settings_force(int on) {
    settings_forced = (uint8_t)(on != 0);
    store_fm1_force(settings_forced || song_forced || persisted.forcing ||
                    ui.users.forcing || ui.formatting);
}

void ui_song_force(int on) {
    song_forced = (uint8_t)(on != 0);
    store_fm1_force(settings_forced || song_forced || persisted.forcing ||
                    ui.users.forcing || ui.formatting);
}
/* grid_palette in the panel's byte order. */
static uint16_t palette[16];
static uint32_t last_compose;
/* When the next frame or half-frame preparation is due. */
static uint32_t next_at;
/* A run rendered into lcd_buffer(), waiting for the panel. */
static grid_run run;
static uint8_t pending;
/* The display is on: the first frame has reached the panel whole. */
static uint8_t shown;
/* The view may have work between frames (views_idle): from a frame that leaves some (SCENE's
 * idle, RED MOON RITE's scan) until views_idle says it has none. */
static uint8_t idle_work;
/* A call with nothing to send has nothing to do either: the display is on and the view has no
 * work between frames (the one test such calls make, most calls). */
static uint8_t at_rest;

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

void ui_set_keys(keys_route* kr) {
    views_set_keys(&ui, kr);
}

void ui_set_panel(panel* p) {
    views_set_panel(&ui, p);
}

void ui_set_song(const song_transport* t) {
    views_set_song(&ui, t);
}

void ui_song_loaded(int back) {
    if (back) {
        views_song_back(&ui);
    } else {
        views_song_loaded(&ui);
    }
}

void ui_set_song_io(song_io* io) {
    views_song_io(&ui, io);
}

void ui_song_rows(uint8_t rows_per_beat) {
    ui.song.rows_beat = rows_per_beat;
    ui.song.built = 0;
}

settings* ui_settings(void) {
    return &persisted;
}

void ui_init(live* l, ui_sound* us, uint32_t now_ms, int home_held) {
    for (uint32_t i = 0; i < 16u; i++) {
        palette[i] = LCD_SWAP(grid_palette[i]);
    }
    grid_init(&cells, ' ', VIEWS_TEXT);
    views_init(&ui, &cells, l, audio_scope(), "V" SIDOU_VERSION);
    views_set_sounds(&ui, us);
    views_learn(&ui, midi_learned()); /* midi_init ran before */
    views_route(&ui, midi_routed());
    views_sync(&ui, midi_synced());
    /* The settings as they were left (the store mounted in lcd_init's wake-up wait); the full
     * boot once after an install: no record, or another version's. */
    settings_init(&persisted, l, midi_learned(), &ui.cards, &ui.song.scroll, us, store_fm1(), "V" SIDOU_VERSION,
                  settings_force);
    int known = settings_load(&persisted, now_ms);
    views_settings(&ui, &persisted);
    views_boot(&ui, !known, home_held);
    last_compose = now_ms;
    next_at = now_ms + UI_PERIOD_MS / 2u;
    views_frame(&ui, now_ms);
}

/* The first frame has gone out whole and nothing is in transfer: the display can come on
 * without showing the panel's power-on contents. */
static void display_on_when_drawn(void) {
    if (!shown && !pending && !(cells.rows & ~cells.hold) && !lcd_busy()) {
        lcd_on();
        shown = 1;
    }
}

/* A call with nothing to send: the display on once the first frame is out, and a slice of the
 * view's work between frames if it has any (a call renders runs or works a slice, never
 * both). Out of line: the calls at rest test one flag. */
__attribute__((noinline, unused)) static void between(void) {
    display_on_when_drawn();
    if (idle_work) {
        idle_work = (uint8_t)views_idle(&ui);
    }
    at_rest = (uint8_t)(shown && !idle_work);
}

__attribute__((noinline, unused)) static uint32_t unheld_rows(uint32_t rows) {
    return rows & ~cells.hold;
}

void ui_poll(uint32_t now_ms) {
    /* A frame, half a frame later its preparation (views_prepare), and the runs: each in a
     * call of its own, so no call is long and the main loop drains MIDI between them. */
    if ((int32_t)(now_ms - next_at) >= 0) {
        if (now_ms - last_compose >= UI_PERIOD_MS) {
            last_compose = now_ms;
            next_at = now_ms + UI_PERIOD_MS / 2u;
            views_frame(&ui, now_ms);
            if (ui.scene.idle) { /* the frame left work between frames (views_idle) */
                idle_work = 1;
                at_rest = 0;
            }
        } else {
            next_at = last_compose + UI_PERIOD_MS;
            views_prepare(&ui);
        }
        return;
    }
    if (!pending && (!cells.rows || !unheld_rows(cells.rows))) {
        /* nothing to send: most calls, between frames and on an idle screen */
        if (!at_rest) {
            between();
        }
        return;
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

void ui_song(const song_ctl* sc, const song_event* ev, uint32_t n) {
    views_song(&ui, sc, ev, n);
}

void ui_leds(uint32_t now_ms, panel_led_in* out) {
    views_leds(&ui, now_ms, out);
}

void ui_toast(const char* text, uint32_t ms) {
    screen_toast(&ui.s, text, ms);
}

void ui_note_keys(panel* p, uint32_t down, uint32_t pressed) {
    views_note_keys(&ui, p, down, pressed);
}

void ui_update(uint32_t step, int32_t code) {
    grid_hold(&cells, 0); /* a wipe's held rows go out too */
    views_update(&ui, step, code);
    flush();
    if (!shown) { /* an update that starts before the first frame was out */
        lcd_on();
        shown = 1;
    }
}
