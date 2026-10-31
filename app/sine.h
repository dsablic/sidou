// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SINE_H
#define SIDOU_SINE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SINE_TURN 256u

int32_t sine_q7(uint32_t phase);
int32_t sine_scaled(uint32_t phase, int32_t amp);

#ifdef __cplusplus
}
#endif

#endif
