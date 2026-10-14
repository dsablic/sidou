// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI in (midi.h). This file includes Felucca's USB stack and TRS UART driver (usb/usb.c and
 * usb/midi_uart.c, GPL-3.0-only): like the HAL, they are static code for one translation
 * unit, and they call back into the hooks defined here before them. Only this file includes
 * fm1_usb.h and fm1_uart.h (platform/fm1/VENDORED.md). */
#include <stdint.h>

#include "fm1_time.h"

#include "audio.h"
#include "midi.h"
#include "midi_ring.h"
#include "midi_rx.h"
#include "reg_ring.h"

/* The register writes one MIDI entry can cause at most. A program change writes every register
 * (live_set_preset, 25). A TRS byte completes at most one message; a USB packet up to three,
 * through running status (app/midi_usb.h), e.g. CIN 3 with three data bytes after C0. */
#define MIDI_ENTRY_WRITES (3u * 25u)

static midi_ring in_ring;
static midi_rx rx;
/* fm1_ticks() at the start of the poll that is running: every entry it pushes gets it. */
static uint32_t poll_stamp;

/* Diagnostics, read by the emulator tests through the ELF symbol: six uint32_t, in this order. */
static struct {
    uint32_t entries;        /* ring entries taken onto live */
    uint32_t lost;           /* midi_ring_lost at the last drain that saw a loss */
    uint32_t restarts;       /* parser restarts after lost input */
    uint32_t max_wait_ticks; /* the longest an entry waited in the ring, 24 MHz ticks */
    uint32_t usb_polls;      /* USB polls with the controller up */
    uint32_t uart_polls;
} midi_stats __attribute__((used));

/* ---- the hooks of the vendored code ---- */

/* The ring's index stores are ordered by midi_ring.c itself; this orders usb.c's own rings. */
#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
/* The update protocol's SysEx frames and TX queue in usb.c, built out for now.
 * TODO(Task 6): turn on with the update protocol (Felucca's ota.c). Never the stock UBOOT soft
 * key F0 22 24 35 7D F7: usb.c no longer knows it. */
#define FELUCCA_OTA 0

/* source: 1 a USB-MIDI packet, 2 a TRS byte (MIDI_SRC_*). 0 when the ring is full. */
static int midi_enqueue(uint32_t data, uint32_t source) {
    return midi_ring_push(&in_ring, (uint8_t)source, data, poll_stamp);
}

static uint32_t midi_in_room(void) {
    return midi_ring_room(&in_ring);
}

static void midi_lost(uint32_t n) {
    midi_ring_lose(&in_ring, n);
}

/* clang-format off */
#include "usb/usb.c"
#include "usb/midi_uart.c"
/* clang-format on */

_Static_assert(MIDI_SRC_USB == 1u && MIDI_SRC_TRS == 2u, "usb.c and midi_uart.c push 1 and 2");

/* ---- midi.h ---- */

void midi_init(live* l) {
    midi_ring_init(&in_ring);
    midi_rx_init(&rx, l, midi_ring_epoch(&in_ring));
    uart_midi_init();
    usb_start();
}

void midi_usb_isr(void) {
    poll_stamp = fm1_ticks();
    if (usb.up) {
        midi_stats.usb_polls++;
    }
    usb_poll();
}

void midi_uart_isr(void) {
    poll_stamp = fm1_ticks();
    midi_stats.uart_polls++;
    uart_midi_poll();
}

int midi_drain(void) {
    midi_ring_entry e;
    uint32_t n = 0;
    uint32_t restarts = rx.restarts;
    /* The epoch first, then the emptiness: a gap after this read is caught next time. */
    uint8_t epoch = in_ring.epoch;
    if (in_ring.head == in_ring.tail && epoch == rx.epoch) {
        return 0; /* the main loop wakes at 10 kHz: keep the usual case to a few loads */
    }
    /* Leave the rest of the pass its worst case, so it never waits for the audio ISR on its
     * own account; what stays in the ring is taken after the next audio block. */
    while (audio_queued() + MIDI_ENTRY_WRITES + REG_RING_WORST_PASS <= REG_RING_SIZE &&
           midi_ring_pop(&in_ring, &e)) {
        uint32_t wait = fm1_ticks() - e.stamp;
        if (wait > midi_stats.max_wait_ticks) {
            midi_stats.max_wait_ticks = wait;
        }
        midi_rx_entry(&rx, &e);
        n++;
    }
    if (in_ring.head == in_ring.tail) {
        /* All taken: a gap after the last entry, with nothing behind it yet. Its epoch was
         * read before the ring was seen empty, so every entry before it has been taken. */
        midi_rx_check_lost(&rx, epoch);
    }
    midi_stats.entries += n;
    if (rx.restarts != restarts) {
        midi_stats.lost = midi_ring_lost(&in_ring);
        midi_stats.restarts = rx.restarts;
        return 1; /* the restart released the notes: there are writes to commit */
    }
    return n != 0;
}

void midi_service(uint32_t now_ms) {
    usb_retry(now_ms);
}
