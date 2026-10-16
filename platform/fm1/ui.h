// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screen on the LCD: SIDOU's views (app/views.h) composed on the cell grid (app/grid.h) in
 * SIDOU's font and palette (docs/art/README.md), and the panel's buttons and encoders routed
 * through them. Main loop only; nothing here runs in an interrupt or from RAM. */
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "ui_sound.h"
#include "panel.h"

/* The screen is composed at most this often (25 fps); only cells that changed are sent. */
#define UI_PERIOD_MS 40u

/* After lcd_init: LIVE, the root view, of l and the audio's tap (audio_scope), under the boot
 * sequence (app/boot.h), the full one if home_held (HOME held at power-on), and its first
 * frame composed at now_ms. It does not wait
 * on the panel: ui_poll sends that frame like any other (the whole screen, about 80 ms of
 * SPI) and turns the display on once it is out, so the main loop plays the keys and MIDI from
 * the start. now_ms is on ui_poll's clock: the main loop's uptime, which counts from TIMER4's
 * start, not from here. */
void ui_init(live* l, uint32_t now_ms, int home_held);
/* The two chips' routing, for the CHIP view's tabs (app/keys_route.h). */
void ui_set_keys(keys_route* kr);
/* The UI sounds (app/ui_sound.h), for the menu's SOUNDS and the triggers. */
void ui_set_sounds(ui_sound* us);
/* Main loop. Every UI_PERIOD_MS it runs a frame (views_frame: the view composes if what it
 * shows changed, app/screen.h), and half a frame later lets the view prepare the next one
 * (views_prepare), each in a call of its own. Changed cells go out as runs (one LCD window each), one per
 * call and only while the panel is free; the next run is rendered while the last one
 * transfers, so the main loop never waits on the panel. A call with nothing to send returns
 * at once; the first one after the first frame is out turns the display on. */
void ui_poll(uint32_t now_ms);
/* Main loop, from input_poll: button press edges and the held buttons, and an encoder's
 * detents, through the views to the panel (views_buttons, views_encoder). */
void ui_buttons(panel* p, uint32_t pressed, uint32_t held);
void ui_encoder(panel* p, uint32_t enc, int32_t steps);
/* The update protocol's progress (views_update) in a frame over the view. Draws at once
 * (blocking): the main loop is inside the update session and does not call ui_poll. */
void ui_update(uint32_t step, int32_t code);

#endif
