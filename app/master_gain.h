// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MASTER_GAIN_H
#define SIDOU_MASTER_GAIN_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MASTER_UNITY 32768u
#define MASTER_BLOCK_LOG2 7u
#define MASTER_BLOCK (1u << MASTER_BLOCK_LOG2)

uint32_t master_gain_of(uint32_t value, uint32_t max);

void master_gain_apply(int16_t* lr, uint32_t frames_log2, uint32_t from, uint32_t to);

#ifdef __cplusplus
}
#endif

#endif
