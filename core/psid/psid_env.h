/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
/* The PSID environment (M3 Tasks 6a and 6b; docs/reference/psid.md, "The environment"): a
 * tune's 64 KiB, its init and play calls on SIDOU's 6502, the bank register, the period, the
 * limits, and time, run in slices of 6502 cycles so the main loop can interleave it with
 * everything else.
 *
 * The design follows FM-1 SID's player (psid.c, fm1-emu, BSD-2-Clause, Joakim Eriksson): the
 * calls through a sentinel return address with a cycle limit, the CIA timer's period, the
 * bank register at $01. SIDOU's version sets the bank register per call from the call's
 * address, runs init in slices instead of one blocking call, honours CIA periods under 1,000
 * cycles, takes the speed bit as the format says (psid_speed_bit), plays NTSC tunes at the
 * NTSC clock and frame, schedules a late call as a C64's pending interrupt does, and stops a
 * call that hangs instead of cutting it off at two frames.
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
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 *
 * Time is the 6502's running cycle count (cpu.cycles): 0 is init's first cycle, and every
 * comparison is wrapping ((int32_t)(a - b)). A call starts at its due cycle; between calls the
 * 6502 idles (time passes, nothing runs). One caller, the main loop; portable C11, no
 * allocation.
 *
 * The I/O page (docs/reference/psid.md, "The I/O page") while $01 maps it in: SID writes go to
 * the sink and to the chip's voice shadow, SID reads are the shadow's, at the access's cycle
 * (sid_shadow.h); the VIC and the CIAs keep what is written to them, and answer the raster
 * line, CIA 1 timer A's counter and the interrupt flags from the absolute cycle. */
#ifndef SIDOU_PSID_ENV_H
#define SIDOU_PSID_ENV_H

#include <stdint.h>

#include "cpu6502/cpu6502.h"
#include "psid/psid_file.h"
#include "sid/sid_shadow.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The clocks and frames (docs/reference/psid.md, "The clock", "The period"). */
#define PSID_CLOCK_PAL 985248u
#define PSID_CLOCK_NTSC 1022727u
#define PSID_FRAME_PAL 19656u  /* 312 lines of 63 cycles */
#define PSID_FRAME_NTSC 17095u /* 263 lines of 65 */
#define PSID_LINE_PAL 63u
#define PSID_LINE_NTSC 65u
#define PSID_LATCH_PAL 0x4025u /* CIA 1 timer A as the KERNAL sets it */
#define PSID_LATCH_NTSC 0x4295u
#define PSID_INIT_SECONDS 5u /* init's limit; a play call's is one second */

/* psid_env_run's results. The first four go on; from PSID_RUN_INIT_TOO_LONG on, the tune has
 * stopped and every later run returns the same code until the next psid_env_start. */
enum {
    PSID_RUN_MORE = 0,      /* the budget ran out inside a call */
    PSID_RUN_INIT_DONE,     /* init returned; the first play call is due (psid_env_due) */
    PSID_RUN_CALL_DONE,     /* a play call returned; the next is due */
    PSID_RUN_IDLE,          /* the budget ran out between calls, before the next one's due */
    PSID_RUN_INIT_TOO_LONG, /* init ran PSID_INIT_SECONDS without returning: INIT TOO LONG */
    PSID_RUN_PLAY_HUNG,     /* a play call ran a second without returning: PLAY HUNG */
    PSID_RUN_JAM,           /* a JAM opcode, at psid_env.cpu.pc: 6502 JAM AT $xxxx */
    PSID_RUN_NOT_STARTED    /* nothing loaded, or no psid_env_start since the load */
};

/* Where the 6502 is (psid_env.phase). */
enum { PSID_PHASE_NONE = 0, PSID_PHASE_INIT, PSID_PHASE_PLAY, PSID_PHASE_WAIT, PSID_PHASE_STOP };

/* A SID write: its cycle on the running count, the chip (0 A, 1 B), the register (0-31) and
 * the value. */
typedef void (*psid_sink)(void* ctx, uint32_t cycle, uint32_t chip, uint32_t reg,
                          uint32_t value);

