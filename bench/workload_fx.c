// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "workload_fx.h"

#include "fx/fx.h"
#include "workload.h"

#define BLOCK 128u
#define SETTLE_BLOCKS 64u /* 8192 samples, past the smoothing's snap (about 4,100) */
#define SEED 0x51D0u

static sid_fx fx;

void workload_fx_begin(void) {
    static const sid_fx_params full = {1.0f, 1.0f, 1.0f, 1.0f, 1.0f, 1.0f};
    int16_t silence[BLOCK];
    sid_fx_init(&fx, WORKLOAD_SAMPLE_RATE, SEED);
    sid_fx_set(&fx, &full);
    for (uint32_t b = 0; b < SETTLE_BLOCKS; ++b) {
        for (uint32_t i = 0; i < BLOCK; ++i) {
            silence[i] = 0;
        }
        sid_fx_process(&fx, silence, BLOCK);
    }
}

void workload_fx_process(int16_t* io, uint32_t n) {
    while (n) {
        uint32_t len = n < BLOCK ? n : BLOCK;
        sid_fx_process(&fx, io, len);
        io += len;
        n -= len;
    }
}
