// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The firmware's flash hooks for ota.c (platform/fm1/ota_flash.h) on the host: the same code,
 * with the flash driver, the interrupt mask, the audio and the LEDs played by stand-ins that
 * record how they are used. ota.c (platform/fm1/src/ota.c) supplies OTA_AREA and the hooks'
 * declarations. */
#include <stdint.h>
#include <string.h>

#define FELUCCA_ID "FM-1_900"
#define FELUCCA_OTA_DRYRUN 0
#define section(x) aligned(4) /* ota.c's .pool buffers (Mach-O section names) */

/* hal/fm1_flash.h's range check, verbatim (tests/flash_hooks.rs compares the text): the
 * header's driver is pi32v2 code in .ram_text and does not build here. */
#define FL_IN(off, n, lo, hi) ((uint32_t)(off) >= (lo) && (uint32_t)(off) <= (hi) && \
                               (uint32_t)(n) <= (hi) - (uint32_t)(off))
#define FL_FAR(fn) (fn)

#define FLASH_SIZE (1u << 20)
static uint8_t flash[FLASH_SIZE];
static int irqs_off, audio_on;
static uint32_t leds_dark, unmasked_driver_calls, read_windows, oversized_windows, erases, writes;

static uint32_t irq_save(void) {
    irqs_off = 1;
    return 0;
}
static void irq_restore(uint32_t f) {
    (void)f;
    irqs_off = 0;
}
static int fl_erase4k_ram(uint32_t off, uint32_t* took) {
    unmasked_driver_calls += !irqs_off;
    erases++;
    memset(flash + off, 0xFF, 0x1000);
    *took = 0;
    return 0;
}
static int fl_read_ram(uint32_t off, uint8_t* dst, uint32_t n) {
    unmasked_driver_calls += !irqs_off;
    read_windows++;
    oversized_windows += n > 256u;
    memcpy(dst, flash + off, n);
    return 0;
}
/* The real fl_write masks interrupts per 256-byte page itself (fm1_flash.h). */
static int fl_write(uint32_t off, const uint8_t* src, uint32_t n) {
    writes++;
    for (uint32_t i = 0; i < n; i++) {
        flash[off + i] &= src[i];
    }
    return 0;
}
static int audio_running(void) {
    return audio_on;
}
static void input_leds_dark(void) {
    leds_dark++;
}

/* ota.c's other hooks: not used here. */
static int ota_wire_send(const uint8_t* p, uint32_t n) {
    (void)p;
    (void)n;
    return -1;
}
static int ota_frame_get(const uint8_t** p, uint32_t* n) {
    (void)p;
    (void)n;
    return 0;
}
static void ota_frame_done(void) {}
static uint32_t ota_now_ms(void) {
    return 0;
}
static void ota_idle(void) {}
static void ota_show(uint32_t step, int32_t code) {
    (void)step;
    (void)code;
}
static void ota_commit(const uint8_t* parm) {
    (void)parm;
}

#include "ota.c"
#include "ota_flash.h"

typedef struct {
    uint32_t refused, leds_dark, unmasked_driver_calls, read_windows, oversized_windows, erases,
        writes, area, area_len;
} fh_counters;

void fh_reset(int audio);
uint8_t* fh_flash(void);
fh_counters fh_counts(void);
int fh_erase(uint32_t off);
int fh_prog(uint32_t off, const uint8_t* p, uint32_t n);
int fh_fread(uint32_t off, uint8_t* p, uint32_t n);

void fh_reset(int audio) {
    memset(flash, 0x5A, sizeof flash);
    irqs_off = 0;
    audio_on = audio;
    flash_refused = leds_dark = unmasked_driver_calls = read_windows = oversized_windows = 0;
    erases = writes = 0;
}
uint8_t* fh_flash(void) {
    return flash;
}
fh_counters fh_counts(void) {
    fh_counters c = {flash_refused, leds_dark, unmasked_driver_calls, read_windows,
                     oversized_windows, erases, writes, OTA_AREA, OTA_AREA_LEN};
    return c;
}
int fh_erase(uint32_t off) {
    return ota_erase(off);
}
int fh_prog(uint32_t off, const uint8_t* p, uint32_t n) {
    return ota_prog(off, p, n);
}
int fh_fread(uint32_t off, uint8_t* p, uint32_t n) {
    return ota_fread(off, p, n);
}
