// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The key matrix, encoders, LEDs and MASTER pot (Felucca's fm1_input.h and fm1_adc.h, which
 * only this file includes), handed to the portable panel logic (app/panel.h). */
#ifndef SIDOU_FM1_INPUT_H
#define SIDOU_FM1_INPUT_H

#include <stdint.h>

#include "live.h"
#include "panel.h"

/* fm1_cstart's power-on UBOOT gesture (main.c), first thing at boot: before memory is set
 * up, so it touches no variable, only GPIO (the matrix's rows, the 74HC595's three lines,
 * PA2, the LED lines) and TIMER4 (fm1_time_init has run). One whole matrix scan with the LED
 * lines dark: the keys closed now, undebounced, as INPUT_RAW_* below. PA2 (the 595's /OE and
 * the backlight, active low) is driven low for the scan's ~0.3 ms, then put back as it was.
 * Interrupts off. */
#define INPUT_RAW_NOTES (1u << 31) /* any note key closed */
uint32_t input_raw_keys(void);
/* After the gesture: PA2 high, so the backlight goes dark (and the 595's outputs off), the
 * gesture's confirmation before the reset into UBOOT. */
void input_raw_dark(void);
/* Matrix pins and the ADC, with interrupts off (the scan shares port A with PA2). */
void input_init(void);
/* From the 10 kHz TIMER5 interrupt: one matrix column. */
void input_tick_isr(void);
/* The key scan's frame count (one frame = 11 TIMER5 ticks, ~1.1 ms): the debounced state
 * and the edges only change when it does. */
uint32_t input_frame(void);
/* Main loop: button, note-key and encoder changes since the last call into live; buttons and
 * encoders through the views first (ui_buttons, ui_encoder), then the note keys
 * (ui_note_keys).
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
/* Main loop: the LEDs as panel_get_leds has them (app/panel.h), lit and dim, at most every
 * LEDS_PERIOD_MS of now_ms. Dim is the HAL's glow (fm1_led_dim), which its scan pulses. */
void input_leds(const panel* p, const live* l, uint32_t now_ms);

#endif
