// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* A sine for the screen's animations (the logo, the About cracktro): integer only, from a
 * quarter-wave table in flash. Portable C with no hardware access. */
#ifndef SIDOU_SINE_H
#define SIDOU_SINE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* A full turn of phase. */
#define SINE_TURN 256u

/* sin(2 pi phase / SINE_TURN) * 127, rounded; phase wraps. */
int32_t sine_q7(uint32_t phase);
/* round(amp * sin(2 pi phase / SINE_TURN)), for |amp| up to 2^23. */
int32_t sine_scaled(uint32_t phase, int32_t amp);

#ifdef __cplusplus
}
#endif

#endif
