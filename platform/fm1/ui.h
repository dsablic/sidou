// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screen on the LCD: SIDOU's views (app/views.h) composed on the cell grid (app/grid.h) in
 * SIDOU's font and palette (docs/art/README.md), and the panel's buttons and encoders routed
 * through them. Main loop only; nothing here runs in an interrupt or from RAM. */
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "live.h"
#include "panel.h"

/* The screen is composed at most this often (25 fps); only cells that changed are sent. */
#define UI_PERIOD_MS 40u

/* After lcd_init: the status view of l under the boot sequence (app/boot.h), its first frame
 * drawn whole (blocking) at now_ms, and the display on. now_ms is on ui_poll's clock: the
 * main loop's uptime, which counts from the timer's start at power-on, not from here. */
void ui_init(live* l, uint32_t now_ms);
/* Main loop. Every UI_PERIOD_MS it runs a frame (views_frame: the view composes if what it
 * shows changed, app/screen.h). Changed cells go out as runs (one LCD window each), one per
 * call and only while the panel is free; the next run is rendered while the last one
 * transfers, so the main loop never waits on the panel. A call with nothing to send returns
 * at once. */
void ui_poll(uint32_t now_ms);
/* Main loop, from input_poll: button press edges and the held buttons, and an encoder's
 * detents, through the views to the panel (views_buttons, views_encoder). */
void ui_buttons(panel* p, uint32_t pressed, uint32_t held);
void ui_encoder(panel* p, uint32_t enc, int32_t steps);
/* The update protocol's progress (views_update) in a frame over the view. Draws at once
 * (blocking): the main loop is inside the update session and does not call ui_poll. */
void ui_update(uint32_t step, int32_t code);

#endif
