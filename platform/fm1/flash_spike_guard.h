// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The flash spike's bounds (M2a Task 3b, platform/fm1/flash_spike.c): where it may erase and
 * program, how it tells its own sectors from anyone else's, and the guarded calls it makes.
 * Only the spike build (CMake SIDOU_FLASH_SPIKE) includes this; the default image has none of
 * it (emu-gate tests/flash_spike.rs checks the ELF's symbols).
 *
 * The spike works on the store's last two sectors only (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2a-storage.md,
 * the flash map): 0xDE000, where it leaves a pattern for the stock round trip, and 0xDF000,
 * where it measures. Never the flash head [0, 0x4000), the app area [0x4000, 0x93000), the
 * VM region's head [0x93000, 0x97000), the update staging area [0xE0000, 0xE5000) or
 * anything above. Programs also keep out of the store's guard band, bytes 0xF00..0xFFF of
 * each sector: an erased band reads 0xFFFF where an update record has its magic (ota.c), so
 * nothing the spike leaves can look like one to the SPL or the update loader.
 *
 * Static code for one translation unit, like ota_flash.h: flash_spike.c includes it, and the
 * host tests (host/crates/sidou-pack/harness/flash_spike_guard.c) include it with stand-ins.
 * The includer defines the driver calls declared below, each a fm1_flash.h-style RAM function
 * with interrupts masked around it. The macros are plain expressions so the RAM functions use them
 * too (a call from .ram_text into flash code cannot link, and with XIP off cannot run). */
#ifndef SIDOU_FM1_FLASH_SPIKE_GUARD_H
#define SIDOU_FM1_FLASH_SPIKE_GUARD_H

#include <stdint.h>

/* The store's range (the plan's flash map; app/store_layout.h once M2a Task 1 lands). */
#define FLASH_SPIKE_STORE_LO 0x00097000u
#define FLASH_SPIKE_STORE_HI 0x000E0000u
/* What lies around it, never written by the spike. */
#define FLASH_SPIKE_APP_HI 0x00093000u /* the flash head and the app area end here */
#define FLASH_SPIKE_OTA_LO 0x000E0000u /* the update staging area, ota.c OTA_AREA */

#define FLASH_SPIKE_SECTOR 0x1000u
#define FLASH_SPIKE_PAGE 0x100u
/* The store's guard band starts here in every sector: never programmed. */
#define FLASH_SPIKE_SECTOR_USE 0xF00u
/* The pattern for the stock round trip, and the sector the measurements use. */
#define FLASH_SPIKE_PATTERN (FLASH_SPIKE_STORE_HI - 2u * FLASH_SPIKE_SECTOR) /* 0xDE000 */
#define FLASH_SPIKE_MEASURE (FLASH_SPIKE_STORE_HI - FLASH_SPIKE_SECTOR)      /* 0xDF000 */
#define FLASH_SPIKE_LO FLASH_SPIKE_PATTERN
#define FLASH_SPIKE_HI FLASH_SPIKE_STORE_HI

_Static_assert(FLASH_SPIKE_LO >= FLASH_SPIKE_STORE_LO && FLASH_SPIKE_HI <= FLASH_SPIKE_STORE_HI,
               "the spike stays inside the store");
_Static_assert(FLASH_SPIKE_STORE_LO >= FLASH_SPIKE_APP_HI, "the store is above the app area");
_Static_assert(FLASH_SPIKE_HI <= FLASH_SPIKE_OTA_LO, "the spike ends below the staging area");
_Static_assert(FLASH_SPIKE_HI - FLASH_SPIKE_LO == 2u * FLASH_SPIKE_SECTOR, "two sectors");
_Static_assert((FLASH_SPIKE_LO & (FLASH_SPIKE_SECTOR - 1u)) == 0u, "whole sectors");

/* [off, off + n) inside the spike's two sectors, without wrapping (fm1_flash.h's FL_IN). */
#define FLASH_SPIKE_IN(off, n)                                                                 \
    ((uint32_t)(off) >= FLASH_SPIKE_LO && (uint32_t)(off) <= FLASH_SPIKE_HI &&                 \
     (uint32_t)(n) <= FLASH_SPIKE_HI - (uint32_t)(off))
