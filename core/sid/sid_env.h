// SPDX-FileCopyrightText: 2011-2022 Leandro Nini <drfiemost@users.sourceforge.net>
// SPDX-FileCopyrightText: 2018 VICE Project
// SPDX-FileCopyrightText: 2007-2010 Antti Lankila
// SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-2.0-or-later
#ifndef SIDOU_SID_ENV_H
#define SIDOU_SID_ENV_H

#include <stdint.h>

typedef enum { SID_ENV_ATTACK, SID_ENV_DECAY_SUSTAIN, SID_ENV_RELEASE } sid_env_state;

#define SID_ENV_LFSR_PERIOD 32767u
#define SID_ENV_DORMANT 0xffffffffu

#if defined(__PI32V2__) && !defined(SID_ENV_TABLES_IN_FLASH)
__attribute__((section(".data.sid_env_rate_pos")))
#endif
static const uint16_t SID_ENV_RATE_POS[16] = {8,   31,  62,   94,   148,   219,   266,   312,
                                              391, 976, 1953, 3125, 3906, 11719, 19531, 31250};

typedef struct {
    uint32_t idle;
    uint32_t lfsr_pos;
    uint32_t rate_pos;
    uint32_t exp_counter;
    uint32_t exp_period;
    uint32_t new_exp_period;
    sid_env_state state;
    sid_env_state next_state;
    uint8_t state_pipeline;
    uint8_t envelope_pipeline;
    uint8_t exponential_pipeline;
    uint8_t reset_lfsr;
    uint8_t counter_enabled;
    uint8_t counter;
    uint8_t env3;
    uint8_t attack, decay, release;
    uint8_t sustain;
    uint8_t gate;
    uint8_t gate_reset;
    uint8_t dormant;
    uint8_t linear;
    uint8_t lin_up;
    uint8_t lin_steps;
    uint8_t lin_base_counter;
    uint32_t lin_base_x;
    uint32_t lin_span;
    uint32_t lin_elapsed;
    uint32_t lin_mark;
} sid_env;

#define SID_ENV_POWER_ON 0xaau

static inline uint32_t sid_env_rate_pos(uint8_t rate) { return SID_ENV_RATE_POS[rate & 0x0f]; }

#if defined(__PI32V2__) && !defined(SID_ENV_TABLES_IN_FLASH)
__attribute__((section(".data.sid_env_thresholds")))
#endif
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
            e->rate_pos = sid_env_rate_pos(e->decay);
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

static inline void sid_env_skip_periods(sid_env* e, uint32_t periods) {
    if (e->counter_enabled) {
        e->exp_counter = (e->exp_counter + periods % e->exp_period) % e->exp_period;
    }
}

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

static inline void sid_env_match(sid_env* e, uint32_t length, uint32_t steps) {
    e->env3 = e->counter;
    e->lfsr_pos = length - 1;
    if (e->state == SID_ENV_ATTACK || length > 2) {
        e->exp_counter = 0;
    } else if (e->counter_enabled) {
        ++e->exp_counter;
    }
    if (steps && e->counter_enabled) {
        sid_env_step(e);
    }
}

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
            ++counter;
            e->exp_counter = 0;
            cycles -= period;
        }
    } else if (e->counter_enabled) {
        uint32_t x = e->exp_counter;
        int decay = e->state == SID_ENV_DECAY_SUSTAIN;
        while (cycles >= period) {
            if (x + 1 == e->exp_period) {
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
        e->env3 = e->counter;
    }
    return cycles;
}

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
        e->idle = 4;
        e->lin_span = period;
    } else {
        uint32_t x = e->exp_counter;
        if (x >= e->exp_period || c == 0) {
            return 0;
        }
        uint32_t stop = sid_env_threshold_below(c) + 1;
        if (e->state == SID_ENV_DECAY_SUSTAIN && c > e->sustain && e->sustain > stop) {
            stop = e->sustain;
        }
        steps = c - stop;
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

static inline uint32_t sid_env_run_linear(sid_env* e, uint32_t cycles) {
    uint32_t c = cycles;
    uint32_t next = e->idle;
    e->lin_elapsed += e->lin_mark - next;
    uint32_t before = e->counter;
    uint32_t last = 0;
    while (c >= next) {
        c -= next;
        last = c == 0;
        before = e->counter;
        e->counter = (uint8_t)(e->lin_up ? before + 1 : before - 1);
        if (--e->lin_steps == 0) {
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

extern void (*const sid_env_run_far)(sid_env* e, uint32_t cycles, int may_rest);
#ifdef SID_VOICE_FAR
#define sid_env_run sid_env_run_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_env_run(sid_env* e, uint32_t cycles, int may_rest) {
    for (;;) {
        if (!(e->state_pipeline | e->envelope_pipeline | e->exponential_pipeline |
              e->reset_lfsr | e->new_exp_period)) {
            uint32_t to_match = e->rate_pos >= e->lfsr_pos
                                    ? e->rate_pos - e->lfsr_pos
                                    : SID_ENV_LFSR_PERIOD - e->lfsr_pos + e->rate_pos;
            if (cycles < to_match) {
                if (cycles) {
                    e->env3 = e->counter;
                }
                if (may_rest) {
                    e->idle = to_match - cycles;
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
                continue;
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
#endif

static inline void sid_env_settle(sid_env* e) {
    if (e->linear) {
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
            e->env3 = e->counter;
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

extern void (*const sid_env_advance_slow_far)(sid_env* e, uint32_t cycles);
#ifdef SID_VOICE_FAR
#define sid_env_advance_slow sid_env_advance_slow_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_env_advance_slow(sid_env* e, uint32_t cycles) {
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
#endif

static inline int sid_env_check(const sid_env* e) {
    if (e->linear && e->dormant) {
        return 1;
    }
    int resting = e->idle || e->linear || e->dormant;
    if (resting && (e->state_pipeline | e->envelope_pipeline | e->exponential_pipeline |
                    e->reset_lfsr | e->new_exp_period)) {
        return 2;
    }
    if ((e->linear || e->dormant) && !e->idle) {
        return 3;
    }
    if (e->linear && (!e->lin_steps || e->idle > e->lin_span)) {
        return 4;
    }
    if (e->dormant && !sid_env_is_steady(e)) {
        return 5;
    }
    if (e->lfsr_pos >= SID_ENV_LFSR_PERIOD) {
        return 6;
    }
    return 0;
}

static inline void sid_env_advance(sid_env* e, uint32_t cycles) {
    if (cycles < e->idle && cycles) {
        e->idle -= cycles;
        e->env3 = e->counter;
        return;
    }
    sid_env_advance_slow(e, cycles);
}

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

static inline void sid_env_power_on_done(sid_env* e) {
    e->counter = 0;
    e->env3 = 0;
    e->counter_enabled = 0;
    e->exp_counter = 0;
    e->exp_period = 1;
    e->new_exp_period = 0;
    e->envelope_pipeline = 0;
    e->exponential_pipeline = 0;
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
    if (e->gate_reset) {
        e->lfsr_pos = 0;
    }
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
