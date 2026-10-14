// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The effects scenario: the workload's SID output, then the post-chip chain with every
 * effect at full over it (BENCH_FX_WORST: the worst case of workload_fx.h). The emulator test counts instructions between the four
 * bench_mark() calls (setup, SID render, effects) and reads the samples from
 * RESULT_ADDR. */
#include <stdint.h>

#include "workload.h"
#include "workload_fx.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    int16_t* samples = (int16_t*)(RESULT_ADDR + 0x40u);

    bench_mark();
    workload_begin(BENCH_CHIPS, SID_MODEL_6581);
#ifdef BENCH_FX_WORST
    workload_fx_begin_worst();
#else
    workload_fx_begin();
#endif
    bench_mark();
    workload_render_samples(samples, WORKLOAD_SAMPLES);
    bench_mark();
    workload_fx_process(samples, WORKLOAD_SAMPLES);
    bench_mark();

    result[1] = WORKLOAD_SAMPLES;
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
