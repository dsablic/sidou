// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI in (midi.h) and the update protocol's step 1 (update.h). This file includes Felucca's
 * USB stack, TRS UART driver and update protocol (usb/usb.c, usb/midi_uart.c, src/ota.c,
 * GPL-3.0-only): like the HAL, they are static code for one translation unit, and they call
 * back into the hooks defined here before them; ota.c talks through usb.c's static SysEx
 * plumbing. Only this file includes fm1_usb.h, fm1_uart.h and fm1_flash.h
 * (platform/fm1/VENDORED.md). */
#include <stdint.h>

#include "fm1_sys.h"
#include "fm1_time.h"
/* The app may erase and program the update staging area only (ota.c OTA_AREA, 0xE0000..0xE5000,
 * which holds the update record too); SIDOU keeps no other data in flash yet. */
#define FL_RANGE_OK(off, n) FL_IN(off, n, FL_OTA_LO, FL_OTA_HI)
#include "fm1_flash.h"

#include "audio.h"
#include "input.h"
#include "midi.h"
#include "midi_out.h"
#include "midi_ring.h"
#include "midi_rx.h"
#include "reg_ring.h"
#include "ui.h"
#include "update.h"

/* The register writes one MIDI entry can cause at most. A program change writes every register
 * (live_set_preset, 25). A CC writes at most 6 (the pulse width or the envelopes on every
 * voice, the latter also for the velocity target): the chip model is only recorded here and
 * live_tick applies it, inside the main loop's own budget (reg_ring.c, REG_RING_WORST_PASS), as
 * it applies pitch bend, aftertouch, the wheel on a target other than the pulse width and RPN 0,
 * which write nothing here, so a flood of 14-bit bends costs one tick's writes. A note-on writes
 * at most 11 (reg_ring.c). A TRS byte completes at most one message; a USB packet up to three,
 * through running status (app/midi_usb.h), e.g. CIN 3 with three data bytes after C0, so three
 * program changes (75) are the worst; two CCs fit in a packet (12), three note-ons (33). An
 * entry after a gap of lost input first restarts the parsers and releases every voice
 * (live_all_notes_off: one gate write per voice): 75 + 3 = 78. */
#define MIDI_ENTRY_WRITES (3u * 25u + LIVE_VOICES)
/* After the last entry, midi_rx_check_lost may find a gap that came behind it and release
 * every voice again: 3 writes beyond any entry's. */
#define MIDI_LOST_WRITES LIVE_VOICES
/* The drain takes an entry only while this many are free after it (1024 - 469 = 555 queued
 * at most), so the rest of the pass never waits for the audio ISR on its own account. */
#define MIDI_DRAIN_RESERVE (MIDI_ENTRY_WRITES + MIDI_LOST_WRITES + REG_RING_WORST_PASS)
_Static_assert(MIDI_DRAIN_RESERVE < REG_RING_SIZE, "the ring leaves MIDI no room");

/* MIDI out's settings until M2 stores settings (CMake: SIDOU_MIDI_OUT_CHANNEL, 1..16, and
 * SIDOU_MIDI_OUT_NRPN; docs/reference/midi.md, "Output"). */
#ifndef SIDOU_MIDI_OUT_CHANNEL
#define SIDOU_MIDI_OUT_CHANNEL 1
#endif
#ifndef SIDOU_MIDI_OUT_NRPN
#define SIDOU_MIDI_OUT_NRPN 0
#endif
_Static_assert(SIDOU_MIDI_OUT_CHANNEL >= 1 && SIDOU_MIDI_OUT_CHANNEL <= 16,
               "SIDOU_MIDI_OUT_CHANNEL is 1..16");

static midi_ring in_ring;
static midi_rx rx;
/* Read by the emulator tests through the ELF symbol (its counters, app/midi_out.h). */
static midi_out midi_tx __attribute__((used));
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
/* The update protocol's SysEx frames and TX queue in usb.c (ota.c below). Never the stock UBOOT
 * soft key F0 22 24 35 7D F7: usb.c no longer knows it. */
#define FELUCCA_OTA 1
/* The identity the protocol answers (cmd 0x11), == the package's (sidou-pack --product):
 * "FM-1_900" for development builds, as every community firmware's; CMakeLists.txt. */
#define FELUCCA_ID SIDOU_PRODUCT

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
#include "src/ota.c"
/* clang-format on */

_Static_assert(MIDI_SRC_USB == 1u && MIDI_SRC_TRS == 2u, "usb.c and midi_uart.c push 1 and 2");

