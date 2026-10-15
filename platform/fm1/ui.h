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

/* After lcd_init: the status view of l, drawn whole (blocking), and the display on. */
void ui_init(live* l);
/* Main loop. Every UI_PERIOD_MS it composes a frame (views_frame). Changed cells go out as
 * runs (one LCD window each), one per call and only while the panel is free; the next run is
 * rendered while the last one transfers, so the main loop never waits on the panel. */
void ui_poll(uint32_t now_ms);
/* Main loop, from input_poll: button press edges and the held buttons, and an encoder's
 * detents, through the views to the panel (views_buttons, views_encoder). */
void ui_buttons(panel* p, uint32_t pressed, uint32_t held);
void ui_encoder(panel* p, uint32_t enc, int32_t steps);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code < 0 a failure), in a frame over the view. Draws
 * at once (blocking): the main loop is inside the update session and does not call
 * ui_poll. */
void ui_update(uint32_t step, int32_t code);

#endif
