// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The tail after notes ends in silence (M2 hardware fix; platform/fm1/audio.c, before the effect
 * slots).
 *
 * With every envelope at 0 and its gate off a SID chip still puts out the waveform's leak through
 * the voice DACs' envelope zero (core/sid/sid_dac.c): a faint tone, about -50 dBFS, which the real
 * chip has, reSIDfp renders, and never decays. The chips are left exactly as they are (every
 * reSIDfp comparison holds); this stage mutes the block they produce instead: once the chip (or
 * both, with chip B rendering) has had every envelope at 0 with its gate off, and a filter bleed of
 * 0, for TAIL_HOLD_BLOCKS blocks, the next block fades to 0 linearly (TAIL_BLOCK samples) and the
 * output stays exactly 0 until a voice gates on or a bleed is set; then the chip's output comes
 * back as it is, at once. A bleed keeps the leak: it is a deliberate effect of the character. The
 * hold is longer than the gaps between the notes of a phrase, so they play untouched.
 *
 * Integers only, one block at a time (the audio block), nothing per sample while a note sounds:
 * the caller does the one-compare test (tail_gate_sounding) and calls tail_gate_run only when it
 * may be quiet. Portable C with no hardware access; tail_gate_run runs in the audio interrupt
 * (platform/fm1/app.ld puts it in SRAM). */
#ifndef SIDOU_TAIL_GATE_H
#define SIDOU_TAIL_GATE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The audio block (platform/fm1/audio.h AUDIO_HALF_FRAMES), and the hold: 16 blocks, 2048
 * samples, about 46 ms. */
#define TAIL_BLOCK_LOG2 7u
#define TAIL_BLOCK (1u << TAIL_BLOCK_LOG2)
#define TAIL_HOLD_BLOCKS 16u

typedef struct {
    uint32_t n; /* blocks the chip(s) have been quiet, up to TAIL_HOLD_BLOCKS + 2 */
} tail_gate;

/* The mono block `s` (TAIL_BLOCK samples: chip A's, chip B's mixed in) of chip a, and of chip b
 * if it rendered (else NULL). Run after every block that may be quiet. */
void tail_gate_run(tail_gate* t, const sid* a, const sid* b, int16_t* s);

/* 1 if chip a's first voice is sounding and the gate is at rest: tail_gate_run would change
 * nothing, so the call may be skipped (a voice that is sounding has a nonzero envelope counter or
 * its gate on, and a quiet chip's first voice is the one most often at 0). */
static inline int tail_gate_sounding(const tail_gate* t, const sid* a) {
    return a->env[0].counter != 0u && t->n == 0u;
}

#ifdef __cplusplus
}
#endif

#endif
