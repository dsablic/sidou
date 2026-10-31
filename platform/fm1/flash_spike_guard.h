// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_FLASH_SPIKE_GUARD_H
#define SIDOU_FM1_FLASH_SPIKE_GUARD_H

#include <stdint.h>

#define FLASH_SPIKE_STORE_LO 0x00097000u
#define FLASH_SPIKE_STORE_HI 0x000E0000u
#define FLASH_SPIKE_APP_HI 0x00093000u
#define FLASH_SPIKE_OTA_LO 0x000E0000u

#define FLASH_SPIKE_SECTOR 0x1000u
#define FLASH_SPIKE_PAGE 0x100u
#define FLASH_SPIKE_SECTOR_USE 0xF00u
#define FLASH_SPIKE_PATTERN (FLASH_SPIKE_STORE_HI - 2u * FLASH_SPIKE_SECTOR)
#define FLASH_SPIKE_MEASURE (FLASH_SPIKE_STORE_HI - FLASH_SPIKE_SECTOR)
#define FLASH_SPIKE_LO FLASH_SPIKE_PATTERN
#define FLASH_SPIKE_HI FLASH_SPIKE_STORE_HI

_Static_assert(FLASH_SPIKE_LO >= FLASH_SPIKE_STORE_LO && FLASH_SPIKE_HI <= FLASH_SPIKE_STORE_HI,
               "the spike stays inside the store");
_Static_assert(FLASH_SPIKE_STORE_LO >= FLASH_SPIKE_APP_HI, "the store is above the app area");
_Static_assert(FLASH_SPIKE_HI <= FLASH_SPIKE_OTA_LO, "the spike ends below the staging area");
_Static_assert(FLASH_SPIKE_HI - FLASH_SPIKE_LO == 2u * FLASH_SPIKE_SECTOR, "two sectors");
_Static_assert((FLASH_SPIKE_LO & (FLASH_SPIKE_SECTOR - 1u)) == 0u, "whole sectors");

#define FLASH_SPIKE_IN(off, n)                                                                 \
    ((uint32_t)(off) >= FLASH_SPIKE_LO && (uint32_t)(off) <= FLASH_SPIKE_HI &&                 \
     (uint32_t)(n) <= FLASH_SPIKE_HI - (uint32_t)(off))
#define FLASH_SPIKE_PROG_OK(off, n)                                                            \
    (FLASH_SPIKE_IN(off, n) && (uint32_t)(n) >= 1u && (uint32_t)(n) <= FLASH_SPIKE_PAGE &&     \
     ((uint32_t)(off) & (FLASH_SPIKE_PAGE - 1u)) + (uint32_t)(n) <= FLASH_SPIKE_PAGE &&        \
     ((uint32_t)(off) & (FLASH_SPIKE_SECTOR - 1u)) + (uint32_t)(n) <= FLASH_SPIKE_SECTOR_USE)
#define FLASH_SPIKE_ERASE_OK(off, size)                                                        \
    (((uint32_t)(size) == FLASH_SPIKE_SECTOR || (uint32_t)(size) == FLASH_SPIKE_PAGE) &&       \
     ((uint32_t)(off) & ((uint32_t)(size) - 1u)) == 0u && FLASH_SPIKE_IN(off, size))
#define FLASH_SPIKE_DRIVER_OK(off, n)                                                          \
    ((uint32_t)(n) == FLASH_SPIKE_SECTOR ? FLASH_SPIKE_ERASE_OK(off, n)                        \
                                         : FLASH_SPIKE_PROG_OK(off, n))

#define FLASH_SPIKE_MARK "SIDOU FLASHSPIKE"
#define FLASH_SPIKE_MARK_LEN 16u

#define FLASH_SPIKE_BLANK 0
#define FLASH_SPIKE_OURS 1
#define FLASH_SPIKE_FOREIGN 2

