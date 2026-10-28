// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live_dice.h"

/* The next number of a xorshift32 generator, the state advanced in place. */
static uint32_t next(uint32_t* s) {
    uint32_t x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    return x;
}

void live_dice_roll(live* l, uint32_t seed) {
    uint32_t s = (seed ^ ++l->dice_rolls * 0x9E3779B9u) | 1u;
    live_set_preset(l, LIVE_PRESET_DICE); /* the base, no edits kept over it */
    live_dice_shape(l, s);
    live_sound_get(l, l->base); /* DICE's base is its roll */
}

void live_dice_shape(live* l, uint32_t s) {
    s |= 1u; /* xorshift's state is never 0 */
    /* waveform: 3 of 8 saw, 2 pulse, 2 triangle, 1 noise (live_param's bits) */
    static const uint8_t WAVES[8] = {2, 2, 2, 4, 4, 1, 1, 8};
    uint32_t wave = WAVES[next(&s) % 8u];
    live_param_set(l, LIVE_PARAM_WAVEFORM, wave);
    /* one number a choice, drawn whether or not the choice applies */
    uint32_t ring = next(&s);
    uint32_t sync = next(&s);
    live_param_set(l, LIVE_PARAM_RING, wave == 1u && ring % 4u == 0u);
    live_param_set(l, LIVE_PARAM_SYNC, (wave == 2u || wave == 4u) && sync % 8u == 0u);
    live_param_set(l, LIVE_PARAM_ATTACK, next(&s) % 7u);
    live_param_set(l, LIVE_PARAM_DECAY, 4u + next(&s) % 8u);
    uint32_t sustain = next(&s);
    live_param_set(l, LIVE_PARAM_SUSTAIN, wave == 8u ? 0u : 4u + sustain % 12u);
    live_param_set(l, LIVE_PARAM_RELEASE, 3u + next(&s) % 8u);
    live_param_set(l, LIVE_PARAM_CUTOFF, live_cutoff_steps[l->model][16u + next(&s) % 45u]);
    live_param_set(l, LIVE_PARAM_RESONANCE, next(&s) % 13u);
    static const uint8_t MODES[8] = {1, 1, 1, 1, 1, 2, 4, 5}; /* LP x5, BP, HP, LP+HP */
    live_param_set(l, LIVE_PARAM_FILTER_MODE, MODES[next(&s) % 8u]);
    live_param_set(l, LIVE_PARAM_FILTER_VOICES, 7u);
    uint32_t width = next(&s);
    if (wave == 4u) {
        live_param_set(l, LIVE_PARAM_PULSE_WIDTH, 128u + 64u * (4u + width % 27u));
    }
    uint32_t r = next(&s);
    live_param_set(l, LIVE_PARAM_VIBRATO_DEPTH, r % 8u == 0u ? 8u + r % 33u : 0u);
}
