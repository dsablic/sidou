// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_live.h"

void fx_live_init(fx_live* f, sid_slots* slots) {
    f->slots = slots;
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot_params off = {0, 0.0f, 0.0f, 0.0f, 0.0f};
        f->p[k] = off;
    }
}

/* The slots' range, 0xA0..0xBF, the register ring's REG_RING_IS_SLOT: clear of both chips'
 * registers and pseudo-registers. */
_Static_assert((LIVE_REG_SLOT_BASE & 0xe0u) == 0xa0u &&
                   (LIVE_REG_SLOT(SID_SLOT_COUNT - 1u, LIVE_SLOT_FIELDS - 1u) & 0xe0u) == 0xa0u,
               "the slots' pseudo-registers are 0xA0..0xBF");

int fx_live_apply(fx_live* f, uint8_t reg, uint8_t value) {
    uint32_t slot = (uint32_t)(reg - LIVE_REG_SLOT_BASE) >> 3;
    uint32_t field = reg & 7u;
    if (reg < LIVE_REG_SLOT_BASE || slot >= SID_SLOT_COUNT || field >= LIVE_SLOT_FIELDS) {
        return 0;
    }
    sid_slot_params* p = &f->p[slot];
    float v = (float)value * (1.0f / (float)LIVE_SLOT_MAX);
    switch (field) {
    case LIVE_SLOT_ALGORITHM: p->algorithm = value; break;
    case LIVE_SLOT_AMOUNT: p->amount = v; break;
    case LIVE_SLOT_A: p->a = v; break;
    case LIVE_SLOT_B: p->b = v; break;
    default: p->mix = v; break;
    }
    sid_slots_set(f->slots, slot, p);
    return 1;
}

void fx_live_pseudo(void* ctx, uint8_t reg, uint8_t value) {
    fx_live_apply(ctx, reg, value);
}

void fx_live_sid_sink(void* ctx, uint8_t reg, uint8_t value) {
    fx_live_target* t = ctx;
    if (!fx_live_apply(t->fx, reg, value)) {
        live_sid_sink(&t->sid, reg, value);
    }
}
