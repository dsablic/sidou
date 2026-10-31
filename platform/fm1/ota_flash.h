// SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-only
#ifndef SIDOU_FM1_OTA_FLASH_H
#define SIDOU_FM1_OTA_FLASH_H

#include <stdint.h>

static uint32_t flash_refused;

static int ota_in_area(uint32_t off, uint32_t n) {
    return FL_IN(off, n, OTA_AREA, OTA_AREA + OTA_AREA_LEN);
}

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
    return fl_write(off, p, n);
}

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