typedef struct psid_env {
    cpu6502 cpu;          /* cpu.mem is the tune's 64 KiB; cpu.cycles is the time */
    const psid_header* h; /* the caller's, kept as long as the tune is loaded */
    psid_sink sink;
    void* sink_ctx;
    uint32_t clock;      /* the tune's clock in Hz: PSID_CLOCK_PAL or _NTSC */
    uint32_t call_start; /* the current call's first cycle */
    uint32_t call_due;   /* the current (or last) play call's due cycle: its interrupt */
    uint32_t next_due;   /* PSID_PHASE_WAIT: the cycle the next play call starts at */
    uint32_t next_grid;  /* and its interrupt's: next_due, or earlier for a late call */
    uint32_t calls;      /* play calls started since psid_env_start */
    uint32_t overruns;   /* play calls that ran past their period */
    uint32_t late;       /* SID writes the player applied late (psid_ctl counts them here) */
    uint32_t sid3_dropped; /* a third SID's writes, dropped */
    sid_shadow* shadow[2]; /* chip A's and B's voice shadows (psid_env_set_shadows) */
    uint16_t frame;        /* PSID_FRAME_PAL or _NTSC */
    uint16_t latch;        /* CIA 1 timer A's latch */
    uint16_t song;         /* 1..songs */
    uint8_t line;          /* cycles a raster line: PSID_LINE_PAL or _NTSC */
    uint8_t phase;         /* PSID_PHASE_* */
    uint8_t stop;          /* PSID_PHASE_STOP: the PSID_RUN_* code it stopped with */
    uint8_t vic_irq;       /* $D019: $81 from a VBI play call's start until written */
    uint8_t cia1_icr;      /* $DC0D: $81 from a CIA play call's start until read */
    uint8_t vic[64];       /* the VIC's registers, as last written */
    uint8_t cia[2][16];    /* CIA 1's and 2's, as last written */
} psid_env;

/* Sets the environment up for the tune h (parsed and accepted by psid_parse) over mem, its
 * 64 KiB: the clock and frame from h's clock bits, mem zeroed, $00/$01 the C64's power-on
 * $2F/$37, and the data copied to its load address from `file` (the whole file, as psid_parse
 * read it) unless `file` is NULL, when psid_env_load_part brings it in. Everything in *e is
 * reset, the sink included (set it after loading). The tune waits for psid_env_start. */
void psid_env_load(psid_env* e, uint8_t* mem, const psid_header* h, const uint8_t* file);

/* Copies the part of the file's bytes [offset, offset + n) that is the tune's data to its
 * place in memory; bytes outside the data (the header, a load-address prefix, a v5 song-length
 * table) are skipped. For a file read in pieces (Task 10). */
void psid_env_load_part(psid_env* e, uint32_t offset, const uint8_t* bytes, uint32_t n);

/* The voice shadows that answer a tune's SID reads: chip A's, and B's for a 2SID tune (NULL:
 * reads return 0). The caller owns them (Task 8), initialises them from the chips at cycle 0
 * after psid_env_start, and keeps them as long as the tune is loaded; every SID write also
 * goes to its shadow, at its cycle. Set them after psid_env_load, which clears them. */
void psid_env_set_shadows(psid_env* e, sid_shadow* a, sid_shadow* b);

/* Where SID writes go (NULL: nowhere). */
void psid_env_set_sink(psid_env* e, psid_sink sink, void* ctx);

/* Prepares init's call for `song` (1..songs; 0 is the header's start song) at cycle 0, with
 * CIA 1's latch at the KERNAL's value and the counters cleared. Memory is left as it is: to
 * restart a tune from scratch (as a C64 player does on a subtune change), load it again
 * first. Returns 0, or -1 for a song out of range or nothing loaded (nothing changes). */
int psid_env_start(psid_env* e, uint32_t song);

/* Runs at most `cycles` of 6502 time: of the current call, or of the wait for the next one
 * (which then starts and runs within the same budget). Whole instructions only, so time can
 * pass the budget by up to an instruction's cycles. Stops early when a call returns. Returns a
 * PSID_RUN_* code. */
int psid_env_run(psid_env* e, uint32_t cycles);

/* The 6502's time: cycles since init's first. */
static inline uint32_t psid_env_now(const psid_env* e) {
    return e->cpu.cycles;
}

/* PSID_PHASE_WAIT: the cycle the next play call starts at. */
static inline uint32_t psid_env_due(const psid_env* e) {
    return e->next_due;
}

/* The bank register ($01) a call to `addr` starts with, as the format asks: $37 below $A000,
 * $36 up to $CFFF, $34 for $D000-$DFFF, $35 from $E000. */
uint8_t psid_env_bank(uint32_t addr);

/* The current period in cycles: the frame, or CIA 1 timer A's latch + 1 for a CIA song. */
uint32_t psid_env_period(const psid_env* e);

#ifdef __cplusplus
}
#endif

#endif