/* ---- midi.h ---- */

void midi_init(live* l) {
    midi_ring_init(&in_ring);
    midi_rx_init(&rx, l, midi_ring_epoch(&in_ring));
    midi_out_init(&midi_tx, l, (uint8_t)(SIDOU_MIDI_OUT_CHANNEL - 1), SIDOU_MIDI_OUT_NRPN != 0);
    midi_rx_watch(&rx, &midi_tx);
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
    /* Leave the entry, a later gap and the rest of the pass their worst cases
     * (MIDI_DRAIN_RESERVE); what stays in the ring is taken after the next audio block. */
    while (audio_queued() + MIDI_DRAIN_RESERVE <= REG_RING_SIZE &&
           midi_ring_pop(&in_ring, &e)) {
        uint32_t wait = fm1_ticks() - e.stamp;
        if (wait > midi_stats.max_wait_ticks) {
            midi_stats.max_wait_ticks = wait;
        }
        midi_rx_entry(&rx, &e);
        n++;
    }
    /* MIDI out's second look, once a pass (midi_rx_watch). Since the first, nothing but this
     * loop changed a parameter: midi_drain runs to completion in the main loop, the panel
     * changes parameters only in main.c's play(), after it (ui_poll only reads live), and the
     * interrupts (key scan, MIDI polls, audio render) change none. */
    midi_rx_settle(&rx);
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

void midi_send(void) {
    /* No host, or one asleep or gone (usb.c: the frame number stopped): forget the changes
     * rather than queue them for a host that may come back much later. */
    if (!usb.config || usb.suspended) {
        midi_out_drop(&midi_tx, rx.l);
        return;
    }
    /* The ring's room now; the TIMER5 interrupt only ever frees more (ep1_tx). */
    uint32_t room = MQ - (mo_w - mo_r);
    uint32_t pkt[MIDI_OUT_TICK_PACKETS];
    uint32_t n = midi_out_tick(&midi_tx, rx.l, room, pkt);
    for (uint32_t i = 0; i < n; i++) {
        midi_out_event(pkt[i]);
    }
}

void midi_service(uint32_t now_ms) {
    usb_retry(now_ms);
}

/* ---- ota.c's hooks (Felucca's ota_hw.c, adapted) ---- */

static uint8_t flash_ok;           /* the JEDEC id matched at boot */
static void (*commit_hook)(void);  /* update_session's before_reset */

/* Monotonic milliseconds from TIMER4, across its wrap (178.9 s); as the loader's ldr_ms. */
static uint32_t ota_now_ms(void) {
    static uint32_t last, acc, ms;
    uint32_t now = fm1_ticks();
    acc += now - last;
    last = now;
    ms += acc / (1000u * FM1_TICKS_PER_US);
    acc %= 1000u * FM1_TICKS_PER_US;
    return ms;
}

/* The TIMER5 interrupt keeps polling USB while the session waits for the host. */
static void ota_idle(void) {
    fm1_wdt_feed();
}

/* ota_in_area, flash_write_allowed, ota_erase, ota_prog, ota_fread (host-tested). */
#include "ota_flash.h"

static void ota_show(uint32_t step, int32_t code) {
    ui_update(step, code);
    if (step >= 9u) {
        fm1_delay_ms(1500); /* the result stays readable */
    }
}

static void ota_commit(const uint8_t* parm) {
    if (commit_hook) {
        commit_hook();
    }
    usb_detach();
    fm1_delay_ms(30);
    fm1_enter_update(parm); /* the record into RAM, then a core reset: the SPL runs the loader */
}

/* ---- update.h ---- */

/* The FM-1's 1 MiB NOR (fm1_flash.h). */
#define FLASH_JEDEC 0x856014u

void update_boot(void) {
    uint32_t f = irq_save(); /* (its sti is harmless here: no interrupt source is on yet) */
    uint32_t id = FL_FAR(fl_jedec_ram)();
    irq_restore(f);
    flash_ok = id == FLASH_JEDEC;
    if (flash_ok) {
        ota_boot_cleanup();
    }
}

int update_poll(void) {
    if (!sx_ready && !usb.ota_req) {
        return 0; /* the usual case, at every main-loop wake */
    }
    ota_service();
    if (usb.ota_req) {
        usb.ota_req = 0;
        return 1;
    }
    return 0;
}

int update_session(void (*before_reset)(void)) {
    if (!flash_ok) {
        ota_show(9, -17); /* not the flash chip we know: nothing is written */
        return -17;
    }
    commit_hook = before_reset;
    return ota_session();
}
