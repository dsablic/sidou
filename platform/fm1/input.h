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
/* Main loop: button, note-key and encoder changes since the last call into live. */
void input_poll(panel* p, live* l);
/* Main loop: the MASTER pot, 0..1023, or -1 on an ADC timeout. */
int32_t input_master(void);
/* Main loop: held keys, the octave and the chip model on the LEDs. */
void input_leds(const panel* p, const live* l);

#endif
