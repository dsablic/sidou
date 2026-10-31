// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_STORE_FM1_H
#define SIDOU_FM1_STORE_FM1_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "store.h"

#define STORE_PROG16_MAX_US 3000u
#define STORE_PROG64_MAX_US 3000u
#define STORE_PROG256_MAX_US 3000u
#define STORE_ERASE_MAX_US 20000u
#define STORE_DRIVER_US 100u

#define STORE_WINDOW_US 1000u
#define STORE_WINDOW_LEAD_US 250u
#define STORE_WINDOW_ENTRY_US 1u
#define STORE_HOLD_US 35000u
#define STORE_HOLD_GAP_MS 10u
#define STORE_FORCE_GAP_MS 100u
#ifndef STORE_SYSEX_GAP_MS
#define STORE_SYSEX_GAP_MS 250u
#endif
_Static_assert(STORE_SYSEX_GAP_MS >= 10u && STORE_SYSEX_GAP_MS <= 2000u,
               "SIDOU_SYSEX_GAP_MS is 10..2000: past SysEx's two-second timeout a sender gives up");
#define STORE_PROGRAM_GAP_MS 1u
#define STORE_QUIET_MS 500u

#define STORE_WINDOW_BYTES                                                                      \
    (STORE_PROG256_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US  ? 256u                          \
     : STORE_PROG64_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US ? 64u                           \
     : STORE_PROG16_MAX_US + STORE_DRIVER_US <= STORE_WINDOW_US ? 16u                           \
                                                                : 0u)
_Static_assert(STORE_ERASE_MAX_US <= 30000u,
               "the sector erase does not fit a hold: build the decision rule's next erase mode");
_Static_assert(STORE_ERASE_MAX_US + STORE_DRIVER_US <= STORE_HOLD_US, "a held erase fits");
_Static_assert(STORE_PROG256_MAX_US + STORE_DRIVER_US <= STORE_HOLD_US, "a held program fits");

#define STORE_BLOCK_US 2901u
#define STORE_FILL_WORST_US 1440u
_Static_assert(STORE_WINDOW_LEAD_US + STORE_WINDOW_US < STORE_BLOCK_US - STORE_FILL_WORST_US,
               "a window never delays the next block's interrupt");

typedef struct {
    uint32_t progs, erases;
    uint32_t window_later;
    uint32_t hold_later;
    uint32_t holds;
    uint32_t errors;
} store_fm1_stats;

enum {
    STORE_FM1_PUT = 1,
    STORE_FM1_GET,
    STORE_FM1_FILE,
    STORE_FM1_COLLECT,
    STORE_FM1_SONG,
    STORE_FM1_FORCE,
    STORE_FM1_KEEP,
};
#define STORE_FM1_DATA 16u
typedef struct {
    volatile uint32_t op;
    uint32_t key;
    uint32_t len;
    int32_t result;
    uint8_t data[STORE_FM1_DATA];
} store_fm1_test;

void store_fm1_boot(void (*feed)(void));
void store_fm1_service(uint32_t now_ms, const live* l, keys_route* kr, int sounding);
extern uint8_t store_fm1_active;
extern volatile uint32_t store_fm1_window;
void store_fm1_force(int on);
store* store_fm1(void);

int flash_store_prog(uint32_t off, const void* p, uint32_t n);
int flash_store_erase(uint32_t off);
int flash_store_ok(void);

#endif
