// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's 6502 on the device build (M3 Task 2): core/cpu6502 as the firmware builds it
 * (CPU6502_SMALL, -Os, the I/O page's accesses out of line), run on fm1-emu to count the
 * pi32v2 instructions a 6502 cycle costs. The emulator test (emu-gate tests/cpu6502_budget.rs)
 * writes the 64 KiB the 6502 sees at MEM_ADDR and the parameters at PARAM_ADDR, then reads the
 * result words and samples.
 *
 * PARAM_ADDR [0] the mode:
 *   0  Dormann's functional test (the image at MEM_ADDR, started at $0400, no I/O page) to its
 *      success trap at $3469, in chunks of 1,000 cycles. Marks: 2 (the start, the end).
 *   1  a GoatTracker player: [1] its init, [2] its play address, [3] the subtune, [4] the
 *      frames, [5] the cycles of a frame. $01 is $35 and $D000-$DFFF goes to the write hook,
 *      which sums the values and counts the writes. Init, then each frame's play at the frame's
 *      first cycle (the count jumps over the idle time). Marks: two around each call (2 *
 *      (1 + frames)); samples: the cycles each call took (init first).
 *   2  one instruction of each opcode, alone: a run with the limit already reached (executes
 *      nothing: the call's own cost) and a run with the limit one cycle on (executes one
 *      instruction). Marks: 4 a opcode and 4 more a I/O-page access (LDA, STA and INC of
 *      $D400, through the hooks); samples: the cycles the one instruction took.
 * RESULT_ADDR [2] the outcome (mode 0: 1 success; mode 1: 1 all calls returned; mode 2: 1),
 * [3] the 6502 cycles, [4] the sum of the written values, [5] the writes, [6] the plays over
 * their frame, [7] the PC at the end or at the call that did not return. */
#include <stdint.h>

#include "cpu6502/cpu6502.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */
#define PARAM_ADDR 0x01CB0000u
#define MEM_ADDR 0x01CC0000u /* 64 KiB */

#define DORMANN_SUCCESS 0x3469u
#define DORMANN_CAP 150000000u
#define CALL_CAP 30000u /* a call's cycles must fit a sample (an int16) */

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

static uint32_t write_sum, write_count;

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

static uint8_t hook_rd(void* user, uint32_t addr, uint32_t cycle) {
    (void)user;
    (void)addr;
    (void)cycle;
    return 0;
}

static void hook_wr(void* user, uint32_t addr, uint32_t value, uint32_t cycle) {
    (void)user;
    (void)addr;
    (void)cycle;
    write_sum += value;
    write_count++;
}

static cpu6502 cpu;

/* Calls the routine at `pc` with A as given, as a PSID player's call does: the stack holds
 * $FFFF (RTS goes to $0000, a stop), the registers are a call's. */
static int call(uint32_t pc, uint32_t a) {
    uint8_t* mem = cpu.mem;
    mem[0x1ff] = 0xFF;
    mem[0x1fe] = 0xFF;
    cpu6502_reset_regs(&cpu);
    cpu.pc = (uint16_t)pc;
    cpu.a = (uint8_t)a;
    cpu.s = 0xFD;
    return cpu6502_run(&cpu, cpu.cycles + CALL_CAP, 1);
}

static void dormann(volatile uint32_t* result) {
    cpu.io = 0;
    cpu6502_reset_regs(&cpu);
    cpu.pc = 0x0400;
    uint32_t last = 0xFFFFFFFFu, same = 0, ok = 0;
    bench_mark();
    while (cpu.cycles < DORMANN_CAP) {
        cpu6502_run(&cpu, cpu.cycles + 1000u, 0);
        if (cpu.pc == DORMANN_SUCCESS) {
            ok = 1;
            break;
        }
        if (cpu.state == CPU6502_JAM || (cpu.pc == last && ++same >= 2u)) {
            break;
        }
        if (cpu.pc != last) {
            same = 0;
        }
        last = cpu.pc;
    }
    bench_mark();
    result[2] = ok;
}

static void workload(volatile uint32_t* result, volatile uint32_t* param, volatile int16_t* out) {
    uint8_t* mem = cpu.mem;
    mem[1] = 0x35;
    cpu.io = 1;
    uint32_t frames = param[4], frame_cycles = param[5], over = 0, ok = 1;
    for (uint32_t i = 0; i <= frames && ok; i++) {
        uint32_t routine = i == 0u ? param[1] : param[2];
        if (i > 0u) {
            uint32_t start = i * frame_cycles;
            if ((int32_t)(cpu.cycles - start) < 0) {
                cpu.cycles = start; /* the idle time */
            } else {
                over++;
            }
        }
        uint32_t before = cpu.cycles;
        bench_mark();
        int state = call(routine, i == 0u ? param[3] : 0u);
        bench_mark();
        out[i] = (int16_t)(cpu.cycles - before);
        if (state != CPU6502_RET) {
            ok = 0;
            result[7] = cpu.pc;
        }
    }
    result[1] = frames + 1u;
    result[2] = ok;
    result[6] = over;
}

static void opcodes(volatile uint32_t* result, volatile int16_t* out) {
    uint8_t* mem = cpu.mem;
    cpu.io = 0;
    for (uint32_t op = 0; op < 259u; op++) {
        for (uint32_t i = 0; i < 0x40u; i += 2u) { /* every pointer in the zero page: $0200 */
            mem[i] = 0x00;
            mem[i + 1u] = 0x02;
        }
        mem[0x1fe] = 0;
        mem[0x1ff] = 0;
        mem[0x200] = 0;
        mem[0x210] = 0;
        mem[0x211] = 0;
        uint32_t code = op;
        if (op >= 256u) { /* the I/O page: LDA, STA, INC $D400 */
            static const uint8_t io_ops[3] = {0xAD, 0x8D, 0xEE};
            code = io_ops[op - 256u];
            mem[1] = 0x35;
            cpu.io = 1;
            mem[0x1002] = 0xD4;
            mem[0x1001] = 0x00;
        } else {
            mem[0x1001] = 0x10;
            mem[0x1002] = 0x02;
        }
        mem[0x1000] = (uint8_t)code;
        cpu6502_reset_regs(&cpu);
        cpu.pc = 0x1000;
        cpu.s = 0xFD;
        uint32_t c0 = cpu.cycles;
        bench_mark();
        cpu6502_run(&cpu, cpu.cycles, 0);
        bench_mark();
        bench_mark();
        cpu6502_run(&cpu, cpu.cycles + 1u, 0);
        bench_mark();
        out[op] = (int16_t)(cpu.cycles - c0);
        cpu.io = 0;
        mem[1] = 0;
    }
    result[1] = 259u;
    result[2] = 1;
}

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    volatile uint32_t* param = (volatile uint32_t*)PARAM_ADDR;
    volatile int16_t* out = (volatile int16_t*)(RESULT_ADDR + 0x40u);
    cpu.mem = (uint8_t*)MEM_ADDR;
    cpu.rd = hook_rd;
    cpu.wr = hook_wr;
    cpu.user = 0;
    result[1] = 0;
    switch (param[0]) {
    case 0:
        dormann(result);
        break;
    case 1:
        workload(result, param, out);
        break;
    default:
        opcodes(result, out);
        break;
    }
    result[3] = cpu.cycles;
    result[4] = write_sum;
    result[5] = write_count;
    if (param[0] == 0u) {
        result[7] = cpu.pc;
    }
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
