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
/* What the chips play. The triad is the benchmark workload; the others are envelope
 * worst cases for the emulator gate (see workload.c). */
enum {
    WORKLOAD_TRIAD,
    WORKLOAD_SUSTAIN_FF,
    WORKLOAD_SUSTAIN_AA,
    WORKLOAD_GATE_EVERY_FRAME,
    WORKLOAD_IDLE,
    WORKLOAD_COMBINED,
    WORKLOAD_GATE_EVERY_BLOCK,
    WORKLOAD_NOISE_TEST_HELD,
    WORKLOAD_NOISE_SAW,
    WORKLOAD_NOISE_MIX,
    WORKLOAD_NOISE_RETRIGGER3,
    WORKLOAD_NOISE_TEST_EVERY_BLOCK,
    WORKLOAD_NOISE_TEST_EVERY_BLOCK_B,
    WORKLOAD_NOISE_TRI_FFFF,
    WORKLOAD_NOISE_TRI_RING_FFFF,
    WORKLOAD_NOISE_TRI_0800,
    WORKLOAD_NOISE_SAW_0800,
    WORKLOAD_NOISE_TRI_RING_0800,
    WORKLOAD_NOISE_SAW_FFFF,
    WORKLOAD_NOISE_SAW_SYNC_FFFF,
    WORKLOAD_SAW_FFFF,
    WORKLOAD_SAW_E000,
    WORKLOAD_PULSE_SAW_FFFF,
    WORKLOAD_PULSE_SAW_E000,
    /* The chip's character (sid_character.h): the worn 6581 profile on the 6581, the 8580
     * R5 on the 8580, and every 8th block the stage swapped between the controls at 127 and
     * at 64 (sid_character_commit, as the register ring's drain does). On the triad every
     * control; on the two worst blocks' scenarios the bleed alone, the costliest. */
    WORKLOAD_CHARACTER_TRIAD,
    WORKLOAD_CHARACTER_NOISE_TRI_FFFF,
    WORKLOAD_CHARACTER_NOISE_TRI_RING_0800,
    WORKLOAD_SCENARIOS
};

/* The triad. */
void workload_begin(uint32_t chips, sid_model model);
/* An unknown scenario plays the triad. */
void workload_begin_scenario(uint32_t chips, sid_model model, uint32_t scenario);
/* NULL for an unknown scenario. */
const char* workload_scenario_name(uint32_t scenario);
void workload_render_samples(int16_t* out, uint32_t n);
uint32_t workload_sample_count(void);

#ifdef __cplusplus
}
#endif

#endif
