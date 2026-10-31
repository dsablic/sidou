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

#define VERSION_CELLS 9u
_Static_assert(sizeof SIDOU_VERSION <= VERSION_CELLS, "SIDOU_VERSION is too long for row 2");

_Static_assert(GRID_RUN_PIXELS <= LCD_BUFFER_PIXELS, "a grid run does not fit an LCD buffer");

static grid cells __attribute__((section(".pool")));
static views ui;
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
static uint16_t palette[16];
static uint32_t last_compose;
static uint32_t next_at;
static grid_run run;
static uint8_t pending;
static uint8_t shown;
static uint8_t idle_work;
static uint8_t at_rest;

#define STAMP_ROW (GRID_ROWS - 1u)
#define STAMP_CELLS 2u
static const uint16_t stamp_rows[8] = {0, 0x452A, 0x45AC, 0x4568, 0x452C, 0x752A, 0, 0};
static uint8_t stamped;

__attribute__((noinline)) static void stamp(uint16_t* px) {
    uint32_t w = run.len * GRID_CELL_PX;
    uint32_t x0 = run.col * GRID_CELL_PX;
    uint32_t x1 = STAMP_CELLS * GRID_CELL_PX;
    for (uint32_t y = 0; y < GRID_CELL_PX; y++) {
        for (uint32_t x = x0; x < x1 && x - x0 < w; x++) {
            px[y * w + x - x0] =
                stamp_rows[y] >> (15u - x) & 1u ? palette[GRID_LIGHT_GREEN] : palette[GRID_BLACK];
        }
    }
}

static int prepare(void) {
    if (!pending && grid_next_run(&cells, &run)) {
        grid_render_run(&cells, &run, palette, lcd_buffer());
        if (stamped && run.row == STAMP_ROW && run.col < STAMP_CELLS) {
            stamp(lcd_buffer());
        }
        pending = 1;
    }
    return pending;
}

static void send(void) {
    lcd_send(run.col * GRID_CELL_PX, run.row * GRID_CELL_PX, run.len * GRID_CELL_PX,
             GRID_CELL_PX);
    if (midi_link_active) {
        midi_link_mark(run.row, run.col, run.len);
    }
    pending = 0;
}

void ui_link(uint32_t now_ms) {
    midi_link_run(&cells, now_ms);
    uint8_t on = (uint8_t)midi_link_on();
    if (on != stamped) {
        stamped = on;
        for (uint32_t c = 0; c < STAMP_CELLS; c++) {
            grid_mark(&cells, c, STAMP_ROW);
        }
    }
}

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

static const song_transport* song_bound;

void ui_set_song(const song_transport* t) {
    song_bound = t;
    views_set_song(&ui, t);
}

void ui_set_tune(const sid_transport* t) {
    views_set_tune(&ui, t);
}

int ui_tune(int on) {
    if (!on) {
        views_tune(&ui, 0);
        views_set_song(&ui, song_bound);
        return 1;
    }
    if (ui.pages.pending) {
        return 0;
    }
    views_set_song(&ui, NULL);
    if (ui.song.t) {
        return 0;
    }
    views_tune(&ui, 1);
    return 1;
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

int ui_keep_sound(uint32_t slot, const char* name) {
    uint8_t sound[LAB_SOUND_BYTES];
    live_sound_get(ui.l, sound);
    return user_sound_save(&ui.sounds, slot, name, sound, ui.s.now);
}

void ui_init(live* l, ui_sound* us, uint32_t now_ms, int home_held) {
    for (uint32_t i = 0; i < 16u; i++) {
        palette[i] = LCD_SWAP(grid_palette[i]);
    }
    grid_init(&cells, ' ', VIEWS_TEXT);
    views_init(&ui, &cells, l, audio_scope(), "V" SIDOU_VERSION);
    views_set_sounds(&ui, us);
    views_learn(&ui, midi_learned());
    views_route(&ui, midi_routed());
    views_sync(&ui, midi_synced());
    settings_init(&persisted, l, midi_learned(), &ui.cards, &ui.song.scroll, us, store_fm1(), "V" SIDOU_VERSION,
                  settings_force);
    views_sounds(&ui, store_fm1());
    settings_sounds(&persisted, &ui.sounds);
    int known = settings_load(&persisted, now_ms);
    views_settings(&ui, &persisted);
    views_boot(&ui, !known, home_held);
    last_compose = now_ms;
    next_at = now_ms + UI_PERIOD_MS / 2u;
    views_frame(&ui, now_ms);
}

static void display_on_when_drawn(void) {
    if (!shown && !pending && !(cells.rows & ~cells.hold) && !lcd_busy()) {
        lcd_on();
        shown = 1;
    }
}

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
    if ((int32_t)(now_ms - next_at) >= 0) {
        if (now_ms - last_compose >= UI_PERIOD_MS) {
            last_compose = now_ms;
            next_at = now_ms + UI_PERIOD_MS / 2u;
            views_frame(&ui, now_ms);
            if (ui.scene.idle) {
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
        if (!at_rest) {
            between();
        }
        return;
    }
    if (prepare() && !lcd_busy()) {
        send();
        prepare();
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
    grid_hold(&cells, 0);
    views_update(&ui, step, code);
    flush();
    if (!shown) {
        lcd_on();
        shown = 1;
    }
}
