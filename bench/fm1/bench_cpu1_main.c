// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>

#include "fm1_cpu1.h"
#include "sid/sid_osc.h"
#include "workload.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u
#define SAMPLES_OFFSET 0x40u

enum {
    WORD_MAGIC,
    WORD_SAMPLES,
    WORD_SCENARIO,
    WORD_MODEL,
    WORD_CORES,
    WORD_REBOOTS,
    WORD_STARTED,
    WORD_GAVE_UP,
    WORD_JOBS,
    WORD_REJECTED,
    WORD_BOOT,
    WORD_ON,
};

#define BENCH_BUILD 0x43505531u
#define BOOTS_MAGIC 0x424f4f54u

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

struct {
    uint32_t magic, count;
} bench_boots __attribute__((section(".noinit")));

typedef struct {
    sid* chip;
    int16_t* out;
    uint32_t len;
} chip_b_block;

static chip_b_block chip_b_job __attribute__((aligned(32)));

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

static void fm1_fault(const fm1_crash_t* c) {
    (void)c;
    fm1_reboot();
}

static void render_chip_b(uint32_t job) {
    const chip_b_block* b = (const chip_b_block*)(uintptr_t)job;
    sid_render(b->chip, b->out, b->len);
}

__attribute__((noinline)) static int chip_b_hand_over(sid* chip, int16_t* out, uint32_t len) {
    chip_b_job.chip = chip;
    chip_b_job.out = out;
    chip_b_job.len = len;
    return fm1_cpu1_run(render_chip_b, (uint32_t)(uintptr_t)&chip_b_job);
}

__attribute__((noinline)) static int chip_b_join(void) { return fm1_cpu1_wait(); }

static const workload_second_core CORE_1 = {chip_b_hand_over, chip_b_join};

static void ram_init(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
}

static uint32_t boot_count(void) {
    if (bench_boots.magic != BOOTS_MAGIC) {
        bench_boots.magic = BOOTS_MAGIC;
        bench_boots.count = 0;
    }
    return ++bench_boots.count;
}

int main(void) {
    fm1_cpu1_stop();
    ram_init();
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    int16_t* samples = (int16_t*)(RESULT_ADDR + SAMPLES_OFFSET);
    result[WORD_MAGIC] = 0;
    uint32_t boot = boot_count();
    fm1_time_init();
    fm1_irq_init();
    sid_noise_tables_init();
    fm1_cpu1_bar_build(BENCH_BUILD);
    uint32_t two_cores = result[WORD_CORES] == 2u;
    int started = two_cores ? fm1_cpu1_start() : -1;
    sid_model model = result[WORD_MODEL] ? SID_MODEL_8580 : SID_MODEL_6581;

    bench_mark();
    workload_begin_scenario(2, model, result[WORD_SCENARIO]);
    bench_mark();
    if (two_cores) {
        workload_render_two_cores(samples, WORKLOAD_SAMPLES, &CORE_1);
    } else {
        workload_render_samples(samples, WORKLOAD_SAMPLES);
    }
    bench_mark();

    result[WORD_SAMPLES] = WORKLOAD_SAMPLES;
    result[WORD_STARTED] = (uint32_t)started;
    result[WORD_GAVE_UP] = fm1_cpu1_gave_up;
    result[WORD_JOBS] = fm1_cpu1_mb.jobs;
    result[WORD_REJECTED] = fm1_cpu1_mb.rejected;
    result[WORD_BOOT] = boot;
    result[WORD_ON] = fm1_cpu1_on;
    result[WORD_MAGIC] = RESULT_MAGIC;
    if (boot <= result[WORD_REBOOTS]) {
        fm1_reboot();
    }
    for (;;) {
    }
}