/* A page program: 1..256 bytes inside one page, inside the spike's sectors, below the band. */
#define FLASH_SPIKE_PROG_OK(off, n)                                                            \
    (FLASH_SPIKE_IN(off, n) && (uint32_t)(n) >= 1u && (uint32_t)(n) <= FLASH_SPIKE_PAGE &&     \
     ((uint32_t)(off) & (FLASH_SPIKE_PAGE - 1u)) + (uint32_t)(n) <= FLASH_SPIKE_PAGE &&        \
     ((uint32_t)(off) & (FLASH_SPIKE_SECTOR - 1u)) + (uint32_t)(n) <= FLASH_SPIKE_SECTOR_USE)
/* An erase of a whole sector or a whole page (size), aligned, inside the spike's sectors. */
#define FLASH_SPIKE_ERASE_OK(off, size)                                                        \
    (((uint32_t)(size) == FLASH_SPIKE_SECTOR || (uint32_t)(size) == FLASH_SPIKE_PAGE) &&       \
     ((uint32_t)(off) & ((uint32_t)(size) - 1u)) == 0u && FLASH_SPIKE_IN(off, size))
/* fm1_flash.h's own gate in the spike's translation unit: its sector erase (n = 4096) and
 * page program (n <= 256) may only reach the spike's sectors, programs below the band. */
#define FLASH_SPIKE_DRIVER_OK(off, n)                                                          \
    ((uint32_t)(n) == FLASH_SPIKE_SECTOR ? FLASH_SPIKE_ERASE_OK(off, n)                        \
                                         : FLASH_SPIKE_PROG_OK(off, n))

/* Every sector the spike erases gets this first, at offset 0: the spike's own mark. Not the
 * store's sector magic ("SIDOUSTR", https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2a-storage.md Task 2), so the store
 * sees a spike sector as dirty and erases it. */
#define FLASH_SPIKE_MARK "SIDOU FLASHSPIKE"
#define FLASH_SPIKE_MARK_LEN 16u

/* What a sector holds. */
#define FLASH_SPIKE_BLANK 0   /* every byte 0xFF */
#define FLASH_SPIKE_OURS 1    /* the mark at 0, the band 0xFF: the spike's own */
#define FLASH_SPIKE_FOREIGN 2 /* anything else: not to be touched */

/* The round-trip pattern's state in FLASH_SPIKE_PATTERN. */
#define FLASH_SPIKE_PATTERN_ABSENT 0  /* the sector is blank */
#define FLASH_SPIKE_PATTERN_INTACT 1  /* the mark and every pattern byte as the spike left them */
#define FLASH_SPIKE_PATTERN_DAMAGED 2 /* anything else */

/* The suspend check's result (flash_spike_drv_suspend). */
typedef struct {
    int32_t rc;            /* 0, or the driver's error */
    uint32_t latency_us;   /* from the suspend command to the part not busy */
    uint32_t status;       /* SR1 | SR2 << 8 while suspended (SUS1 is bit 10) */
    uint32_t rest_us;      /* from the resume to the erase's end */
} flash_spike_suspend_result;

/* The includer's driver calls (defined after this header, static). */
static int flash_spike_drv_erase4k(uint32_t off, uint32_t* took_us);
static int flash_spike_drv_page_erase(uint32_t off, uint32_t* took_us);
static int flash_spike_drv_prog(uint32_t off, const uint8_t* src, uint32_t n, uint32_t* took_us);
static int flash_spike_drv_suspend(uint32_t erase_off, uint32_t after_us, uint32_t read_off,
                                   uint8_t* dst, uint32_t n, flash_spike_suspend_result* out);

static uint32_t flash_spike_refused; /* calls the guard refused (a bug if not 0) */

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

/* The pattern's byte at sector offset i (FLASH_SPIKE_MARK_LEN <= i < FLASH_SPIKE_SECTOR_USE):
 * every value, zeros among them, and no two pages alike. */
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

/* The guarded calls: the bounds first, the driver only inside them. took_us: the part's busy
 * time (fm1_flash.h fl_wait_ready). -8: refused, nothing sent to the part. */
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


/* A sector erase of erase_off, suspended after_us, a read of n bytes at read_off into dst
 * while suspended, resumed, waited for. Reads are harmless anywhere, but stay in the spike's
 * sectors too. */
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
