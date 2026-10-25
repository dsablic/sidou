// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_SHADOW_H
#define SIDOU_SID_SHADOW_H

/* The voice shadow (M3 Task 5b, https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md): a copy of a chip's three
 * voices that the main loop clocks with a tune's 6502, so that the tune's reads of the SID are
 * real. The 6502 runs up to two frames ahead of the audio chip (the look-ahead), which therefore
 * cannot answer them; the shadow sees the same writes at the same cycles and answers at the
 * read's cycle:
 * - $1B, OSC3, and $1C, ENV3, as sid_read computes them on the chip (sid_voices.h says on
 *   which cycles OSC3 is not exact; with no waveform selected it reads 0, where reSIDfp holds its
 *   floating DAC's last value);
 * - $19 and $1A, the paddles, $FF (nothing connected);
 * - every other register, the data bus: the last value written or read, as reSIDfp keeps it
 *   (SID.cpp). Each write and each read of $19-$1C leaves its value with a fresh time to live,
 *   SID_SHADOW_TTL_6581 or _8580 cycles; a read of any other register halves what is left of it
 *   (a read discharges the bus faster); once it runs out, the bus reads 0. A time to live halved
 *   to 0 (from 1) never runs out, as in reSIDfp, until the next write or read of $19-$1C.
 * The shadow has no filter and no output stage: a write to $15-$18 only sets the bus.
 *
 * The voices run sid.c's own code (sid_voices.h), in steps of at most SID_SHADOW_STEP cycles, so
 * with the same writes at the same cycles they reach the chip's state exactly, however each side
 * splits its cycles (the chip a sample at a time, sid_advance to a write), except with hard sync:
 * a synced voice's own noise clocks and MSB rise earlier in a step are not recomputed
 * (sid_clock's note in sid.h), so with sync on the shadow and the chip can differ in what the
 * synced voice did before the restart.
 *
 * Cycles are a 32-bit running count (the tune's, psid_env), compared with wrapping arithmetic: an
 * access at a cycle the shadow has passed clocks nothing. Main context only, from flash on the
 * device (core/CMakeLists.txt); nothing here runs in the audio interrupt. 11,382 bytes of code
 * (-Os: a second instance of the voices' code, the chip's being in SRAM) and 64 of tables; the
 * struct is 704 bytes on the device (tests/sid_shadow.rs ties it). The cost, measured on fm1-emu
 * (emu-gate tests/cpu6502_budget.rs): about 45,000 instructions a PAL frame for a GoatTracker
 * tune (under 1 % of the core), and up to about 26 an idle cycle while every voice runs a
 * combined waveform with noise cycle by cycle. */

#include <stdint.h>

#include "sid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* reSIDfp's bus value lifetimes (SID.cpp, BUS_TTL_6581 and BUS_TTL_8580), in chip cycles. */
#define SID_SHADOW_TTL_6581 0x1d00u
#define SID_SHADOW_TTL_8580 0xa2000u

/* The longest step the shadow clocks its voices in (the voices' step allows 1 to 256). Between
 * accesses a tune leaves the SID alone for most of a frame, so the shadow covers a frame's idle
 * cycles in about 154 steps rather than the chip's 880 samples. While a voice has SYNC set it
 * steps at most SID_SHADOW_SYNC_STEP, sid_clock's step, never coarser than the chip's own render
 * at any PAL rate up to 61.5 kHz: hard sync's stepping difference (above) is then no larger than
 * the chip's own against reSIDfp (tests/sid_shadow.rs measures both). */
#define SID_SHADOW_STEP 128u
#define SID_SHADOW_SYNC_STEP 16u

typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_vflags vf;
    uint8_t bus;      /* the data bus's value */
    sid_model model;
    uint32_t now;     /* the cycle the voices have been clocked to */
    uint32_t bus_ttl; /* cycles until the bus reads 0; 0: it holds (above) */
    uint32_t ttl;     /* the model's lifetime: SID_SHADOW_TTL_6581 or _8580 */
    /* The chip's own noise write-back table, when its voices use one (the character's
     * write-back, sid_character.h): copied, so the shadow never points into a chip that a model
     * change may reuse for another setup. */
    uint32_t noise_wb[2][16];
} sid_shadow;

/* Starts the shadow from the chip's voices as they are, at cycle now: its oscillators,
 * envelopes, step flags, model and combined-waveform profile. The bus starts empty (0, as after
 * reSIDfp's reset). Call it where the chip's voices are known: before a tune's first write, the
 * chip not rendering (or between renders, in the context that renders it). */
void sid_shadow_init(sid_shadow* sh, const sid* chip, uint32_t now);

/* Clocks the voices to cycle x (nothing if x is not after now). The look-ahead's idle slices
 * call it too, so that no access has a whole frame to catch up. */
void sid_shadow_clock(sid_shadow* sh, uint32_t x);

/* A write at cycle x: the voices clocked to x, then the value on the bus and, for $00-$14,
 * applied to the voices as sid_write applies it (OSC3 latched first for a write to voice 2 or
 * 3). reg is taken modulo 32. */
void sid_shadow_write(sid_shadow* sh, uint32_t x, uint32_t reg, uint32_t value);

/* A read at cycle x: the voices clocked to x, then the register's value (above). reg is taken
 * modulo 32. */
uint32_t sid_shadow_read(sid_shadow* sh, uint32_t x, uint32_t reg);

#ifdef __cplusplus
}
#endif

#endif
