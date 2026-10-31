// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_TAIL_GATE_H
#define SIDOU_TAIL_GATE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define TAIL_BLOCK_LOG2 7u
#define TAIL_BLOCK (1u << TAIL_BLOCK_LOG2)
#define TAIL_HOLD_BLOCKS 16u

typedef struct {
    uint32_t n;
} tail_gate;

void tail_gate_run(tail_gate* t, const sid* a, const sid* b, int16_t* s);
void tail_gate_also(const tail_gate* t, int16_t* s);

static inline int tail_gate_sounding(const tail_gate* t, const sid* a) {
    return a->env[0].counter != 0u && t->n == 0u;
}

#ifdef __cplusplus
}
#endif

#endif
