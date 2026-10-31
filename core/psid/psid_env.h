/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
/* Derived from FM-1 SID's player (psid.c, fm1-emu), BSD-2-Clause, Joakim Eriksson.
 *
 * Redistribution and use in source and binary forms, with or without modification, are
 * permitted provided that the following conditions are met:
 *  1. Redistributions of source code must retain the above copyright notice, this list of
 *     conditions and the following disclaimer.
 *  2. Redistributions in binary form must reproduce the above copyright notice, this list
 *     of conditions and the following disclaimer in the documentation and/or other
 *     materials provided with the distribution.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
 * EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL
 * THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT
 * OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. */
#ifndef SIDOU_PSID_ENV_H
#define SIDOU_PSID_ENV_H

#include <stdint.h>

#include "cpu6502/cpu6502.h"
#include "psid/psid_file.h"
#include "sid/sid_shadow.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_CLOCK_PAL 985248u
#define PSID_CLOCK_NTSC 1022727u
#define PSID_FRAME_PAL 19656u
#define PSID_FRAME_NTSC 17095u
#define PSID_LINE_PAL 63u
#define PSID_LINE_NTSC 65u
#define PSID_LATCH_PAL 0x4025u
#define PSID_LATCH_NTSC 0x4295u
#define PSID_INIT_SECONDS 5u

enum {
    PSID_RUN_MORE = 0,
    PSID_RUN_INIT_DONE,
    PSID_RUN_CALL_DONE,
    PSID_RUN_IDLE,
    PSID_RUN_INIT_TOO_LONG,
    PSID_RUN_PLAY_HUNG,
    PSID_RUN_JAM,
    PSID_RUN_NOT_STARTED
};

enum { PSID_PHASE_NONE = 0, PSID_PHASE_INIT, PSID_PHASE_PLAY, PSID_PHASE_WAIT, PSID_PHASE_STOP };

typedef void (*psid_sink)(void* ctx, uint32_t cycle, uint32_t chip, uint32_t reg,
                          uint32_t value);

typedef struct psid_env {
    cpu6502 cpu;
    const psid_header* h;
    psid_sink sink;
    void* sink_ctx;
    uint32_t clock;
    uint32_t call_start;
    uint32_t call_due;
    uint32_t next_due;
    uint32_t next_grid;
    uint32_t calls;
    uint32_t overruns;
    uint32_t late;
    uint32_t sid3_dropped;
    uint32_t sid_accesses;
    sid_shadow* shadow[2];
    uint16_t frame;
    uint16_t latch;
    uint16_t song;
    uint8_t line;
    uint8_t phase;
    uint8_t stop;
    uint8_t vic_irq;
    uint8_t cia1_icr;
    uint8_t vic[64];
    uint8_t cia[2][16];
} psid_env;

void psid_env_load(psid_env* e, uint8_t* mem, const psid_header* h, const uint8_t* file);
void psid_env_load_over(psid_env* e, uint8_t* mem, const psid_header* h);

void psid_env_load_part(psid_env* e, uint32_t offset, const uint8_t* bytes, uint32_t n);

void psid_env_set_shadows(psid_env* e, sid_shadow* a, sid_shadow* b);

void psid_env_set_sink(psid_env* e, psid_sink sink, void* ctx);

int psid_env_start(psid_env* e, uint32_t song);

int psid_env_run(psid_env* e, uint32_t cycles);

static inline uint32_t psid_env_now(const psid_env* e) {
    return e->cpu.cycles;
}

static inline uint32_t psid_env_due(const psid_env* e) {
    return e->next_due;
}

uint8_t psid_env_bank(uint32_t addr);

uint32_t psid_env_period(const psid_env* e);

#ifdef __cplusplus
}
#endif

#endif
