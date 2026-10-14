// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_ENV_H
#define SIDOU_SID_ENV_H

#include <stdint.h>

typedef enum { SID_ENV_ATTACK, SID_ENV_DECAY_SUSTAIN, SID_ENV_RELEASE } sid_env_state;

/* The envelope follows reSIDfp's EnvelopeGenerator (libresidfp c1791383) cycle for
 * cycle, so ENV3 reads the same values at the same cycles.
 *
 * The rate counter is a 15-bit LFSR that starts at 0x7fff and shifts once a cycle
 * until it equals the rate's table value; the next cycle reloads it. Its period is
 * 32767, so the LFSR is kept here as a position, the number of shifts since 0x7fff,
 * and each table value as its position (period - 1). A rate lowered below the
 * position has to wait for the LFSR to come round (the ADSR delay bug).
 *
 * After the reload come pipelines of one to four cycles: the exponential divider, then
 * the counter step (two cycles after the reload in attack or at divider period 1, three
 * otherwise). Gate changes go through a two- or three-cycle state pipeline whose first
 * cycle briefly selects the decay rate. A period change from a threshold takes effect
 * a cycle late, and ENV3 is the counter latched at the start of the cycle. */
#define SID_ENV_LFSR_PERIOD 32767u

/* Shifts from 0x7fff to reSIDfp's adsrtable[] values: reSID's periods minus one. */
static const uint16_t SID_ENV_RATE_POS[16] = {8,   31,  62,   94,   148,   219,   266,   312,
                                              391, 976, 1953, 3125, 3906, 11719, 19531, 31250};

typedef struct {
    uint32_t idle;           /* cycles that only shift the LFSR, still to come; see below */
    uint32_t lfsr_pos;       /* rate LFSR, as shifts since 0x7fff (0..32766); see below */
    uint32_t rate_pos;       /* the position that ends the current rate period */
    uint32_t exp_counter;    /* exponential divider */
    uint32_t exp_period;     /* 1, 2, 4, 8, 16 or 30 */
    uint32_t new_exp_period; /* applied at the start of the next cycle; 0 for none */
    sid_env_state state;
    sid_env_state next_state;   /* where the state pipeline leads */
    uint8_t state_pipeline;     /* cycles left of a state change */
    uint8_t envelope_pipeline;  /* cycles until the counter steps */
    uint8_t exponential_pipeline; /* cycles until the divider fires */
    uint8_t reset_lfsr;         /* the LFSR matched: reload it on the next free cycle */
    uint8_t counter_enabled;    /* 0 once the counter reaches 0; an attack enables it */
    uint8_t counter;
    uint8_t env3; /* the counter at the start of the last cycle: what $1C reads */
    uint8_t attack, decay, release;
    uint8_t sustain; /* the 4-bit level in both nibbles, compared with the counter */
    uint8_t gate;
} sid_env;

/* The envelope counter at power-on, as in reSIDfp. The real chip's counter starts at
 * whatever its cells settle to; reSIDfp's 0xaa is a value sampled from one. */
#define SID_ENV_POWER_ON 0xaau

static inline uint32_t sid_env_rate_pos(uint8_t rate) { return SID_ENV_RATE_POS[rate & 0x0f]; }

/* While `idle` is nonzero the envelope is quiet (no pipeline or reload pending) and
 * the LFSR reaches the rate in `idle` shifts, so sid_env_advance only counts `idle`
 * down and lfsr_pos is stale. This makes lfsr_pos current again; every write calls it
 * before changing the state. */
static inline void sid_env_settle(sid_env* e) {
    if (e->idle) {
        e->lfsr_pos = e->rate_pos >= e->idle ? e->rate_pos - e->idle
                                             : e->rate_pos + SID_ENV_LFSR_PERIOD - e->idle;
        e->idle = 0;
    }
}

/* The rate LFSR's position, current or not. */
static inline uint32_t sid_env_lfsr_pos(const sid_env* e) {
    sid_env copy = *e;
    sid_env_settle(&copy);
    return copy.lfsr_pos;
}

