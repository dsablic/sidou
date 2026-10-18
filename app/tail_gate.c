// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "tail_gate.h"

#include "master_gain.h"

/* What keeps a chip from being quiet: a counter or a gate up, or a bleed. 0 for a quiet chip.
 * Out of line, so the one copy serves both chips. */
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
    if (n > TAIL_HOLD_BLOCKS) {
        /* The block after the hold ramps to 0, later ones stay there: the mono block as 64
         * frames, whose gain is the same for both samples of a pair. */
        master_gain_apply(s, TAIL_BLOCK_LOG2 - 1u, n == TAIL_HOLD_BLOCKS + 1u ? MASTER_UNITY : 0u,
                          0u);
    }
}
