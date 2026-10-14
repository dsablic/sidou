// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The update protocol's flash hooks for ota.c (Felucca's ota_hw.c, adapted): erases and writes
 * only inside the staging area (ota.c OTA_AREA), with interrupts masked, XIP off and only once
 * the audio is stopped; reads in short interrupt-off windows.
 *
 * Static code for one translation unit, like the HAL: midi.c includes it after ota.c, and the
 * host tests (host/crates/sidou-pack/harness/flash_hooks.c) include it with stand-ins. The
 * includer provides: from fm1_flash.h FL_IN, FL_FAR, fl_erase4k_ram, fl_read_ram, fl_write,
 * irq_save and irq_restore; from ota.c OTA_AREA and OTA_AREA_LEN; audio_running (audio.h) and
 * input_leds_dark (input.h). */
#ifndef SIDOU_FM1_OTA_FLASH_H
#define SIDOU_FM1_OTA_FLASH_H

#include <stdint.h>

static uint32_t flash_refused; /* writes refused because the audio ran (a bug if not 0) */

static int ota_in_area(uint32_t off, uint32_t n) {
    return FL_IN(off, n, OTA_AREA, OTA_AREA + OTA_AREA_LEN);
}

/* Erases and writes run with XIP off and interrupts masked (fl_*_ram, .ram_text): the audio
 * must be stopped first (the DMA would loop a stale buffer, and the audio path reads .rodata
 * from flash), and the TIMER5 interrupt (input scan, USB and UART polls) is masked with the
 * rest. A fault while XIP is off cannot reach the fatal stubs, which are in flash: the
 * watchdog resets the chip then. */
static int flash_write_allowed(void) {
    if (audio_running()) {
        flash_refused++;
        return 0;
    }
    return 1;
}

static int ota_erase(uint32_t off) {
    uint32_t took, f;
    int rc;
    if (!ota_in_area(off, 0x1000u) || (off & 0xFFFu)) {
        return -8;
    }
    if (!flash_write_allowed()) {
        return -16;
    }
    f = irq_save();
    input_leds_dark();
    rc = FL_FAR(fl_erase4k_ram)(off, &took);
    irq_restore(f);
    return rc;
}

static int ota_prog(uint32_t off, const void* p, uint32_t n) {
    if (!ota_in_area(off, n)) {
        return -8;
    }
    if (!flash_write_allowed()) {
        return -16;
    }
    return fl_write(off, p, n); /* p is RAM: ota.c's buffers */
}

/* In 256-byte windows with interrupts off (XIP is off meanwhile). */
static int ota_fread(uint32_t off, void* p, uint32_t n) {
    uint8_t* d = p;
    while (n) {
        uint32_t k = n > 256u ? 256u : n, f = irq_save();
        int rc = FL_FAR(fl_read_ram)(off, d, k);
        irq_restore(f);
        if (rc) {
            return rc;
        }
        off += k;
        d += k;
        n -= k;
    }
    return 0;
}

#endif