/* Like the chip's reset, this keeps the counter ("counter is not changed on reset",
 * reSIDfp), the LFSR and ENV3, and leaves the envelope releasing at rate 0 with the
 * counter enabled, whatever its value (so a counter at 0 wraps to 0xff). The LFSR
 * reloads on the first cycle, so the first release step comes on the third.
 * sid_init sets SID_ENV_POWER_ON first. Before M1b Task 2, this release stepped first
 * after a full rate period, so the power-on phase was one step behind reSIDfp. */
/* sid_env_reset for an envelope with nothing to settle (a zeroed one, as sid_init
 * makes): sid_init runs from flash and must not call sid_env_settle, which runs from
 * SRAM on the device (platform/fm1/app.ld). */
static inline void sid_env_reset_settled(sid_env* e) {
    e->envelope_pipeline = 0;
    e->state_pipeline = 0;
    e->attack = e->decay = e->sustain = e->release = 0;
    e->gate = 0;
    e->reset_lfsr = 1;
    e->exp_counter = 0;
    e->exp_period = 1;
    e->new_exp_period = 0;
    e->state = SID_ENV_RELEASE;
    e->counter_enabled = 1;
    e->rate_pos = sid_env_rate_pos(0);
}

static inline void sid_env_reset(sid_env* e) {
    sid_env_settle(e);
    sid_env_reset_settled(e);
}

static inline void sid_env_set_gate(sid_env* e, uint8_t gate) {
    gate = gate ? 1u : 0u;
    if (gate == e->gate) {
        return;
    }
    sid_env_settle(e);
    e->gate = gate;
    if (gate) {
        e->next_state = SID_ENV_ATTACK;
        e->state_pipeline = 2;
        if (e->reset_lfsr || e->exponential_pipeline == 2) {
            e->envelope_pipeline =
                (e->exp_period == 1 || e->exponential_pipeline == 2) ? 2u : 4u;
        } else if (e->exponential_pipeline == 1) {
            e->state_pipeline = 3;
        }
    } else {
        e->next_state = SID_ENV_RELEASE;
        e->state_pipeline = e->envelope_pipeline ? 3u : 2u;
    }
}

static inline void sid_env_write_ad(sid_env* e, uint8_t v) {
    sid_env_settle(e);
    e->attack = v >> 4;
    e->decay = v & 0x0f;
    if (e->state == SID_ENV_ATTACK) {
        e->rate_pos = sid_env_rate_pos(e->attack);
    } else if (e->state == SID_ENV_DECAY_SUSTAIN) {
        e->rate_pos = sid_env_rate_pos(e->decay);
    }
}

static inline void sid_env_write_sr(sid_env* e, uint8_t v) {
    sid_env_settle(e);
    e->sustain = (uint8_t)((v & 0xf0) | (v >> 4));
    e->release = v & 0x0f;
    if (e->state == SID_ENV_RELEASE) {
        e->rate_pos = sid_env_rate_pos(e->release);
    }
}

static inline void sid_env_state_change(sid_env* e) {
    --e->state_pipeline;
    switch (e->next_state) {
    case SID_ENV_ATTACK:
        if (e->state_pipeline == 1) {
            e->rate_pos = sid_env_rate_pos(e->decay); /* "accidentally", for one cycle */
        } else if (e->state_pipeline == 0) {
            e->state = SID_ENV_ATTACK;
            e->rate_pos = sid_env_rate_pos(e->attack);
            e->counter_enabled = 1;
        }
        break;
    case SID_ENV_DECAY_SUSTAIN:
        if (e->state_pipeline == 0) {
            e->state = SID_ENV_DECAY_SUSTAIN;
            e->rate_pos = sid_env_rate_pos(e->decay);
        }
        break;
    case SID_ENV_RELEASE:
        if ((e->state == SID_ENV_ATTACK && e->state_pipeline == 0) ||
            (e->state == SID_ENV_DECAY_SUSTAIN && e->state_pipeline == 1)) {
            e->state = SID_ENV_RELEASE;
            e->rate_pos = sid_env_rate_pos(e->release);
        }
        break;
    }
}

