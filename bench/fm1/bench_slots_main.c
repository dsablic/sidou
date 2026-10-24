// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The effect-slot scenarios (workload_slots.h): the workload's SID output, then each
 * scenario over part of it, then the switch scenario. The emulator test counts
 * instructions between the bench_mark() calls: SID init, SID render, per scenario its
 * start (with warm-up) and its measured run, then the switch scenario's start and each
 * of its blocks. The stereo outputs follow each other at RESULT_ADDR. */
#include <stdint.h>

#include "sid/sid_osc.h" /* sid_noise_tables_init */
#include "workload.h"
#include "workload_slots.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

static int16_t sid_out[WORKLOAD_SAMPLES];

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    sid_noise_tables_init();
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    int16_t* samples = (int16_t*)(RESULT_ADDR + 0x40u);

    bench_mark();
    workload_begin(BENCH_CHIPS, SID_MODEL_6581);
    bench_mark();
    workload_render_samples(sid_out, WORKLOAD_SAMPLES);
    bench_mark();
    uint32_t n = workload_slots_count();
    for (uint32_t s = 0; s < n; ++s) {
        workload_slots_begin(s, sid_out, WORKLOAD_SLOTS_FRAMES);
        bench_mark();
        workload_slots_process(sid_out + WORKLOAD_SLOTS_FRAMES,
                               samples + 2u * WORKLOAD_SLOTS_FRAMES * s, WORKLOAD_SLOTS_FRAMES);
        bench_mark();
    }

    /* The switch scenario, a mark after every block. */
    int16_t* sw = samples + 2u * WORKLOAD_SLOTS_FRAMES * n;
    workload_slots_switch_begin(sid_out, WORKLOAD_SLOTS_FRAMES);
    bench_mark();
    for (uint32_t b = 0; b < WORKLOAD_SLOTS_SWITCH_BLOCKS; ++b) {
        workload_slots_switch_block(sid_out + WORKLOAD_SLOTS_SWITCH_INPUT(b),
                                    sw + 2u * WORKLOAD_SLOTS_SWITCH_BLOCK * b, b);
        bench_mark();
    }

    result[1] = 2u * WORKLOAD_SLOTS_FRAMES * n +
                2u * WORKLOAD_SLOTS_SWITCH_BLOCK * WORKLOAD_SLOTS_SWITCH_BLOCKS;
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
