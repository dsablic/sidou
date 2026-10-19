// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The store on the device (M2a Task 4): app/store.h's flash hooks, the write windows, the held
 * operations, the quiet detector and the main loop's share of the store.
 *
 * How the store writes while the audio plays (docs/hardware.md, "How the store writes"):
 * - a program in a window: with interrupts masked, at most STORE_WINDOW_LEAD_US after the end
 *   of an audio block and with no block due, so it ends long before the next block's
 *   interrupt; else STORE_LATER at once, and the store tries again on a later pass;
 * - a held operation (an erase; a program too while no window fits): only when the device is
 *   quiet, with the audio faded out to silence and the DMA looping silence, interrupts masked
 *   at most STORE_HOLD_US; else STORE_LATER at once, so no caller can start one while the
 *   audio plays.
 *
 * The timings are PROVISIONAL: the P25Q80H datasheet's maxima (Table 5-4), until the M2a Task
 * 3b spike measures the unit's part. To retune, set the STORE_*_MAX_US below from the spike:
 * STORE_WINDOW_BYTES and the erase mode follow from them by the plan's decision rule. */
#ifndef SIDOU_FM1_STORE_FM1_H
#define SIDOU_FM1_STORE_FM1_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "store.h"

/* ---- the part's timings: PROVISIONAL, the P25Q80H datasheet's maxima (Task 3b retunes) ---- */
#define STORE_PROG16_MAX_US 3000u   /* a page program of 16 bytes (tPP: 3 ms for up to 256) */
#define STORE_PROG64_MAX_US 3000u   /* ... of 64 bytes */
#define STORE_PROG256_MAX_US 3000u  /* ... of 256 bytes */
#define STORE_ERASE_MAX_US 20000u   /* a 4 KiB sector erase (tSE) */
#define STORE_DRIVER_US 100u        /* the driver around one operation (estimate; the emulator
                                       measures its instructions, emu-gate store.rs) */

/* ---- the budgets (M2a plan) ---- */
#define STORE_WINDOW_US 1000u      /* a write window masks interrupts at most this long */
#define STORE_WINDOW_LEAD_US 250u  /* and starts at most this long after a block's end */
#define STORE_HOLD_US 35000u       /* a held operation masks interrupts at most this long */
#define STORE_HOLD_GAP_MS 10u      /* between held operations: TIMER5 drains the TRS ring */
#define STORE_FORCE_GAP_MS 100u    /* between a forced save's held erases (store_fm1_force) */
#define STORE_QUIET_MS 500u        /* quiet: no key, no MIDI input, the output low, this long */

/* The decision rule (docs/hardware.md, "Open checks"): programs go in windows of the largest of
 * 256, 64 and 16 bytes whose worst program and the driver fit STORE_WINDOW_US; 0 when none
 * fits: programs are held operations then (quiet only), and a settings save waits for quiet. */
#define STORE_WINDOW_BYTES                                                                      \
    (STORE_PROG256_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US  ? 256u                          \
     : STORE_PROG64_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US ? 64u                           \
     : STORE_PROG16_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US ? 16u                           \
                                                                : 0u)
/* Erases: a held 4 KiB sector erase if its worst fits 30 ms (35 ms less the driver's margin).
 * The rule's other modes (16 held page erases, erase-suspend slices, erases only at boot) are
 * not built: a spike that needs one fails here. */
_Static_assert(STORE_ERASE_MAX_US <= 30000u,
               "the sector erase does not fit a hold: build the decision rule's next erase mode");
_Static_assert(STORE_ERASE_MAX_US + STORE_DRIVER_US <= STORE_HOLD_US, "a held erase fits");
_Static_assert(STORE_PROG256_MAX_US + STORE_DRIVER_US <= STORE_HOLD_US, "a held program fits");

/* Why 250 us (M2a plan, Task 4): a block is 128 frames at 44,117.6 Hz, 2,901 us; the audio
 * interrupt's worst fill (2,700 instructions a sample) is 1,440 us at 240 MHz, so the next
 * block's interrupt comes at least 1,461 us after a block's end. A window that starts within
 * STORE_WINDOW_LEAD_US and lasts at most STORE_WINDOW_US ends 211 us before it at the worst. */
#define STORE_BLOCK_US 2901u
#define STORE_FILL_WORST_US 1440u
_Static_assert(STORE_WINDOW_LEAD_US + STORE_WINDOW_US < STORE_BLOCK_US - STORE_FILL_WORST_US,
               "a window never delays the next block's interrupt");

/* Counts for the emulator tests and the hardware checklist. */
typedef struct {
    uint32_t progs, erases;  /* operations done (every one a place a power cut can fall) */
    uint32_t window_later;   /* programs refused: outside a window */
    uint32_t hold_later;     /* operations refused: not quiet, or the hold not yet silent */
    uint32_t holds;          /* held operations */
    uint32_t errors;         /* operations the driver failed */
} store_fm1_stats;

/* A test hook the emulator tests drive by writing its RAM (zero on the device, where nothing
 * writes it): op, then the main loop does it once the store rests and sets op back to 0. */
enum {
    STORE_FM1_PUT = 1,     /* key, len bytes of data, type 1 */
    STORE_FM1_GET,         /* key: result its length or STORE_NOT_FOUND, data its first bytes */
    STORE_FM1_FILE,        /* a file "TEST" of len bytes (byte i = i * 7 + key), streamed */
    STORE_FM1_COLLECT,     /* store_collect */
    STORE_FM1_SONG,        /* len 1: a song plays on chip A (keys_route_set_song), 0: none */
    STORE_FM1_FORCE,       /* len 1: store_fm1_force on, 0 off */
};
#define STORE_FM1_DATA 16u
typedef struct {
    volatile uint32_t op;
    uint32_t key;
    uint32_t len;
    int32_t result;
    uint8_t data[STORE_FM1_DATA];
} store_fm1_test;

/* Boot, interrupts off, the plain XIP window on, before the audio starts (main.c runs it in
 * the LCD's wake-up wait): mounts the store, and if it is SIDOU's or blank, runs recovery and
 * fills the pool to its target with no holds, calling `feed` (the watchdog's) between polls.
 * Nothing if the flash chip is not ours. */
void store_fm1_boot(void (*feed)(void));
/* Main loop, in a pass's play: once a tick (10 ms), or at every key-scan frame (about 1.1 kHz)
 * while store_fm1_active (a window opens after each audio block and lasts STORE_WINDOW_LEAD_US:
 * about a quarter of the blocks meet one): the quiet detector, then store_poll (unless the
 * pass ran a chip preparation slice) and store_file_idle. An idle store does nothing but ask
 * whether maintenance has work, once a second. `sounding`: a UI sound plays
 * (app/ui_sound.h). */
void store_fm1_service(uint32_t now_ms, const live* l, keys_route* kr, int sounding);
/* 1 while the store has work in hand: a write, a job, a hold, the test hook. */
extern uint8_t store_fm1_active;
extern volatile uint32_t store_fm1_window;
/* An explicit SAVE that found the pool short (Task 5): held erases go without quiet, each
 * STORE_FORCE_GAP_MS apart (short gaps the user asked for), until it is off again. */
void store_fm1_force(int on);
/* The device's store. */
store* store_fm1(void);

/* The store's flash operations, in midi.c with the flash driver: interrupts already masked,
 * the store's range only and never a guard band; 0 or the driver's error (< 0). */
int flash_store_prog(uint32_t off, const void* p, uint32_t n);
int flash_store_erase(uint32_t off);
/* Boot, interrupts off: 1 if the flash chip is the one we know (its JEDEC id). */
int flash_store_ok(void);

#endif
