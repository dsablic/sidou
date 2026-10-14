// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI in from the TRS jack and USB onto the live instrument.
 *
 * The TIMER5 interrupt polls Felucca's USB stack and TRS UART driver (usb/, vendored), each
 * at 2 kHz. They push every USB-MIDI packet and every TRS byte, stamped with fm1_ticks() at
 * the poll, onto a ring (app/midi_ring.h). The main loop drains the ring through a parser per
 * source onto live (app/midi_rx.h, which has the default mapping). */
#ifndef SIDOU_FM1_MIDI_H
#define SIDOU_FM1_MIDI_H

#include <stdint.h>

#include "live.h"

/* Interrupts off, before TIMER5 starts: the UART (a port H read-modify-write) and USB (which
 * waits 25 ms for the controller's reset). l receives the messages. */
void midi_init(live* l);
/* TIMER5 interrupt, each at 2 kHz, on different ticks. */
void midi_usb_isr(void);
void midi_uart_isr(void);
/* Main loop: takes what came in onto live, leaving the register ring room for the rest of
 * the pass (reg_ring.h's worst pass). Returns the entries taken; if any, the caller commits
 * the register writes (audio_commit). */
uint32_t midi_drain(void);
/* Main loop: a USB retry while no host answers (once a second). */
void midi_service(uint32_t now_ms);

#endif
