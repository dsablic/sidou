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

uint32_t workload_slots_count(void);
/* Starts scenario `s` and runs `warm` (n samples) through it, so the switch-in (pool
 * clear, warm-up, fade) is over before measuring. */
void workload_slots_begin(uint32_t s, const int16_t* warm, uint32_t n);
/* Mono in, interleaved stereo out, in blocks of 128. */
void workload_slots_process(const int16_t* in, int16_t* out, uint32_t frames);

#ifdef __cplusplus
}
#endif

#endif
