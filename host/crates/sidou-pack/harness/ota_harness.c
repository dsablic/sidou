// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>
#include <string.h>

#define FELUCCA_ID "FM-1_900"
#define FELUCCA_OTA_DRYRUN 0
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

static uint32_t ldr_writes;
static int ldr_fread(uint32_t off, void* p, uint32_t n);
static int ldr_erase(uint32_t off);
static int ldr_prog(uint32_t off, const void* p, uint32_t n);
static void ldr_record_clear(void);
static void ldr_progress(uint32_t done, uint32_t total);
#include "../loader/ldr_core.c"
static int ldr_fread(uint32_t off, void* p, uint32_t n) {
    return ota_fread(off, p, n);
}
static int ldr_erase(uint32_t off) {
    ldr_writes++;
    return ota_erase(off);
}
static int ldr_prog(uint32_t off, const void* p, uint32_t n) {
    ldr_writes++;
    return ota_prog(off, p, n);
}
static void ldr_record_clear(void) {
    ldr_writes++;
}
static void ldr_progress(uint32_t done, uint32_t total) {
    (void)done;
    (void)total;
}

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

int ldr_h_session(ota_host* h) {
    H = h;
    ldr_writes = 0;
    return ldr_session();
}

uint32_t ldr_h_writes(void) {
    return ldr_writes;
}
