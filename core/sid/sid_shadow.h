// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_SHADOW_H
#define SIDOU_SID_SHADOW_H

#include <stdint.h>

#include "sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_SHADOW_TTL_6581 0x1d00u
#define SID_SHADOW_TTL_8580 0xa2000u

#define SID_SHADOW_STEP 128u
#define SID_SHADOW_SYNC_STEP 16u

typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_vflags vf;
    uint8_t bus;
    sid_model model;
    uint32_t now;
    uint32_t bus_ttl;
    uint32_t ttl;
    uint32_t noise_wb[2][16];
} sid_shadow;

void sid_shadow_init(sid_shadow* sh, const sid* chip, uint32_t now);

void sid_shadow_clock(sid_shadow* sh, uint32_t x);

void sid_shadow_write(sid_shadow* sh, uint32_t x, uint32_t reg, uint32_t value);

uint32_t sid_shadow_read(sid_shadow* sh, uint32_t x, uint32_t reg);

#ifdef __cplusplus
}
#endif

#endif
