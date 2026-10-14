// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "workload_fx.h"

#include "fx/fx.h"
#include "workload.h"

#define BLOCK 128u
#define SETTLE_BLOCKS 64u /* 8192 samples, past the smoothing's snap (about 4,100) */
#define SEED 0x51D0u

/* Field order: tape_drive, wow, flutter, noise, crt, crush. */
static const sid_fx_params FULL = {1.0f, 1.0f, 1.0f, 1.0f, 1.0f, 1.0f};
/* Gaps of 0.4 (and 0.001 for crush) shrink by a quarter per block of 128 at the 10 ms
 * smoothing: never within the 1e-4 snap. */
static const sid_fx_params WORST[2] = {
    {0.6f, 0.6f, 0.6f, 0.6f, 0.6f, 0.001f},
    {1.0f, 1.0f, 1.0f, 1.0f, 1.0f, 0.002f},
};

static sid_fx fx;
static int worst;
static uint32_t blocks;

static void process_block(int16_t* io, uint32_t len) {
    if (worst) {
        sid_fx_set(&fx, &WORST[blocks & 1u]);
    }
    ++blocks;
    sid_fx_process(&fx, io, len);
}

static void begin(int worst_case) {
    int16_t silence[BLOCK];
    worst = worst_case;
    blocks = 0;
    sid_fx_init(&fx, WORKLOAD_SAMPLE_RATE, SEED);
    sid_fx_set(&fx, &FULL);
    for (uint32_t b = 0; b < SETTLE_BLOCKS; ++b) {
        for (uint32_t i = 0; i < BLOCK; ++i) {
            silence[i] = 0;
        }
        process_block(silence, BLOCK);
    }
}

void workload_fx_begin(void) { begin(0); }

void workload_fx_begin_worst(void) { begin(1); }

void workload_fx_process(int16_t* io, uint32_t n) {
    while (n) {
        uint32_t len = n < BLOCK ? n : BLOCK;
        process_block(io, len);
        io += len;
        n -= len;
    }
}
