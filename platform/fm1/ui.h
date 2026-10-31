// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "player/song_ctl.h"
#include "ui_sound.h"
#include "panel.h"
#include "sid_transport.h"
#include "song_transport.h"
#include "song_io.h"

#define UI_PERIOD_MS 40u

void ui_init(live* l, ui_sound* us, uint32_t now_ms, int home_held);
void ui_set_keys(keys_route* kr);
void ui_set_panel(panel* p);
void ui_set_song(const song_transport* t);
void ui_set_tune(const sid_transport* t);
int ui_tune(int on);
void ui_song_loaded(int back);
void ui_set_song_io(song_io* io);
void ui_song_rows(uint8_t rows_per_beat);
settings* ui_settings(void);
int ui_keep_sound(uint32_t slot, const char* name);
void ui_song_force(int on);
void ui_poll(uint32_t now_ms);
void ui_link(uint32_t now_ms);
void ui_song(const song_ctl* sc, const song_event* ev, uint32_t n);
void ui_buttons(panel* p, uint32_t pressed, uint32_t held);
void ui_encoder(panel* p, uint32_t enc, int32_t steps);
void ui_toast(const char* text, uint32_t ms);
void ui_note_keys(panel* p, uint32_t down, uint32_t pressed);
void ui_leds(uint32_t now_ms, panel_led_in* out);
void ui_update(uint32_t step, int32_t code);

#endif
