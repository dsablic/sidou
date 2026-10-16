// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The pixel scene's benchmark (bench/workload_scene.h) in fm1-emu: the emulator test counts
 * the instructions between the bench_mark() calls the workload makes around each step, and
 * compares the log it leaves at RESULT_ADDR with the host build's. */
#include <stdint.h>

#include "workload_scene.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

static workload_scene scene;

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    int16_t* log = (int16_t*)(RESULT_ADDR + 0x40u);
    result[1] = workload_scene_run(&scene, bench_mark, log);
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
