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
 *   3  mode 1 with the voice shadow (M3 Task 5b; core/sid/sid_shadow.h): each write also goes to a
 *      shadow of a 6581 (sid_init's), at its cycle, and each frame the shadow is clocked to the
 *      frame's end as the look-ahead's idle slices will (a mark pair around that too: 4 marks a
 *      frame, 2 for init). Samples as mode 1.
 *   4  the shadow's worst slices: [1] the model, [2] voice 3's control and [3] the frequency
 *      every voice gets ([4] voices 1 and 2's control), then [5] slices of [6] cycles clocked
 *      one by one (sid_shadow_clock). Marks: two around each slice.
 *   5  psid_ctl's look-ahead slices (M3 Task 8; core/psid/psid_ctl.h): the PSID file at
 *      FILE_ADDR, [1] its length, played from song 1 for [2] main-loop passes, the chips (and so
 *      the shadows) [3] 8580s, else 6581s. After each pass the interrupt's side is played by
 *      taking every entry off the ring and putting the chips' clocks at the 6502's time, so
 *      every pass runs a slice. Marks: two around each pass's psid_ctl_poll; samples: the 6502
 *      cycles each pass ran (-1: none). [6] the player's state at the end, [7] its writes.
 *   6  psid_ctl's interrupt half (M3 Task 9; psid_ctl_block): mode 5's tune and chips for [2]
 *      blocks, before each main-loop passes until the 6502 is ahead (PSID_AHEAD_CYCLES) or the
 *      ring is out of room, at most [4] (the device makes about 30 a block), then the block,
 *      psid_ctl_block rendering it with its writes placed (the emulator test tells the chips'
 *      own work in it from the .sid path's by the functions it runs). Marks: 2 a block. [6] the
 *      writes applied late, [7] the most renders a chip's block took; samples: the writes
 *      pushed, low and high halves, then each block's entries taken.
 * RESULT_ADDR [2] the outcome (mode 0: 1 success; mode 1: 1 all calls returned; mode 2: 1),
 * [3] the 6502 cycles, [4] the sum of the written values, [5] the writes, [6] the plays over
 * their frame, [7] the PC at the end or at the call that did not return. */
#include <stdint.h>

#include "cpu6502/cpu6502.h"
#include "psid/psid_ctl.h"
#include "psid/psid_file.h"
#include "sid/sid.h"
#include "sid/sid_shadow.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */
#define PARAM_ADDR 0x01CB0000u
#define MEM_ADDR 0x01CC0000u /* 64 KiB */
#define FILE_ADDR (PARAM_ADDR + 0x100u) /* mode 5's PSID file, up to MEM_ADDR */

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

static sid chip;
static sid_shadow shadow;

static void hook_wr(void* user, uint32_t addr, uint32_t value, uint32_t cycle) {
    (void)user;
    (void)addr;
    (void)cycle;
    write_sum += value;
    write_count++;
}

/* Mode 3's: the write into the shadow too (mode 1's hook stays as it was, so its count is the
 * 6502's alone). */
static void hook_wr_shadow(void* user, uint32_t addr, uint32_t value, uint32_t cycle) {
    sid_shadow_write((sid_shadow*)user, cycle, addr, value);
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

/* Mode 3: workload with the shadow (a function of its own, so mode 1's code and count stay as
 * they were). */
static void workload_shadowed(volatile uint32_t* result, volatile uint32_t* param,
                              volatile int16_t* out) {
    uint8_t* mem = cpu.mem;
    mem[1] = 0x35;
    cpu.io = 1;
    sid_init(&chip, SID_MODEL_6581, SID_CLOCK_PAL, 44118u);
    sid_shadow_init(&shadow, &chip, cpu.cycles);
    cpu.user = &shadow;
    cpu.wr = hook_wr_shadow;
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
        if (i > 0u) {
            bench_mark();
            sid_shadow_clock(&shadow, (i + 1u) * frame_cycles);
            bench_mark();
        }
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

static void shadow_slices(volatile uint32_t* result, volatile uint32_t* param) {
    sid_init(&chip, param[1] ? SID_MODEL_8580 : SID_MODEL_6581, SID_CLOCK_PAL, 44118u);
    sid_shadow_init(&shadow, &chip, 0);
    for (uint32_t v = 0; v < 3u; v++) {
        uint32_t r = 7u * v;
        sid_shadow_write(&shadow, 0, r + 0u, param[3] & 0xffu);
        sid_shadow_write(&shadow, 0, r + 1u, param[3] >> 8);
        sid_shadow_write(&shadow, 0, r + 2u, 0x00);
        sid_shadow_write(&shadow, 0, r + 3u, 0x08);
        sid_shadow_write(&shadow, 0, r + 5u, 0x00);
        sid_shadow_write(&shadow, 0, r + 6u, 0xf0);
        sid_shadow_write(&shadow, 0, r + 4u, v == 2u ? param[2] : param[4]);
    }
    uint32_t at = 0;
    for (uint32_t k = 0; k < param[5]; k++) {
        at += param[6];
        bench_mark();
        sid_shadow_clock(&shadow, at);
        bench_mark();
    }
    result[1] = 0;
    result[2] = 1;
}

static sid chip_b;
static psid_ctl ctl;
static psid_work work;
static psid_header header;

static sid* bench_chip(void* ctx, uint32_t k) {
    (void)ctx;
    return k ? &chip_b : &chip;
}

static void look_ahead(volatile uint32_t* result, volatile uint32_t* param,
                       volatile int16_t* out) {
    const uint8_t* file = (const uint8_t*)FILE_ADDR;
    sid_model model = param[3] ? SID_MODEL_8580 : SID_MODEL_6581;
    psid_ctl_hooks hooks = {bench_chip, 0, 0};
    result[2] = 0;
    if (psid_parse(file, param[1], &header) != PSID_OK) {
        return;
    }
    sid_init(&chip, model, SID_CLOCK_PAL, 44118u);
    sid_init(&chip_b, model, SID_CLOCK_PAL, 44118u);
    psid_ctl_init(&ctl, &work, (uint8_t*)MEM_ADDR, &hooks);
    if (!psid_ctl_load(&ctl, &header, file) || !psid_ctl_play(&ctl, 1)) {
        return;
    }
    uint32_t passes = param[2];
    for (uint32_t i = 0; i < passes; i++) {
        uint32_t before = psid_env_now(&work.env);
        bench_mark();
        int ran = psid_ctl_poll(&ctl);
        bench_mark();
        uint32_t now = psid_env_now(&work.env);
        out[i] = ran ? (int16_t)(now - before) : (int16_t)-1;
        ctl.tail = ctl.head;
        ctl.clock[0] = now;
        ctl.clock[1] = now;
    }
    result[1] = passes;
    result[2] = 1;
    result[6] = ctl.state;
    result[7] = ctl.writes;
}

static int16_t block_a[PSID_BLOCK], block_b[PSID_BLOCK];

/* The interrupt's render pointer: the plain render (the device's tapped one renders the same
 * samples; the tap's cost is TAP_PER_SAMPLE's). */
static void bench_render(sid* s, int16_t* out, uint32_t n, int tapped) {
    (void)tapped;
    sid_render(s, out, n);
}

static void isr_blocks(volatile uint32_t* result, volatile uint32_t* param,
                       volatile int16_t* out) {
    const uint8_t* file = (const uint8_t*)FILE_ADDR;
    sid_model model = param[3] ? SID_MODEL_8580 : SID_MODEL_6581;
    psid_ctl_hooks hooks = {bench_chip, 0, 0};
    result[2] = 0;
    if (psid_parse(file, param[1], &header) != PSID_OK) {
        return;
    }
    sid_init(&chip, model, SID_CLOCK_PAL, 44118u);
    sid_init(&chip_b, model, SID_CLOCK_PAL, 44118u);
    psid_ctl_init(&ctl, &work, (uint8_t*)MEM_ADDR, &hooks);
    if (!psid_ctl_load(&ctl, &header, file) || !psid_ctl_play(&ctl, 1)) {
        return;
    }
    uint32_t blocks = param[2], passes = param[4];
    for (uint32_t i = 0; i < blocks; i++) {
        for (uint32_t k = 0; k < passes && psid_ctl_poll(&ctl); k++) {
        }
        uint32_t tail = ctl.tail;
        bench_mark();
        (void)psid_ctl_block(&ctl, &chip, &chip_b, block_a, block_b, 0, bench_render);
        bench_mark();
        out[2u + i] = (int16_t)(ctl.tail - tail);
    }
    out[0] = (int16_t)(ctl.writes & 0xffffu);
    out[1] = (int16_t)(ctl.writes >> 16);
    result[1] = 2u + blocks;
    result[2] = 1;
    result[6] = ctl.late;
    result[7] = ctl.splits_max;
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
    if (param[0] >= 3u) {
        sid_noise_tables_init(); /* the voices' noise tables (sid.c), as the firmware's boot */
    }
    switch (param[0]) {
    case 0:
        dormann(result);
        break;
    case 1:
        workload(result, param, out);
        break;
    case 3:
        workload_shadowed(result, param, out);
        break;
    case 4:
        shadow_slices(result, param);
        break;
    case 5:
        look_ahead(result, param, out);
        break;
    case 6:
        isr_blocks(result, param, out);
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
