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
 * a cycle late, and ENV3 is the counter latched at the start of the cycle.
 *
 * One period at rate 0 (table position 8, period 9), cycle by cycle; "pos" is the
 * LFSR position at the end of the cycle:
 *
 *   cycle  pos  attack                    decay/release, divider period 1 (2+)
 *   M      8    pos == rate: reload next   (same)
 *   R      1    reload; envelope pipe = 2  reload; divider fires: exp pipe = 1 (2)
 *   R+1    2    envelope pipe 2 -> 1       exp pipe done: envelope pipe = 1 (2 -> 1)
 *   R+2    3    counter steps              counter steps (exp pipe done)
 *   R+3    4                               (counter steps)
 *   ...    ...  quiet: only the LFSR shifts, until pos == 8 again at M + 9
 *
 * ENV3 shows a step one cycle after the counter takes it.
 *
 * sid_env_advance is cheap at rest: between matches (`idle`), in a steady state where
 * nothing visible changes (`dormant`: frozen at 0, or holding sustain) and in a run of
 * ordinary steps (`linear`); see sid_env_settle. Whole match sequences and rate periods
 * otherwise run as single events. Cycle by cycle (sid_env_clock) runs only around gate
 * changes, threshold crossings and calls that end inside a match sequence. */
#define SID_ENV_LFSR_PERIOD 32767u
/* `idle` on entering a dormant state; it counts down from here. */
#define SID_ENV_DORMANT 0xffffffffu

/* Shifts from 0x7fff to reSIDfp's adsrtable[] values: reSID's periods minus one. */
static const uint16_t SID_ENV_RATE_POS[16] = {8,   31,  62,   94,   148,   219,   266,   312,
                                              391, 976, 1953, 3125, 3906, 11719, 19531, 31250};

typedef struct {
    uint32_t idle;           /* cycles sid_env_advance may skip; see sid_env_settle */
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
    uint8_t dormant; /* resting in a periodic state; see sid_env_settle */
    uint8_t linear;  /* resting in a run of ordinary steps; see sid_env_settle */
    uint8_t lin_up;  /* linear: 1 counting up (attack), 0 down */
    uint8_t lin_steps;        /* linear: steps left in the run */
    uint8_t lin_base_counter; /* linear: counter and divider when the run began */
    uint32_t lin_base_x;
    uint32_t lin_span;    /* linear: cycles between steps */
    uint32_t lin_elapsed; /* linear: cycles since the run began, up to lin_mark */
    uint32_t lin_mark;    /* linear: `idle` when lin_elapsed was last brought up */
} sid_env;

/* The envelope counter at power-on, as in reSIDfp. The real chip's counter starts at
 * whatever its cells settle to; reSIDfp's 0xaa is a value sampled from one. */
#define SID_ENV_POWER_ON 0xaau

static inline uint32_t sid_env_rate_pos(uint8_t rate) { return SID_ENV_RATE_POS[rate & 0x0f]; }

/* Counter values whose step sets a new divider period (0x00, 0x06, 0x0e, 0x1a, 0x36,
 * 0x5d, 0xff), as a 256-bit set. */
static const uint32_t SID_ENV_THRESHOLDS[8] = {0x04004041u, 0x00400000u, 0x20000000u, 0, 0,
                                               0,           0,           0x80000000u};

