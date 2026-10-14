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

/* Chips beyond the first replay the same register writes and are mixed in at
 * equal level. */
void workload_begin(uint32_t chips, sid_model model);
void workload_render_samples(int16_t* out, uint32_t n);
uint32_t workload_sample_count(void);

#ifdef __cplusplus
}
#endif

#endif
