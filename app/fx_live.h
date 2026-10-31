// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_LIVE_H
#define SIDOU_FX_LIVE_H

#include <stdint.h>

#include "fx/fx_slots.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LIVE_REG_SLOT_BASE 0xA0u
#define LIVE_REG_SLOT(slot, field) (LIVE_REG_SLOT_BASE | ((slot) << 3) | (field))

typedef struct {
    sid_slots* slots;
    sid_slot_params p[SID_SLOT_COUNT];
} fx_live;

void fx_live_init(fx_live* f, sid_slots* slots);
int fx_live_apply(fx_live* f, uint8_t reg, uint8_t value);
void fx_live_pseudo(void* ctx, uint8_t reg, uint8_t value);

typedef struct {
    live_sid_target sid;
    fx_live* fx;
} fx_live_target;

void fx_live_sid_sink(void* ctx, uint8_t reg, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
