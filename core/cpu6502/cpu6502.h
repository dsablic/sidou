// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's 6502 (M3 Task 1; decision record 0008): an NMOS 6510 for PSID tunes, stepped an
 * instruction at a time with exact cycle counts (page crossings and taken branches included).
 *
 * All 256 opcodes: the documented ones, NMOS decimal mode, and the undocumented ones as Tom
 * Harte's SingleStepTests define them (the unstable stores SHA, SHX, SHY and TAS store
 * `value & (high byte of the base + 1)`, and a page crossing puts that value in the address's
 * high byte; ANE and LXA use the magic constant 0xEE). The twelve JAM opcodes stop the CPU
 * with the PC on the opcode, so a call again stops again.
 *
 * The 64 KiB memory is the caller's. While `io` is set and the 6510's port at $01 maps I/O in
 * (bit 2 and either of bits 0-1), reads and writes of $D000-$DFFF go to the hooks instead.
 * Each hook gets the access's cycle on the running count: a load's or a store's last cycle;
 * a read-modify-write reads on its third last and writes twice, the old value on its second
 * last and the new on its last, as the real CPU does. Fetches, pointers and the stack are
 * never I/O and go to memory.
 *
 * `cycles` is the tune's running count: the core adds to it and never resets it, and the
 * limit is compared with wrapping arithmetic, so a count that wraps past 2^32 keeps working.
 *
 * Variants: CPU6502_SMALL (the device's, built at -Os) calls out of line for an access to
 * the I/O page, one compare inline; without it the accesses are inline (larger, about 7 %
 * fewer instructions). CPU6502_HOOK_ALL is for the tests only: every operand access, the
 * zero page and the stack's pushes included, goes through the hooks with the same stamps,
 * whatever the address and $01, so Tom Harte's tests can check each one's cycle.
 *
 * Portable C11, no allocation; the device runs it from flash in the main loop. */
#ifndef SIDOU_CPU6502_H
#define SIDOU_CPU6502_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* cpu6502_run's results and cpu6502.state */
#define CPU6502_RUN 0 /* the limit was reached; call again to continue */
#define CPU6502_RET 1 /* a return to the sentinel ($0000/$0001) or a KERNAL interrupt exit */
#define CPU6502_JAM 2 /* a JAM opcode: the PC is on it */

/* The KERNAL's interrupt exits a PSID play routine may end in (with `stops`). */
#define CPU6502_KERNAL_IRQ_EXIT 0xEA31u
#define CPU6502_KERNAL_IRQ_EXIT_SHORT 0xEA81u
#define CPU6502_KERNAL_NMI_EXIT 0xFEBCu

typedef uint8_t (*cpu6502_rd)(void* user, uint32_t addr, uint32_t cycle);
typedef void (*cpu6502_wr)(void* user, uint32_t addr, uint32_t value, uint32_t cycle);

typedef struct {
    uint8_t* mem;    /* 64 KiB, the caller's */
    cpu6502_rd rd;   /* the I/O page's reads */
    cpu6502_wr wr;   /* the I/O page's writes */
    void* user;      /* the hooks' first argument */
    uint32_t cycles; /* the running count; never reset by the core */
    uint16_t pc;
    uint8_t a, x, y, s, p; /* p: bits 4 and 5 are not flags; the core keeps 5 set */
    uint8_t io;            /* 1: $D000-$DFFF goes to the hooks while $01 maps I/O in */
    uint8_t state;         /* the last run's result: CPU6502_RUN, _RET or _JAM */
} cpu6502;

/* Runs whole instructions until the running count reaches `limit` ((int32_t)(cycles - limit)
 * >= 0), a JAM, or, when `stops` is set, a jump, return or RTI to $0000-$0001 (the sentinel
 * a call pushes) or a KERNAL interrupt exit. Returns the state, which it also stores. */
int cpu6502_run(cpu6502* c, uint32_t limit, int stops);

/* The registers a call starts from: A, X and Y 0, S $FF, P $24 (interrupts off), state RUN.
 * The memory, the PC, the hooks, `io` and the count are left alone. */
void cpu6502_reset_regs(cpu6502* c);

#ifdef __cplusplus
}
#endif

#endif