#define FLASH_SPIKE_PATTERN_ABSENT 0
#define FLASH_SPIKE_PATTERN_INTACT 1
#define FLASH_SPIKE_PATTERN_DAMAGED 2

typedef struct {
    int32_t rc;
    uint32_t latency_us;
    uint32_t status;
    uint32_t rest_us;
} flash_spike_suspend_result;

static int flash_spike_drv_erase4k(uint32_t off, uint32_t* took_us);
static int flash_spike_drv_page_erase(uint32_t off, uint32_t* took_us);
static int flash_spike_drv_prog(uint32_t off, const uint8_t* src, uint32_t n, uint32_t* took_us);
static int flash_spike_drv_suspend(uint32_t erase_off, uint32_t after_us, uint32_t read_off,
                                   uint8_t* dst, uint32_t n, flash_spike_suspend_result* out);

static uint32_t flash_spike_refused;

static int flash_spike_classify(const uint8_t* sector) {
    static const char mark[] = FLASH_SPIKE_MARK;
    uint32_t i, blank = 1, ours = 1;
    for (i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        blank &= sector[i] == 0xFFu;
        if (i < FLASH_SPIKE_MARK_LEN) {
            ours &= sector[i] == (uint8_t)mark[i];
        } else if (i >= FLASH_SPIKE_SECTOR_USE) {
            ours &= sector[i] == 0xFFu;
        }
    }
    return blank ? FLASH_SPIKE_BLANK : ours ? FLASH_SPIKE_OURS : FLASH_SPIKE_FOREIGN;
}

static uint8_t flash_spike_pattern_byte(uint32_t i) {
    return (uint8_t)((i * 37u) ^ (i >> 8) ^ 0xA5u);
}

static int flash_spike_pattern_state(const uint8_t* sector) {
    static const char mark[] = FLASH_SPIKE_MARK;
    uint32_t i;
    int c = flash_spike_classify(sector);
    if (c == FLASH_SPIKE_BLANK) {
        return FLASH_SPIKE_PATTERN_ABSENT;
    }
    for (i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        uint8_t want = i < FLASH_SPIKE_MARK_LEN       ? (uint8_t)mark[i]
                       : i < FLASH_SPIKE_SECTOR_USE   ? flash_spike_pattern_byte(i)
                                                      : 0xFFu;
        if (sector[i] != want) {
            return FLASH_SPIKE_PATTERN_DAMAGED;
        }
    }
    return FLASH_SPIKE_PATTERN_INTACT;
}

static int flash_spike_erase(uint32_t off, uint32_t* took_us) {
    if (!FLASH_SPIKE_ERASE_OK(off, FLASH_SPIKE_SECTOR)) {
        flash_spike_refused++;
        return -8;
    }
    return flash_spike_drv_erase4k(off, took_us);
}

static int flash_spike_page_erase(uint32_t off, uint32_t* took_us) {
    if (!FLASH_SPIKE_ERASE_OK(off, FLASH_SPIKE_PAGE)) {
        flash_spike_refused++;
        return -8;
    }
    return flash_spike_drv_page_erase(off, took_us);
}

static int flash_spike_prog(uint32_t off, const uint8_t* src, uint32_t n, uint32_t* took_us) {
    if (!FLASH_SPIKE_PROG_OK(off, n)) {
        flash_spike_refused++;
        return -8;
    }
    return flash_spike_drv_prog(off, src, n, took_us);
}

static int flash_spike_suspend(uint32_t erase_off, uint32_t after_us, uint32_t read_off,
                               uint8_t* dst, uint32_t n, flash_spike_suspend_result* out) {
    if (!FLASH_SPIKE_ERASE_OK(erase_off, FLASH_SPIKE_SECTOR) || !FLASH_SPIKE_IN(read_off, n) ||
        n > FLASH_SPIKE_PAGE) {
        flash_spike_refused++;
        out->rc = -8;
        return -8;
    }
    return flash_spike_drv_suspend(erase_off, after_us, read_off, dst, n, out);
}

#endif
