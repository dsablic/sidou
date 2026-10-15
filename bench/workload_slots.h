// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WORKLOAD_SLOTS_H
#define SIDOU_WORKLOAD_SLOTS_H

/* The effect-slot scenarios of the benchmark (core/fx/fx_slots.h), run over the
 * workload's output. Scenario 0 is the frame alone: all three slots in the path with
 * algorithm NONE. Then each algorithm alone in its slot (the others NONE), and last every
 * slot's heaviest algorithm together. In every scenario all controls alternate between
 * two settings on every block of 128, so the smoothing never settles, the coefficients
 * are recomputed on every sample and the delays' times glide all the time; bit-crush sits
 * at its fastest rate (a capture on every sample).
 *
 * Like the workload these live in global state and are not reentrant or thread-safe. */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define WORKLOAD_SLOTS_FRAMES 4096u
/* The heaviest algorithm per slot, for the last scenario (set from the gate's report). */
#define WORKLOAD_SLOTS_HEAVIEST_DIST SID_DIST_OVERDRIVE
#define WORKLOAD_SLOTS_HEAVIEST_MOD SID_MOD_CHORUS
#define WORKLOAD_SLOTS_HEAVIEST_FX SID_FXSLOT_REVERB_LUSH

/* The switch scenario: every slot's heaviest, then a preset change switching all three
 * slots at once (to fuzz, flanger and the light reverb) at block 0, and back at block
 * SWITCH_BLOCKS / 2, with the controls alternating as above; measured per block. */
#define WORKLOAD_SLOTS_SWITCH_BLOCK 32u
#define WORKLOAD_SLOTS_SWITCH_BLOCKS 512u
/* In the second half the Modulation slot's Mix is 0 (so after its switch it comes to
 * rest), rising again at this block, while the FX slot is in the middle of its switch
 * into the lush reverb: a slot waking from rest starts without waiting its turn, so its
 * start-up lands on top of another slot's switch. */
#define WORKLOAD_SLOTS_SWITCH_WAKE 445u

uint32_t workload_slots_count(void);
/* Starts scenario `s` and runs `warm` (n samples) through it, so the switch-in (pool
 * clear, warm-up, fade) is over before measuring. */
void workload_slots_begin(uint32_t s, const int16_t* warm, uint32_t n);
/* Mono in, interleaved stereo out, in blocks of 128. */
void workload_slots_process(const int16_t* in, int16_t* out, uint32_t frames);
/* The switch scenario: start (on every slot's heaviest, warmed up on `warm`), then one
 * block of SWITCH_BLOCK frames at a time, b counting from 0. Block b's input is
 * WORKLOAD_SLOTS_SWITCH_INPUT(b) samples into the workload's output (wrapping within its
 * first 2 * FRAMES). */
#define WORKLOAD_SLOTS_SWITCH_INPUT(b) \
    (WORKLOAD_SLOTS_FRAMES + (WORKLOAD_SLOTS_SWITCH_BLOCK * (b)) % WORKLOAD_SLOTS_FRAMES)
void workload_slots_switch_begin(const int16_t* warm, uint32_t n);
void workload_slots_switch_block(const int16_t* in, int16_t* out, uint32_t b);

#ifdef __cplusplus
}
#endif

#endif
