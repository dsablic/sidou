// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The live instrument's effect slots (core/fx/fx_slots.h): how their settings travel from
 * live to the slots that render them.
 *
 * live keeps each slot's settings (algorithm, Amount, A, B, Mix; the controls 0..127 as
 * MIDI) and sends every change down its sink as a pseudo-register, LIVE_REG_SLOT(slot,
 * field), value. On the device that is the register ring, so a slot change reaches the
 * audio interrupt in order with the SID writes of the same pass and is applied at the start
 * of a block, like them; reg_ring_drain hands pseudo-registers it does not know to
 * fx_live_apply. On the host fx_live_sid_sink applies everything at once.
 *
 * Portable C with no hardware access. */
#ifndef SIDOU_FX_LIVE_H
#define SIDOU_FX_LIVE_H

#include <stdint.h>

#include "fx/fx_slots.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The slot pseudo-registers: 0xA0 | slot << 3 | field, slot 0..2, field 0..4 (0xA0..0xB4):
 * the base has no bit in slot << 3 | field, and bit 7 marks a pseudo-register. */
#define LIVE_REG_SLOT_BASE 0xA0u
#define LIVE_REG_SLOT(slot, field) (LIVE_REG_SLOT_BASE | ((slot) << 3) | (field))

/* A slot's settings as live sends them, applied to the slots. */
typedef struct {
    sid_slots* slots;
    sid_slot_params p[SID_SLOT_COUNT];
} fx_live;

/* Every slot off (algorithm NONE, every control 0). */
void fx_live_init(fx_live* f, sid_slots* slots);
/* 1 if reg is a slot pseudo-register (its new value goes to the slot), else 0. Called
 * from the audio interrupt on the device. */
int fx_live_apply(fx_live* f, uint8_t reg, uint8_t value);
/* fx_live_apply as a reg_ring_pseudo_fn (ctx: an fx_live*). */
void fx_live_pseudo(void* ctx, uint8_t reg, uint8_t value);

/* The host's sink: SID registers and LIVE_REG_RESET as live_sid_sink, slot registers to
 * fx_live_apply. ctx is an fx_live_target. */
typedef struct {
    live_sid_target sid;
    fx_live* fx;
} fx_live_target;

void fx_live_sid_sink(void* ctx, uint8_t reg, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