static inline void sid_env_step(sid_env* e) {
    if (e->state == SID_ENV_ATTACK) {
        if (++e->counter == 0xff) {
            e->next_state = SID_ENV_DECAY_SUSTAIN;
            e->state_pipeline = 3;
        }
    } else if (--e->counter == 0) {
        e->counter_enabled = 0;
    }
    switch (e->counter) {
    case 0xff:
    case 0x00: e->new_exp_period = 1; break;
    case 0x5d: e->new_exp_period = 2; break;
    case 0x36: e->new_exp_period = 4; break;
    case 0x1a: e->new_exp_period = 8; break;
    case 0x0e: e->new_exp_period = 16; break;
    case 0x06: e->new_exp_period = 30; break;
    default: break;
    }
}

/* One chip cycle, exactly as reSIDfp's EnvelopeGenerator::clock(). Needs a settled
 * envelope (idle == 0); sid_env_advance sees to that. */
static inline void sid_env_clock(sid_env* e) {
    e->env3 = e->counter;
    if (e->new_exp_period) {
        e->exp_period = e->new_exp_period;
        e->new_exp_period = 0;
    }
    if (e->state_pipeline) {
        sid_env_state_change(e);
    }
    if (e->envelope_pipeline && --e->envelope_pipeline == 0) {
        if (e->counter_enabled) {
            sid_env_step(e);
        }
    } else if (e->exponential_pipeline && --e->exponential_pipeline == 0) {
        e->exp_counter = 0;
        if ((e->state == SID_ENV_DECAY_SUSTAIN && e->counter != e->sustain) ||
            e->state == SID_ENV_RELEASE) {
            e->envelope_pipeline = 1;
        }
    } else if (e->reset_lfsr) {
        e->lfsr_pos = 0;
        e->reset_lfsr = 0;
        if (e->state == SID_ENV_ATTACK) {
            e->exp_counter = 0;
            e->envelope_pipeline = 2;
        } else if (e->counter_enabled && ++e->exp_counter == e->exp_period) {
            e->exponential_pipeline = e->exp_period != 1 ? 2u : 1u;
        }
    }
    if (e->lfsr_pos != e->rate_pos) {
        if (++e->lfsr_pos == SID_ENV_LFSR_PERIOD) {
            e->lfsr_pos = 0;
        }
    } else {
        e->reset_lfsr = 1;
    }
}

/* The general case of sid_env_advance, out of line to keep the common case small. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static inline void sid_env_advance_slow(sid_env* e, uint32_t cycles) {
    sid_env_settle(e);
    for (;;) {
        uint32_t quiet = !(e->state_pipeline | e->envelope_pipeline | e->exponential_pipeline |
                           e->reset_lfsr | e->new_exp_period);
        if (quiet) {
            /* Shifts until the LFSR reaches the rate; the cycle after that matches. */
            uint32_t to_match = e->rate_pos >= e->lfsr_pos
                                    ? e->rate_pos - e->lfsr_pos
                                    : SID_ENV_LFSR_PERIOD - e->lfsr_pos + e->rate_pos;
            if (cycles < to_match) {
                if (cycles) {
                    e->env3 = e->counter;
                }
                e->idle = to_match - cycles; /* lfsr_pos goes stale here */
                return;
            }
            if (to_match) {
                e->env3 = e->counter;
                e->lfsr_pos = e->rate_pos;
                cycles -= to_match;
            }
        }
        if (!cycles) {
            return;
        }
        sid_env_clock(e);
        --cycles;
    }
}

/* Same result as `cycles` calls of sid_env_clock. While no pipeline or reload is
 * pending, a cycle only shifts the LFSR, so those stretches cost one subtraction;
 * the few cycles around each match run one at a time. */
static inline void sid_env_advance(sid_env* e, uint32_t cycles) {
    if (cycles < e->idle && cycles) { /* idle stays nonzero, so lfsr_pos stays derivable */
        e->idle -= cycles;
        e->env3 = e->counter;
        return;
    }
    sid_env_advance_slow(e, cycles);
}

#endif
