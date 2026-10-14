// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WORKLOAD_FX_H
#define SIDOU_WORKLOAD_FX_H

/* The effects scenario of the benchmark: the post-chip chain with every effect at full,
 * run over the workload's output. Like the workload it lives in global state and is not
 * reentrant or thread-safe. */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initialises the chain at WORKLOAD_SAMPLE_RATE, turns every effect to full and runs
 * silence through it until the smoothing has settled, so workload_fx_process measures
 * the steady all-on cost. */
void workload_fx_begin(void);
/* Runs the chain over `io` in place, in blocks of 128 like the firmware. */
void workload_fx_process(int16_t* io, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
