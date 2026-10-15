// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "workload_slots.h"

#include "fx/fx_slots.h"
#include "workload.h"

#define BLOCK 128u
#define SEED 0x5107u

typedef struct {
    uint32_t dist, mod, fx;
} scenario;

/* Order matters: emu-gate's slots_budget.rs names them in the same order. */
static const scenario SCENARIOS[] = {
    {SID_DIST_NONE, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_TAPE, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_OVERDRIVE, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_FUZZ, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_CRUSH, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_LOFI, SID_MOD_NONE, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_CHORUS, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_FLANGER, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_PHASER, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_TREMOLO, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_AUTOPAN, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_WOW, SID_FXSLOT_NONE},
    {SID_DIST_NONE, SID_MOD_NONE, SID_FXSLOT_DELAY},
    {SID_DIST_NONE, SID_MOD_NONE, SID_FXSLOT_PINGPONG},
    {SID_DIST_NONE, SID_MOD_NONE, SID_FXSLOT_REVERB_LIGHT},
    {SID_DIST_NONE, SID_MOD_NONE, SID_FXSLOT_REVERB_LUSH},
    /* Every slot's heaviest, as measured by the scenarios above (the gate checks). */
    {WORKLOAD_SLOTS_HEAVIEST_DIST, WORKLOAD_SLOTS_HEAVIEST_MOD, WORKLOAD_SLOTS_HEAVIEST_FX},
};

static sid_slot_pool pool;
static sid_slots slots;
static uint32_t current;
static uint32_t blocks;

uint32_t workload_slots_count(void) { return sizeof SCENARIOS / sizeof SCENARIOS[0]; }

static void set_targets(void) {
    const scenario* sc = &SCENARIOS[current];
    const uint32_t alg[SID_SLOT_COUNT] = {sc->dist, sc->mod, sc->fx};
    int hi = (int)(blocks & 1u);
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot_params p = {alg[k], hi ? 1.0f : 0.6f, hi ? 1.0f : 0.6f, hi ? 1.0f : 0.6f,
                             hi ? 1.0f : 0.8f};
        if (k == SID_SLOT_DIST && alg[k] == SID_DIST_CRUSH) {
            p.a = hi ? 0.002f : 0.0f; /* rate: a capture on (nearly) every sample */
        }
        sid_slots_set(&slots, k, &p);
    }
}

static void process(const int16_t* in, int16_t* out, uint32_t n) {
    while (n) {
        uint32_t len = n < BLOCK ? n : BLOCK;
        set_targets();
        ++blocks;
        sid_slots_process(&slots, in, out, len);
        in += len;
        out += 2u * len;
        n -= len;
    }
}

void workload_slots_begin(uint32_t s, const int16_t* warm, uint32_t n) {
    static int16_t scratch[2u * BLOCK];
    current = s < workload_slots_count() ? s : 0;
    blocks = 0;
    sid_slots_init(&slots, WORKLOAD_SAMPLE_RATE, SEED, &pool);
    while (n) {
        uint32_t len = n < BLOCK ? n : BLOCK;
        process(warm, scratch, len);
        warm += len;
        n -= len;
    }
}

void workload_slots_process(const int16_t* in, int16_t* out, uint32_t frames) {
    process(in, out, frames);
}

static const scenario SWITCH_TO = {SID_DIST_FUZZ, SID_MOD_FLANGER, SID_FXSLOT_REVERB_LIGHT};
static scenario switch_now;

static void set_switch_targets(uint32_t b) {
    const uint32_t alg[SID_SLOT_COUNT] = {switch_now.dist, switch_now.mod, switch_now.fx};
    int hi = (int)(b & 1u);
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot_params p = {alg[k], hi ? 1.0f : 0.6f, hi ? 1.0f : 0.6f, hi ? 1.0f : 0.6f,
                             hi ? 1.0f : 0.8f};
        sid_slots_set(&slots, k, &p);
    }
}

void workload_slots_switch_begin(const int16_t* warm, uint32_t n) {
    workload_slots_begin(workload_slots_count() - 1u, warm, n); /* every slot's heaviest */
}

void workload_slots_switch_block(const int16_t* in, int16_t* out, uint32_t b) {
    const scenario* heaviest = &SCENARIOS[workload_slots_count() - 1u];
    switch_now = b < WORKLOAD_SLOTS_SWITCH_BLOCKS / 2u ? SWITCH_TO : *heaviest;
    set_switch_targets(b);
    sid_slots_process(&slots, in, out, WORKLOAD_SLOTS_SWITCH_BLOCK);
}
