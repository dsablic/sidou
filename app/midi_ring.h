// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI input from the interrupt that polls the ports to the main loop.
 *
 * Portable C with no hardware access, so the host tests run the very code the device runs.
 *
 * The producer is the TIMER5 interrupt: it polls USB and the TRS jack's UART at 2 kHz and
 * pushes what came in, stamped with the time of the poll. The consumer is the main loop,
 * which pops the entries into the MIDI parsers (midi_rx.h).
 *
 * Single producer, single consumer, one core: the producer is an interrupt that preempts the
 * consumer, never the other way round. Each index is written by one side only, and compiler
 * fences keep an entry's stores before the index store that publishes it, and its loads
 * before the index store that hands its slot back.
 *
 * A full ring drops the new entry and counts it in `lost`; so does the producer for input it
 * lost before the ring (midi_ring_lose: the UART's DMA ring overran). The consumer compares
 * the count with what it saw before and recovers (midi_rx_check_lost). The USB side never
 * gets there: it leaves a packet in the endpoint (the host is NAKed and retries) while the
 * ring has less room than a packet's events. */
#ifndef SIDOU_MIDI_RING_H
#define SIDOU_MIDI_RING_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Entries in the ring, a power of two: 256 TRS bytes are 82 ms at 31250 baud. */
#define MIDI_RING_SIZE 256u

/* Where an entry came from, and what its data is. */
#define MIDI_SRC_USB 1u /* a USB-MIDI event packet as it lay in the endpoint, little-endian */
#define MIDI_SRC_TRS 2u /* one byte from the TRS jack, 0x00..0xFF */

typedef struct {
    uint32_t stamp; /* when the poll took it in: fm1_ticks() on the device (24 MHz) */
    uint32_t data;
    uint8_t source; /* MIDI_SRC_* */
} midi_ring_entry;

typedef struct {
    midi_ring_entry e[MIDI_RING_SIZE];
    volatile uint32_t head; /* producer: entries before it are visible to the consumer */
    volatile uint32_t tail; /* consumer: entries before it have been taken */
    volatile uint32_t lost; /* producer: inputs dropped, by a full ring or before it */
} midi_ring;

void midi_ring_init(midi_ring* r);
/* Producer. 1 on success; 0 when the ring is full: nothing is written and `lost` counts it. */
int midi_ring_push(midi_ring* r, uint8_t source, uint32_t data, uint32_t stamp);
/* Producer: free entries. */
uint32_t midi_ring_room(const midi_ring* r);
/* Producer: n inputs were lost before they reached the ring. */
void midi_ring_lose(midi_ring* r, uint32_t n);
/* Consumer. 1 and the oldest entry, or 0 when there is none. */
int midi_ring_pop(midi_ring* r, midi_ring_entry* out);
/* Either side: inputs lost so far (wraps). */
uint32_t midi_ring_lost(const midi_ring* r);

#ifdef __cplusplus
}
#endif

#endif
