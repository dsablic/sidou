// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_REG_RING_H
#define SIDOU_REG_RING_H

#include <stdint.h>

#include "sid/sid.h"
#include "sid/sid_character.h"

#ifdef __cplusplus
extern "C" {
#endif

#define REG_RING_SIZE 1024u
#define REG_RING_LIVE_PASS 391u
#define REG_RING_SONG_PASS 28u
#define REG_RING_MOVE_PASS 30u
#define REG_RING_WORST_PASS (REG_RING_LIVE_PASS + REG_RING_SONG_PASS + REG_RING_MOVE_PASS)

#define REG_RING_CHIPS 2u
#define REG_RING_CHIP_B 0x40u
#define REG_RING_CHIP_OF(reg) (((reg) >> 6) & 1u)
#define REG_RING_BARE(reg) ((uint8_t)((reg) & ~REG_RING_CHIP_B))
#define REG_RING_IS_SLOT(reg) (((reg) & 0xe0u) == 0xa0u)

typedef struct {
    uint8_t reg;
    uint8_t value;
} reg_ring_entry;

typedef struct {
    reg_ring_entry e[REG_RING_SIZE];
    volatile uint32_t head;
    volatile uint32_t tail;
    uint32_t stage;
} reg_ring;

void reg_ring_init(reg_ring* r);
int reg_ring_push(reg_ring* r, uint8_t reg, uint8_t value);
void reg_ring_commit(reg_ring* r);
uint32_t reg_ring_used(const reg_ring* r);
int reg_ring_pop(reg_ring* r, reg_ring_entry* out);

#define REG_RING_CHARACTER 0x88u
#define REG_RING_NO_STAGE 0xffu

#define REG_RING_MUTE 0x89u

typedef void (*reg_ring_pseudo_fn)(void* ctx, uint8_t reg, uint8_t value);
typedef void (*reg_ring_swap_fn)(void* ctx, uint32_t chip, sid* s);

#define REG_RING_SIDS (REG_RING_CHIPS + 1u)
typedef struct {
    sid chip[REG_RING_SIDS];
    volatile uint8_t active[REG_RING_CHIPS];
    volatile uint8_t spare_ready;
    volatile uint8_t spare_model;
    volatile uint8_t spare_for;
    volatile uint32_t lost_resets;
    uint32_t clock_hz;
    uint32_t sample_rate;
    reg_ring_pseudo_fn pseudo;
    void* pseudo_ctx;
    reg_ring_swap_fn swapped;
    void* swapped_ctx;
    sid_character_ramp ramp[REG_RING_CHIPS];
    uint8_t next[REG_RING_CHIPS];
    volatile uint8_t stage_pending[REG_RING_CHIPS][2];
    volatile uint32_t ticks[REG_RING_CHIPS];
    sid_character_stage stage[REG_RING_CHIPS][2];
    sid_dac dac[REG_RING_SIDS];
    sid_character_prep prep;
    uint8_t prep_on;
    uint8_t prep_profile;
    uint8_t prep_chip;
    uint8_t prep_pass;
    uint8_t build_pass;
    uint8_t keys;
    uint8_t prep_wait[REG_RING_CHIPS];
    uint8_t prep_age[REG_RING_CHIPS];
    uint8_t default_8580_spent;
    uint8_t filter_leases;
} reg_ring_chips;

void reg_ring_chips_init(reg_ring_chips* c, sid_model model, uint32_t clock_hz,
                         uint32_t sample_rate);
sid* reg_ring_chip(reg_ring_chips* c);
sid* reg_ring_chip_of(reg_ring_chips* c, uint32_t chip);
void reg_ring_power_on(sid* s);

uint32_t reg_ring_drain(reg_ring* r, reg_ring_chips* c);

typedef struct {
    reg_ring* ring;
    reg_ring_chips* chips;
    uint8_t chip;
    void (*wait)(void* ctx);
    void* wait_ctx;
    uint32_t full_waits;
    uint32_t reset_waits;
    uint32_t character_defers;
    uint32_t slices;
    uint32_t unprepared_resets;
    uint8_t sliced;
} reg_ring_out;

void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value);

#define REG_RING_PREPARE_SLICE 40000u

int reg_ring_prepare_step(void* ctx, uint8_t profile);
void reg_ring_pass_end(reg_ring_chips* c);
void reg_ring_set_keys(reg_ring_chips* c, uint32_t chip);
#define REG_RING_YIELD_TICKS 16u

#ifdef SIDOU_REG_RING_RACE_HOOK
void reg_ring_set_race_hook(void (*hook)(void* ctx), void* ctx);
#endif

#ifdef __cplusplus
}
#endif

#endif
