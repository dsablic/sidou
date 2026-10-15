// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screen, on the cell-grid renderer (app/grid.h) in SIDOU's font and palette
 * (docs/art/README.md). The status screen: title and version, chip model, preset and octave,
 * the four sound parameters as numbers and bars, and the notes that sound; light blue on blue
 * inside a light blue border. */
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "live.h"

/* The screen is composed at most this often (25 fps); only cells that changed are sent. */
#define UI_PERIOD_MS 40u

/* The screens ui_screen selects. UI_CHARSET shows the font and the palette: the art reference
 * and the emulator tests' glyph-sheet snapshot. The view framework (M1e Task 2) replaces this
 * switch. */
#define UI_STATUS 0u
#define UI_CHARSET 1u
extern volatile uint8_t ui_screen;

/* After lcd_init: draws the whole screen (blocking) and switches the display on. */
void ui_init(const live* l);
/* Main loop. Every UI_PERIOD_MS it composes the screen. Changed cells go out as runs (one LCD
 * window each), one per call and only while the panel is free; the next run is rendered while
 * the last one transfers, so the main loop never waits on the panel. */
void ui_poll(const live* l, uint32_t now_ms);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code < 0 a failure). Draws at once (blocking): the main
 * loop is inside the update session and does not call ui_poll. */
void ui_update(uint32_t step, int32_t code);

#endif
