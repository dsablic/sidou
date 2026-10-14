// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_ENV_H
#define SIDOU_SID_ENV_H

#include <stdint.h>

typedef enum { SID_ENV_ATTACK, SID_ENV_DECAY_SUSTAIN, SID_ENV_RELEASE } sid_env_state;

/* Rate counter periods in chip cycles, as measured for reSID. */
static const uint16_t SID_RATE_PERIOD[16] = {9,   32,  63,   95,   149,  220,   267,   313,
                                             392, 977, 1954, 3126, 3907, 11720, 19532, 31251};

typedef struct {
    uint32_t rate_counter;
    uint32_t rate_period;
    uint32_t exp_counter;
    uint32_t exp_period;
    sid_env_state state;
    uint8_t counter;
    uint8_t attack, decay, sustain, release;
    uint8_t gate;
    uint8_t hold_zero;
} sid_env;

static inline void sid_env_reset(sid_env* e) {
    e->rate_counter = 0;
    e->rate_period = SID_RATE_PERIOD[0];
    e->exp_counter = 0;
    e->exp_period = 1;
    e->state = SID_ENV_RELEASE;
    e->counter = 0;
    e->attack = e->decay = e->sustain = e->release = 0;
    e->gate = 0;
    e->hold_zero = 1;
}

static inline void sid_env_set_gate(sid_env* e, uint8_t gate) {
    if (!e->gate && gate) {
        e->state = SID_ENV_ATTACK;
        e->rate_period = SID_RATE_PERIOD[e->attack];
        e->hold_zero = 0;
    } else if (e->gate && !gate) {
        e->state = SID_ENV_RELEASE;
        e->rate_period = SID_RATE_PERIOD[e->release];
    }
    e->gate = gate;
}

static inline void sid_env_write_ad(sid_env* e, uint8_t v) {
    e->attack = v >> 4;
    e->decay = v & 0x0f;
    if (e->state == SID_ENV_ATTACK) {
        e->rate_period = SID_RATE_PERIOD[e->attack];
    } else if (e->state == SID_ENV_DECAY_SUSTAIN) {
        e->rate_period = SID_RATE_PERIOD[e->decay];
    }
}

static inline void sid_env_write_sr(sid_env* e, uint8_t v) {
    e->sustain = v >> 4;
    e->release = v & 0x0f;
    if (e->state == SID_ENV_RELEASE) {
        e->rate_period = SID_RATE_PERIOD[e->release];
    }
}

static inline void sid_env_tick(sid_env* e) {
    if (e->state != SID_ENV_ATTACK && ++e->exp_counter != e->exp_period) {
        return;
    }
    e->exp_counter = 0;
    if (e->hold_zero) {
        return;
    }
    switch (e->state) {
    case SID_ENV_ATTACK:
        e->counter = (uint8_t)(e->counter + 1);
        if (e->counter == 0xff) {
            e->state = SID_ENV_DECAY_SUSTAIN;
            e->rate_period = SID_RATE_PERIOD[e->decay];
        }
        break;
    case SID_ENV_DECAY_SUSTAIN:
        if (e->counter != (uint8_t)(e->sustain * 0x11)) {
            --e->counter;
        }
        break;
    case SID_ENV_RELEASE:
        e->counter = (uint8_t)(e->counter - 1);
        break;
    }
    switch (e->counter) {
    case 0xff: e->exp_period = 1; break;
    case 0x5d: e->exp_period = 2; break;
    case 0x36: e->exp_period = 4; break;
    case 0x1a: e->exp_period = 8; break;
    case 0x0e: e->exp_period = 16; break;
    case 0x06: e->exp_period = 30; break;
    case 0x00:
        e->exp_period = 1;
        e->hold_zero = 1;
        break;
    default: break;
    }
}

/* The 15-bit rate counter goes 0x7fff -> 1, so a counter already past the period
 * needs (0x7fff - counter + period) cycles to match it again. */
static inline void sid_env_advance(sid_env* e, uint32_t cycles) {
    while (cycles) {
        uint32_t to_hit = e->rate_counter < e->rate_period
                              ? e->rate_period - e->rate_counter
                              : 0x7fffu - e->rate_counter + e->rate_period;
        if (cycles < to_hit) {
            uint32_t c = e->rate_counter + cycles;
            e->rate_counter = c >= 0x8000u ? c - 0x7fffu : c;
            return;
        }
        cycles -= to_hit;
        e->rate_counter = 0;
        sid_env_tick(e);
    }
}

#endif
