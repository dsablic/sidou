// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_CPU2_H
#define SIDOU_FM1_CPU2_H

#include <stdint.h>

#include "audio.h"
#include "scope.h"
#include "sid/sid.h"

#if defined(SIDOU_CPU2)
#include "views.h"

#define CPU2_GAVE_UP_SELF_TEST 0x10u
#define CPU2_SELF_TEST_BLOCKS 100u

typedef struct {
    uint32_t jobs;
    uint32_t fault_cpu, fault_pc, fault_dbg, fault_emu;
    uint32_t con_at_boot;
    const char* why;
    uint8_t on;
    uint8_t faulted;
} cpu2_core1;
void cpu2_core1_state(cpu2_core1* out);
void cpu2_core1_unbar(void);
void cpu2_meter_block(uint32_t ticks);
extern const views_cores cpu2_cores;

typedef void (*cpu2_job)(uint32_t arg);
int cpu2_core1_run(cpu2_job job, uint32_t arg);
int cpu2_core1_wait(void);
void cpu2_core1_retire(uint32_t why);

void cpu2_self_test(sid pair[2]);
int cpu2_chip_b_away(sid* b, int16_t* out, scope* tap);
void cpu2_chip_b_back(int away, sid* b, int16_t* out, scope* tap);
#else
static inline int cpu2_chip_b_away(sid* b, int16_t* out, scope* tap) {
    (void)b;
    (void)out;
    (void)tap;
    return 0;
}

static inline void cpu2_chip_b_back(int away, sid* b, int16_t* out, scope* tap) {
    (void)away;
    if (tap) {
        scope_render(tap, b, out, AUDIO_HALF_FRAMES);
    } else {
        sid_render(b, out, AUDIO_HALF_FRAMES);
    }
}
#endif

#endif
