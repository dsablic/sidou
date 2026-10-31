// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CPU6502_H
#define SIDOU_CPU6502_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CPU6502_RUN 0
#define CPU6502_RET 1
#define CPU6502_JAM 2

#define CPU6502_KERNAL_IRQ_EXIT 0xEA31u
#define CPU6502_KERNAL_IRQ_EXIT_SHORT 0xEA81u
#define CPU6502_KERNAL_NMI_EXIT 0xFEBCu

typedef uint8_t (*cpu6502_rd)(void* user, uint32_t addr, uint32_t cycle);
typedef void (*cpu6502_wr)(void* user, uint32_t addr, uint32_t value, uint32_t cycle);

typedef struct {
    uint8_t* mem;
    cpu6502_rd rd;
    cpu6502_wr wr;
    void* user;
    uint32_t cycles;
    uint16_t pc;
    uint8_t a, x, y, s, p;
    uint8_t io;
    uint8_t state;
} cpu6502;

int cpu6502_run(cpu6502* c, uint32_t limit, int stops);

void cpu6502_reset_regs(cpu6502* c);

#ifdef __cplusplus
}
#endif

#endif
