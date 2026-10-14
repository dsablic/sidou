// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Felucca's ota.c (platform/fm1/usb/ota.c, GPL-3.0-only), step 1 of the update protocol as the
 * firmware runs it, built for the host with test hooks: the host side of the protocol is the
 * test's (callbacks), the flash is a RAM array with NOR semantics (erase to 0xFF, program
 * clears bits), time advances one millisecond per idle call. */
#include <stdint.h>
#include <string.h>

/* The identity the firmware answers (platform/fm1/CMakeLists.txt SIDOU_PRODUCT). */
#define FELUCCA_ID "FM-1_900"
#define FELUCCA_OTA_DRYRUN 0
/* ota.c puts its large buffers in the device's .pool section; Mach-O section names need a
 * segment, so the host build aligns them instead. */
#define section(x) aligned(4)

typedef struct {
    void* ctx;
    int32_t (*send)(void* ctx, const uint8_t* p, uint32_t n);
    int32_t (*frame)(void* ctx, const uint8_t** p, uint32_t* n);
    void (*frame_done)(void* ctx);
    uint8_t* flash;
    uint32_t flash_len;
    uint32_t now;
    int32_t committed;
    uint8_t parm[112];
    uint32_t shown_step;
    int32_t shown_code;
} ota_host;

static ota_host* H;

static int ota_wire_send(const uint8_t* p, uint32_t n) {
    return H->send(H->ctx, p, n);
}
static int ota_frame_get(const uint8_t** p, uint32_t* n) {
    return H->frame(H->ctx, p, n);
}
static void ota_frame_done(void) {
    H->frame_done(H->ctx);
}
static uint32_t ota_now_ms(void) {
    return H->now;
}
static void ota_idle(void) {
    H->now++;
}
static int ota_erase(uint32_t off) {
    if ((off & 0xFFFu) || off > H->flash_len - 0x1000u) {
        return -8;
    }
    memset(H->flash + off, 0xFF, 0x1000);
    return 0;
}
static int ota_prog(uint32_t off, const void* p, uint32_t n) {
    const uint8_t* s = p;
    if (off > H->flash_len || n > H->flash_len - off) {
        return -8;
    }
    for (uint32_t i = 0; i < n; i++) {
        H->flash[off + i] &= s[i];
    }
    return 0;
}
static int ota_fread(uint32_t off, void* p, uint32_t n) {
    if (off > H->flash_len || n > H->flash_len - off) {
        return -1;
    }
    memcpy(p, H->flash + off, n);
    return 0;
}
static void ota_show(uint32_t step, int32_t code) {
    H->shown_step = step;
    H->shown_code = code;
}
static void ota_commit(const uint8_t* parm) {
    memcpy(H->parm, parm, sizeof H->parm);
    H->committed = 1;
}

#include "ota.c"

void ota_h_service(ota_host* h);
int ota_h_session(ota_host* h);
int ota_h_boot_cleanup(ota_host* h);

void ota_h_service(ota_host* h) {
    H = h;
    ota_service();
}

int ota_h_session(ota_host* h) {
    H = h;
    return ota_session();
}

int ota_h_boot_cleanup(ota_host* h) {
    H = h;
    return ota_boot_cleanup();
}