static inline uint32_t sid_env_is_threshold(uint8_t c) {
    return (SID_ENV_THRESHOLDS[c >> 5] >> (c & 31u)) & 1u;
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
 * envelope (idle == 0). */
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

/* A periodic state in which nothing visible changes, entered at the start of a match
 * cycle (quiet, LFSR at the rate): the counter frozen at 0 outside attack, or holding
 * the sustain level in decay. Each rate period (rate_pos + 1 cycles) returns to the
 * same state, except that while holding sustain the divider counts one period. */
static inline int sid_env_is_steady(const sid_env* e) {
    if (e->state == SID_ENV_ATTACK) {
        return 0;
    }
    if (!e->counter_enabled) {
        return 1;
    }
    return e->state == SID_ENV_DECAY_SUSTAIN && e->counter == e->sustain &&
           e->exp_counter < e->exp_period;
}

/* Skips `periods` whole rate periods of a steady state. */
static inline void sid_env_skip_periods(sid_env* e, uint32_t periods) {
    if (e->counter_enabled) { /* holding sustain: the divider counts once a period */
        e->exp_counter = (e->exp_counter + periods % e->exp_period) % e->exp_period;
    }
}

/* From the start of a match cycle M with nothing pending, the length of the sequence
 * up to its last event (M, R, then the pipelines; see the timeline above): 2 when the
 * divider does not fire, 3 or 4 when it fires without a step, 4 or 5 with a step. */
static inline uint32_t sid_env_match_length(const sid_env* e, uint32_t* steps) {
    if (e->state == SID_ENV_ATTACK) {
        *steps = 1;
        return 4;
    }
    if (!e->counter_enabled || e->exp_counter + 1 != e->exp_period) {
        *steps = 0;
        return 2;
    }
    *steps = (e->state == SID_ENV_DECAY_SUSTAIN && e->counter != e->sustain) ||
             e->state == SID_ENV_RELEASE;
    return 2 + (e->exp_period != 1 ? 2u : 1u) + *steps;
}

/* Runs that sequence as one event; the same result as `length` sid_env_clock calls. */
static inline void sid_env_match(sid_env* e, uint32_t length, uint32_t steps) {
    e->env3 = e->counter; /* any step comes in the last cycle */
    e->lfsr_pos = length - 1; /* reloaded at R, shifted every cycle from R on */
    if (e->state == SID_ENV_ATTACK || length > 2) {
        e->exp_counter = 0;
    } else if (e->counter_enabled) {
        ++e->exp_counter;
    }
    if (steps && e->counter_enabled) {
        sid_env_step(e);
    }
}

/* From the start of a match cycle with nothing pending, runs whole rate periods while
 * each one is an ordinary match sequence followed by quiet cycles: no step lands on a
 * threshold (sid_env_is_threshold covers 0, where the counter freezes, and 0xff, where
 * the attack ends). Each period ends at the start of the next match cycle. */
static inline uint32_t sid_env_run_periods(sid_env* e, uint32_t cycles) {
    uint32_t period = e->rate_pos + 1;
    if (cycles < period) {
        return cycles;
    }
    uint32_t counter = e->counter;
    uint32_t start = cycles;
    if (e->state == SID_ENV_ATTACK) {
        if (!e->counter_enabled) {
            return cycles;
        }
        while (cycles >= period && !sid_env_is_threshold((uint8_t)(counter + 1))) {
            ++counter; /* the reload clears the divider; the step comes at R+2 */
            e->exp_counter = 0;
            cycles -= period;
        }
    } else if (e->counter_enabled) {
        uint32_t x = e->exp_counter;
        int decay = e->state == SID_ENV_DECAY_SUSTAIN;
        while (cycles >= period) {
            if (x + 1 == e->exp_period) { /* the divider fires */
                if (!decay || counter != e->sustain) {
                    if (sid_env_is_threshold((uint8_t)(counter - 1))) {
                        break;
                    }
                    --counter;
                }
                x = 0;
            } else {
                ++x;
            }
            cycles -= period;
        }
        e->exp_counter = x;
    }
    if (cycles != start) {
        e->counter = (uint8_t)counter;
        e->env3 = e->counter; /* every step lands at most 4 cycles into its period */
    }
    return cycles;
}

/* The thresholds next to c (sid_env_is_threshold): above for c < 0xff, below for c > 0. */
static inline uint32_t sid_env_threshold_above(uint32_t c) {
    return c < 0x06   ? 0x06
           : c < 0x0e ? 0x0e
           : c < 0x1a ? 0x1a
           : c < 0x36 ? 0x36
           : c < 0x5d ? 0x5d
                      : 0xff;
}

static inline uint32_t sid_env_threshold_below(uint32_t c) {
    return c > 0x5d   ? 0x5d
           : c > 0x36 ? 0x36
           : c > 0x1a ? 0x1a
           : c > 0x0e ? 0x0e
           : c > 0x06 ? 0x06
                      : 0;
}

/* Linear rest. From the start of a match cycle with nothing pending, the counter takes
 * ordinary steps (none on a threshold, so no period or state change follows) at fixed
 * intervals: once a period in attack, once every exp_period periods otherwise. Until
 * the last such step, sid_env_advance only moves the counter; the other fields stay as
 * they were when the run began and sid_env_settle rebuilds them. Returns whether the
 * envelope entered linear rest. */
static inline int sid_env_enter_linear(sid_env* e) {
    if (!e->counter_enabled) {
        return 0;
    }
    uint32_t c = e->counter;
    uint32_t period = e->rate_pos + 1;
    uint32_t steps;
    if (e->state == SID_ENV_ATTACK) {
        if (c == 0xff) {
            return 0;
        }
        steps = sid_env_threshold_above(c) - c - 1;
        e->idle = 4; /* M, R, R+1, R+2 */
        e->lin_span = period;
    } else {
        uint32_t x = e->exp_counter;
        if (x >= e->exp_period || c == 0) { /* from 0, the next step wraps to 0xff */
            return 0;
        }
        uint32_t stop = sid_env_threshold_below(c) + 1;
        if (e->state == SID_ENV_DECAY_SUSTAIN && c > e->sustain && e->sustain > stop) {
            stop = e->sustain; /* the last step lands on the sustain level */
        }
        steps = c - stop;
        /* The divider fires in period exp_period - 1 - x; the step lands at the end of
         * its match sequence. */
        e->idle = (e->exp_period - 1 - x) * period + 3 + (e->exp_period != 1 ? 2u : 1u);
        e->lin_span = period * e->exp_period;
    }
    if (!steps) {
        e->idle = 0;
        return 0;
    }
    e->lin_mark = e->idle;
    e->linear = 1;
    e->lin_up = e->state == SID_ENV_ATTACK;
    e->lin_steps = (uint8_t)steps;
    e->lin_base_counter = (uint8_t)c;
    e->lin_base_x = e->exp_counter;
    e->lin_elapsed = 0;
    return 1;
}

/* Runs `cycles` in linear rest. If the run's last step lands within them, leaves linear
 * rest with every field current at that step and returns the cycles still to run;
 * otherwise returns 0, still in linear rest. */
static inline uint32_t sid_env_run_linear(sid_env* e, uint32_t cycles) {
    uint32_t c = cycles;
    uint32_t next = e->idle; /* in linear rest, `idle` counts down to the next step */
    e->lin_elapsed += e->lin_mark - next;
    uint32_t before = e->counter;
    uint32_t last = 0; /* the latest step landed in the last cycle */
    while (c >= next) {
        c -= next;
        last = c == 0;
        before = e->counter;
        e->counter = (uint8_t)(e->lin_up ? before + 1 : before - 1);
        if (--e->lin_steps == 0) {
            /* As right after an ordinary match sequence with a step (sid_env_match). */
            e->linear = 0;
            e->idle = 0;
            e->env3 = (uint8_t)before;
            e->exp_counter = 0;
            e->lfsr_pos = e->lin_up ? 3u : (e->exp_period != 1 ? 4u : 3u);
            return c;
        }
        next = e->lin_span;
    }
    e->idle = next - c;
    e->lin_mark = e->idle;
    e->lin_elapsed += cycles;
    if (cycles) {
        e->env3 = (uint8_t)(last ? before : e->counter);
    }
    return 0;
}

/* Runs `cycles` cycles exactly, a stretch of LFSR shifts, a steady state or a match
 * sequence at a time where it can, and the cycles around writes, state changes and
 * period changes one at a time. With may_rest, it ends with the envelope at rest
 * (`idle` or `dormant` set) when it can; without, it ends settled. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static inline void sid_env_run(sid_env* e, uint32_t cycles, int may_rest) {
    for (;;) {
        if (!(e->state_pipeline | e->envelope_pipeline | e->exponential_pipeline |
              e->reset_lfsr | e->new_exp_period)) {
            /* Shifts until the LFSR reaches the rate; the cycle after that matches. */
            uint32_t to_match = e->rate_pos >= e->lfsr_pos
                                    ? e->rate_pos - e->lfsr_pos
                                    : SID_ENV_LFSR_PERIOD - e->lfsr_pos + e->rate_pos;
            if (cycles < to_match) {
                if (cycles) {
                    e->env3 = e->counter;
                }
                if (may_rest) {
                    e->idle = to_match - cycles; /* lfsr_pos goes stale */
                } else {
                    uint32_t pos = e->lfsr_pos + cycles;
                    e->lfsr_pos = pos >= SID_ENV_LFSR_PERIOD ? pos - SID_ENV_LFSR_PERIOD : pos;
                }
                return;
            }
            if (to_match) {
                e->env3 = e->counter;
                e->lfsr_pos = e->rate_pos;
                cycles -= to_match;
            }
            /* The next cycle is a match cycle. */
            if (sid_env_is_steady(e)) {
                if (may_rest) {
                    e->dormant = 1;
                    e->idle = SID_ENV_DORMANT - cycles;
                    if (cycles) {
                        e->env3 = e->counter;
                    }
                    return;
                }
                uint32_t period = e->rate_pos + 1;
                if (cycles >= period) {
                    sid_env_skip_periods(e, cycles / period);
                    cycles %= period;
                    e->env3 = e->counter;
                }
            }
            if (may_rest && sid_env_enter_linear(e)) {
                cycles = sid_env_run_linear(e, cycles);
                if (e->linear) {
                    return;
                }
                continue; /* settled at the run's last step */
            }
            cycles = sid_env_run_periods(e, cycles);
            uint32_t steps;
            uint32_t length = sid_env_match_length(e, &steps);
            if (cycles >= length) {
                sid_env_match(e, length, steps);
                cycles -= length;
                continue;
            }
        }
        if (!cycles) {
            return;
        }
        sid_env_clock(e);
        --cycles;
    }
}

