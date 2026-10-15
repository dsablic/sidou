// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The key matrix, encoders, LEDs and MASTER pot (Felucca's fm1_input.h and fm1_adc.h, which
 * only this file includes), handed to the portable panel logic (app/panel.h). */
#ifndef SIDOU_FM1_INPUT_H
#define SIDOU_FM1_INPUT_H

#include <stdint.h>

#include "live.h"
#include "panel.h"

/* Matrix pins and the ADC, with interrupts off (the scan shares port A with PA2). */
void input_init(void);
/* From the 10 kHz TIMER5 interrupt: one matrix column. */
void input_tick_isr(void);
/* The key scan's frame count (one frame = 11 TIMER5 ticks, ~1.1 ms): the debounced state
 * and the edges only change when it does. */
uint32_t input_frame(void);
/* Main loop: button, note-key and encoder changes since the last call into live; buttons and
 * encoders through the views first (ui_buttons, ui_encoder), the note keys straight to it.
 * Only once interrupts are on: its critical sections end with fm1__unlock, which always
 * re-enables interrupts (sti), whatever the state before. */
void input_poll(panel* p, live* l);
/* The debounced buttons (bit i: button i), as the key scan has them now. */
uint32_t input_buttons(void);
/* Once interrupts are on, before the main loop: waits FM1_DEB_PRESS + 1 key scan frames
 * (about 3.3 ms), long enough for a key held since power-on to pass the debounce, and returns
 * the debounced buttons. The press edge the scan makes for such a key stays for input_poll. */
uint32_t input_buttons_at_power_on(void);
/* Main loop: the MASTER pot, 0..1023, or -1 on an ADC timeout. */
int32_t input_master(void);
/* With interrupts off, before a flash erase (~17 ms without the scan): all LED rows dark, so
 * the column the scan left on does not stay lit at full duty (Felucca's storage_hw.c, #236).
 * The scan lights them again at its next pass. */
void input_leds_dark(void);
/* Main loop: held keys, the octave and the chip model on the LEDs. */
void input_leds(const panel* p, const live* l);

#endif
