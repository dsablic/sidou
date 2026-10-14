// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The status screen: title and version, chip model, preset and octave, the four sound
 * parameters as numbers and bars, and the notes that sound. C64 colours: light blue on
 * blue inside a light blue border. 8 x 8 text cells from font8x8_basic, a placeholder until
 * our own PETSCII-style font. */
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "lcd.h"
#include "live.h"

/* The panel's colours (lcd.h byte order): the C64's blue and light blue. */
#define UI_BACKGROUND LCD_RGB(0x35, 0x28, 0x79)
#define UI_FOREGROUND LCD_RGB(0x6C, 0x5E, 0xB5)
/* The screen is looked at again at most this often, and redrawn only where it changed. */
#define UI_PERIOD_MS 40u

/* After lcd_init: draws the whole screen (blocking) and switches the display on. */
void ui_init(const live* l);
/* Main loop. Every UI_PERIOD_MS it composes the screen; strips that changed go out one per
 * call, and only while the panel is not busy with the last one. */
void ui_poll(const live* l, uint32_t now_ms);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code < 0 a failure). Draws at once (blocking): the main
 * loop is inside the update session and does not call ui_poll. */
void ui_update(uint32_t step, int32_t code);

#endif