/* At rest, sid_env_advance only counts `idle` down, and some fields are stale:
 * - idle (not dormant): the envelope is quiet and the LFSR reaches the rate in `idle`
 *   shifts; lfsr_pos is stale;
 * - dormant: the envelope entered a steady state (sid_env_is_steady) at the start of a
 *   match cycle, SID_ENV_DORMANT - idle cycles ago; lfsr_pos, exp_counter, the
 *   pipelines and reset_lfsr are stale;
 * - linear: a run of ordinary steps began at the start of a match cycle,
 *   lin_elapsed + lin_mark - idle cycles ago (sid_env_enter_linear), and the next
 *   step lands in `idle` cycles; lfsr_pos, exp_counter, the pipelines and reset_lfsr
 *   are stale, and are rebuilt with a division.
 * counter and env3 are never stale.
 * This brings every field up to date; every write calls it first. Waking from dormant
 * costs a division, at most once per write. */
static inline void sid_env_settle(sid_env* e) {
    if (e->linear) {
        /* k whole periods from the start of the run, then r cycles of the next. */
        uint32_t period = e->rate_pos + 1;
        uint32_t elapsed = e->lin_elapsed + (e->lin_mark - e->idle);
        uint32_t k = elapsed / period;
        uint32_t r = elapsed % period;
        uint32_t fires;
        if (e->lin_up) {
            fires = k;
            e->exp_counter = k ? 0 : e->lin_base_x;
        } else {
            fires = (e->lin_base_x + k) / e->exp_period;
            e->exp_counter = (e->lin_base_x + k) % e->exp_period;
        }
        e->counter = (uint8_t)(e->lin_up ? e->lin_base_counter + fires
                                         : e->lin_base_counter - fires);
        if (elapsed) {
            e->env3 = e->counter; /* each period ends quiet */
        }
        e->linear = 0;
        e->idle = 0;
        e->lfsr_pos = e->rate_pos;
        sid_env_run(e, r, 0);
        return;
    }
    if (!e->idle) {
        return;
    }
    if (e->dormant) {
        uint32_t elapsed = SID_ENV_DORMANT - e->idle;
        uint32_t period = e->rate_pos + 1;
        e->idle = 0;
        e->dormant = 0;
        sid_env_skip_periods(e, elapsed / period);
        sid_env_run(e, elapsed % period, 0);
        return;
    }
    e->lfsr_pos = e->rate_pos >= e->idle ? e->rate_pos - e->idle
                                         : e->rate_pos + SID_ENV_LFSR_PERIOD - e->idle;
    e->idle = 0;
}

/* sid_env_advance's work when the envelope is not at rest, out of line. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static inline void sid_env_advance_slow(sid_env* e, uint32_t cycles) {
    if (e->linear) {
        cycles = sid_env_run_linear(e, cycles);
        if (e->linear) {
            return;
        }
    } else {
        sid_env_settle(e);
    }
    sid_env_run(e, cycles, 1);
}

/* Same result as `cycles` calls of sid_env_clock. At rest it costs a compare and a
 * subtraction, inline; otherwise sid_env_advance_slow does the work. */
static inline void sid_env_advance(sid_env* e, uint32_t cycles) {
    if (cycles < e->idle && cycles) { /* idle stays nonzero: the rest continues */
        e->idle -= cycles;
        e->env3 = e->counter;
        return;
    }
    sid_env_advance_slow(e, cycles);
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

#endif
