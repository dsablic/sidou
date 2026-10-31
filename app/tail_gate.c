// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "tail_gate.h"

#include "master_gain.h"

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static uint32_t tail_busy(const sid* c) {
    return (c->env[0].counter | c->env[1].counter | c->env[2].counter | c->env[0].gate |
            c->env[1].gate | c->env[2].gate) |
           (c->bleed != 0.0f);
}

void tail_gate_run(tail_gate* t, const sid* a, const sid* b, int16_t* s) {
    if (tail_busy(a) | (b ? tail_busy(b) : 0u)) {
        t->n = 0;
        return;
    }
    uint32_t n = t->n;
    if (n < TAIL_HOLD_BLOCKS + 2u) {
        t->n = ++n;
    }
    tail_gate_also(t, s);
}

void tail_gate_also(const tail_gate* t, int16_t* s) {
    uint32_t n = t->n;
    if (n > TAIL_HOLD_BLOCKS) {
        master_gain_apply(s, TAIL_BLOCK_LOG2 - 1u, n == TAIL_HOLD_BLOCKS + 1u ? MASTER_UNITY : 0u,
                          0u);
    }
}
