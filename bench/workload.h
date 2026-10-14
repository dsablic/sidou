// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WORKLOAD_H
#define SIDOU_WORKLOAD_H

#include <stdint.h>

#include "sid/sid_defs.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WORKLOAD_SAMPLE_RATE 44100u
#define WORKLOAD_SAMPLES 11025u
#define WORKLOAD_MAX_CHIPS 2u

/* Chips beyond the first replay the same register writes, except that voice
 * 1's frequency is raised by 0x0400 (about 3.6 semitones) so the mix is not
 * just a copy of chip 0; the cost per chip is unchanged. Chips are mixed in at
 * equal level. The chip count is clamped to [1, WORKLOAD_MAX_CHIPS].
 *
 * The workload lives in global state: these functions are not reentrant and
 * not thread-safe, so callers must serialise them. */
void workload_begin(uint32_t chips, sid_model model);
void workload_render_samples(int16_t* out, uint32_t n);
uint32_t workload_sample_count(void);

#ifdef __cplusplus
}
#endif

#endif
