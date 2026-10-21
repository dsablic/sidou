/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only */
/* MIDI IN on the TRS jack: PH8 -> input channel 1 -> UART1 RX, 31250 baud,
 * RX DMA into a 128-byte ring, polled from the TIMER5 ISR (no UART IRQ).
 * SIDOU: adapted from Felucca's firmware/src/midi_uart.c (platform/fm1/VENDORED.md).
 * Every byte goes as it is to the includer's midi_enqueue (source 2 = TRS),
 * which stamps it and rings it to the main loop; the MIDI parser there
 * (app/midi_in.c) handles running status, real time and SysEx. Felucca's
 * parser into USB-MIDI packets is gone. Bytes the DMA ring overwrote before
 * a poll took them are reported to midi_lost(n). */
#include "../hal/fm1_uart.h"   /* registers; relative, so the host tests find it too */

#define UM_RING 128u
static volatile uint8_t um_ring[UM_RING] __attribute__((aligned(16)));
static struct {
    uint32_t rd, pend;
    volatile uint32_t bytes, drops;                /* TIMER5 writes; diagnostics */
} um;

static void uart_midi_init(void)                   /* before timer5_start(): PORTH RMW */
{
    fm1_uart1_midi_init(um_ring, UM_RING);
}

static void um_byte(uint32_t b)
{
    if (!midi_enqueue(b, 2u))
        um.drops++;
}

static void uart_midi_take(uint32_t n)
{
    um.pend += n;
    if (um.pend > UM_RING) {
        um.drops += um.pend - UM_RING;
        midi_lost(um.pend - UM_RING);               /* the main loop restarts its parser */
        um.rd = (um.rd + um.pend - UM_RING) & (UM_RING - 1u);
        um.pend = UM_RING;
    }
    while (um.pend) {
        um_byte(um_ring[um.rd]);
        um.rd = (um.rd + 1u) & (UM_RING - 1u);
        um.pend--;
        um.bytes++;
    }
}
static void uart_midi_poll(void)                   /* TIMER5 ISR, same context as usb_poll */
{
    uart_midi_take(fm1_uart1_rx_take());
}
