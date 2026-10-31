// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>
#include <string.h>

#include "flash_spike_guard.h"

#define FLASH_SIZE (1u << 20)
static uint8_t flash[FLASH_SIZE];
static uint32_t drv_calls, drv_lo = 0xFFFFFFFFu, drv_hi;

static void reached(uint32_t off, uint32_t n) {
    drv_calls++;
    if (off < drv_lo) {
        drv_lo = off;
    }
    if (off + n > drv_hi) {
        drv_hi = off + n;
    }
}

static int flash_spike_drv_erase4k(uint32_t off, uint32_t* took_us) {
    reached(off, FLASH_SPIKE_SECTOR);
    for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        flash[(off + i) & (FLASH_SIZE - 1u)] = 0xFF;
    }
    *took_us = 0;
    return 0;
}

static int flash_spike_drv_page_erase(uint32_t off, uint32_t* took_us) {
    reached(off, FLASH_SPIKE_PAGE);
    for (uint32_t i = 0; i < FLASH_SPIKE_PAGE; i++) {
        flash[(off + i) & (FLASH_SIZE - 1u)] = 0xFF;
    }
    *took_us = 0;
    return 0;
}

static int flash_spike_drv_prog(uint32_t off, const uint8_t* src, uint32_t n,
                                uint32_t* took_us) {
    reached(off, n);
    for (uint32_t i = 0; i < n; i++) {
        flash[(off + i) & (FLASH_SIZE - 1u)] &= src[i];
    }
    *took_us = 0;
    return 0;
}

static int flash_spike_drv_suspend(uint32_t erase_off, uint32_t after_us, uint32_t read_off,
                                   uint8_t* dst, uint32_t n, flash_spike_suspend_result* out) {
    (void)after_us;
    reached(erase_off, FLASH_SPIKE_SECTOR);
    memcpy(dst, flash + (read_off & (FLASH_SIZE - 1u)), n);
    for (uint32_t i = 0; i < FLASH_SPIKE_SECTOR; i++) {
        flash[(erase_off + i) & (FLASH_SIZE - 1u)] = 0xFF;
    }
    out->rc = 0;
    return 0;
}

typedef struct {
    uint32_t calls, lo, hi, refused;
    uint32_t spike_lo, spike_hi, pattern, measure, sector_use, store_lo, store_hi;
} fs_counters;

void fs_reset(uint8_t fill);
uint8_t* fs_flash(void);
fs_counters fs_counts(void);
int fs_erase(uint32_t off);
int fs_page_erase(uint32_t off);
int fs_prog(uint32_t off, const uint8_t* p, uint32_t n);
int fs_suspend(uint32_t erase_off, uint32_t read_off, uint8_t* dst, uint32_t n);
int fs_driver_ok(uint32_t off, uint32_t n);
int fs_classify(const uint8_t* sector);
int fs_pattern_state(const uint8_t* sector);
uint8_t fs_pattern_byte(uint32_t i);
const char* fs_mark(void);

void fs_reset(uint8_t fill) {
    memset(flash, fill, sizeof flash);
    drv_calls = drv_hi = 0;
    drv_lo = 0xFFFFFFFFu;
    flash_spike_refused = 0;
}
uint8_t* fs_flash(void) {
    return flash;
}
fs_counters fs_counts(void) {
    fs_counters c = {drv_calls,           drv_lo,          drv_hi,
                     flash_spike_refused, FLASH_SPIKE_LO,  FLASH_SPIKE_HI,
                     FLASH_SPIKE_PATTERN, FLASH_SPIKE_MEASURE, FLASH_SPIKE_SECTOR_USE,
                     FLASH_SPIKE_STORE_LO, FLASH_SPIKE_STORE_HI};
    return c;
}
int fs_erase(uint32_t off) {
    uint32_t took;
    return flash_spike_erase(off, &took);
}
int fs_page_erase(uint32_t off) {
    uint32_t took;
    return flash_spike_page_erase(off, &took);
}
int fs_prog(uint32_t off, const uint8_t* p, uint32_t n) {
    uint32_t took;
    return flash_spike_prog(off, p, n, &took);
}
int fs_suspend(uint32_t erase_off, uint32_t read_off, uint8_t* dst, uint32_t n) {
    flash_spike_suspend_result r;
    return flash_spike_suspend(erase_off, 1000, read_off, dst, n, &r);
}
int fs_driver_ok(uint32_t off, uint32_t n) {
    return FLASH_SPIKE_DRIVER_OK(off, n);
}
int fs_classify(const uint8_t* sector) {
    return flash_spike_classify(sector);
}
int fs_pattern_state(const uint8_t* sector) {
    return flash_spike_pattern_state(sector);
}
uint8_t fs_pattern_byte(uint32_t i) {
    return flash_spike_pattern_byte(i);
}
const char* fs_mark(void) {
    return FLASH_SPIKE_MARK;
}
