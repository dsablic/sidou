// SPDX-FileCopyrightText: 2011-2025 Leandro Nini <drfiemost@users.sourceforge.net>
// SPDX-FileCopyrightText: 2007-2010 Antti Lankila
// SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-2.0-or-later
#ifndef SIDOU_SID_OSC_H
#define SIDOU_SID_OSC_H

#include <stdint.h>

#include "gen/grid.h"
#include "gen/pulldown.h"

#if !defined(NDEBUG) && !defined(__PI32V2__) && (defined(__GNUC__) || defined(__clang__))
#define SID_DEBUG_ASSERT(c) ((c) ? (void)0 : __builtin_trap())
#else
#define SID_DEBUG_ASSERT(c) ((void)0)
#endif

#define SID_CTRL_GATE 0x01u
#define SID_CTRL_SYNC 0x02u
#define SID_CTRL_RING 0x04u
#define SID_CTRL_TEST 0x08u
#define SID_CTRL_TRI 0x10u
#define SID_CTRL_SAW 0x20u
#define SID_CTRL_PULSE 0x40u
#define SID_CTRL_NOISE 0x80u

#define SID_NOISE_TAPS 0x522914u

typedef struct {
    uint32_t acc;
    uint32_t freq;
    uint32_t pw;
    uint32_t noise;
    uint32_t sr;
    uint32_t latch;
    uint32_t overshoot;
    uint32_t out;
    uint32_t owed;
    uint32_t no_pulse;
    uint32_t no_noise;
    uint32_t wf_mask;
    const uint16_t* pd_row;
    uint8_t slow;
    uint8_t pull_low;
    uint8_t pulse_held;
    uint8_t shift_pipeline;
    uint8_t msb_rose;
    uint8_t pulled_last;
    uint8_t cycle_mode;
    uint8_t test_or_reset;
    uint8_t out_kind;
    uint8_t ramp_kind;
    uint8_t control;
    uint8_t is6581;
    uint8_t defer;
    uint32_t osc3;
    uint32_t pulse;
    uint32_t tri_saw;
    uint32_t sr_reset;
    uint32_t quiet;
    uint32_t quiet_src;
    uint32_t quiet_src_freq;
    const uint16_t* const* pulldown;
    uint32_t src_mask;
    const uint32_t (*writeback)[16];
    uint32_t writeback_keep;
    uint32_t noise_keep;
} sid_osc;

#define SID_OSC_POWER_ON_ACC 0x555555u

#define SID_NOISE_RESET_6581 50000u
#define SID_NOISE_FADE_6581 15000u
#define SID_NOISE_RESET_8580 986000u
#define SID_NOISE_FADE_8580 314300u

extern const uint16_t* const SID_PD_ROWS[];

static inline uint32_t sid_pulldown(const uint16_t* row, uint32_t x) {
    const uint8_t* p = (const uint8_t*)row + row[x >> 4] + ((x >> 1) & 7u) * 3u;
    return x & 1u ? (uint32_t)(p[1] >> 4) | ((uint32_t)p[2] << 4)
                  : (uint32_t)p[0] | ((uint32_t)(p[1] & 0x0f) << 8);
}

static inline int sid_pulldown_row(uint32_t wf) {
    switch (wf & 7u) {
    case 3: return 0;
    case 4: return wf & 8u ? 4 : -1;
    case 5: return 1;
    case 6: return 2;
    case 7: return 3;
    default: return -1;
    }
}

static inline uint32_t sid_noise_output(uint32_t sr) {
    return ((sr & (1u << 2)) << 9) | ((sr & (1u << 4)) << 6) | ((sr & (1u << 8)) << 1) |
           ((sr & (1u << 11)) >> 3) | ((sr & (1u << 13)) >> 6) | ((sr & (1u << 17)) >> 11) |
           ((sr & (1u << 20)) >> 15) | ((sr & (1u << 22)) >> 18);
}

#define SID_WB_LO(n)                                                                       \
    ((((n) & 8u) ? 1u << 13 : 0u) | (((n) & 4u) ? 1u << 17 : 0u) |                          \
     (((n) & 2u) ? 1u << 20 : 0u) | (((n) & 1u) ? 1u << 22 : 0u))
#define SID_WB_HI(n)                                                                       \
    ((((n) & 8u) ? 1u << 2 : 0u) | (((n) & 4u) ? 1u << 4 : 0u) | (((n) & 2u) ? 1u << 8 : 0u) | \
     (((n) & 1u) ? 1u << 11 : 0u))
#define SID_WB_ROW(f)                                                                      \
    {f(0u), f(1u), f(2u),  f(3u),  f(4u),  f(5u),  f(6u),  f(7u),                            \
     f(8u), f(9u), f(10u), f(11u), f(12u), f(13u), f(14u), f(15u)}
extern const uint32_t SID_NOISE_WB[2][16];

extern uint32_t SID_NOISE_WB_BYTE[256];
extern uint32_t SID_NOISE_JUMP[23][23];
void sid_noise_tables_init(void);

static inline uint32_t sid_noise_writeback(uint32_t out) {
    return SID_NOISE_WB[0][(out >> 4) & 15u] | SID_NOISE_WB[1][(out >> 8) & 15u];
}

static inline uint32_t sid_noise_writeback_for(const sid_osc* o, uint32_t out) {
    SID_DEBUG_ASSERT(out <= 0xfffu);
    if (!o->noise_keep) {
        return SID_NOISE_WB_BYTE[out >> 4];
    }
    return o->writeback[0][(out >> 4) & 15u] | o->writeback[1][(out >> 8) & 15u];
}

static inline int sid_noise_do_writeback(uint32_t wf_old, uint32_t wf_new, int is6581) {
    if (wf_old <= 8 || wf_new < 8) {
        return 0;
    }
    if (wf_new == 8 && wf_old != 0xf) {
        return 0;
    }
    if (is6581 && (((wf_old & 3) == 1 && (wf_new & 3) == 2) ||
                   ((wf_old & 3) == 2 && (wf_new & 3) == 1))) {
        return 0;
    }
    return wf_old != 0xc && wf_new != 0xc;
}

static inline void sid_noise_shift_phase2(sid_osc* o, uint32_t wf_old, uint32_t wf_new) {
    if (sid_noise_do_writeback(wf_old, wf_new, o->is6581)) {
        uint32_t wb = sid_noise_writeback_for(o, o->out);
        if (o->writeback_keep) {
            wb &= ~o->writeback_keep | o->sr;
        }
        o->latch = (o->sr & ~SID_NOISE_TAPS) | wb;
    }
    uint32_t bit = ((o->test_or_reset | o->latch) ^ (o->latch >> 5)) & 1u;
    o->sr = (o->latch >> 1) | (bit << 22);
    o->noise = sid_noise_output(o->sr);
}

enum { SID_OUT_ANY, SID_OUT_NONE, SID_OUT_TRI, SID_OUT_SAW, SID_OUT_PULSE, SID_OUT_NOISE };

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_osc_update_mode(sid_osc* o) {
    uint32_t wf = (uint32_t)o->control >> 4;
    o->quiet = 0;
    o->pull_low = 0;
    switch (wf) {
    case 0: o->out_kind = SID_OUT_NONE; break;
    case 1: o->out_kind = o->control & SID_CTRL_RING ? SID_OUT_ANY : SID_OUT_TRI; break;
    case 2: o->out_kind = SID_OUT_SAW; break;
    case 4: o->out_kind = SID_OUT_PULSE; break;
    case 8: o->out_kind = SID_OUT_NOISE; break;
    default: o->out_kind = SID_OUT_ANY; break;
    }
    int row = sid_pulldown_row(wf);
    o->ramp_kind = (uint8_t)(wf & 3u);
    o->no_pulse = wf & 4u ? 0u : 0xfffu;
    o->no_noise = wf & 8u ? 0u : 0xfffu;
    o->wf_mask = wf ? 0xfffu : 0u;
    o->pd_row = row < 0 || !o->pulldown ? 0 : o->pulldown[row];
    o->slow = (uint8_t)((o->control & SID_CTRL_TEST) || wf > 8u ||
                        (o->is6581 && (wf & 2u) && wf != 2u));
    if (!o->slow) {
        o->cycle_mode = 0;
        o->pulled_last = 0;
    }
}

static inline void sid_osc_set_profile(sid_osc* o, uint32_t profile) {
    if (profile >= SID_PROFILES) {
        profile = SID_PROFILE_AVERAGE;
    }
    o->pulldown = o->is6581 ? &SID_PD_ROWS[5u * SID_COMBINED_TABLE_6581(profile)]
                            : &SID_PD_ROWS[SID_PD_8580_ROW + 5u * SID_COMBINED_TABLE_8580(profile)];
    sid_osc_update_mode(o);
}

static inline void sid_osc_set_model(sid_osc* o, int is6581) {
    o->is6581 = (uint8_t)(is6581 != 0);
    o->writeback = SID_NOISE_WB;
    o->writeback_keep = 0;
    o->noise_keep = 0;
    sid_osc_set_profile(o, SID_PROFILE_AVERAGE);
}

static inline void sid_osc_reset(sid_osc* o) {
    if (!o->writeback) {
        o->writeback = SID_NOISE_WB;
    }
    o->freq = 0;
    o->pw = 0;
    o->overshoot = 0;
    o->control = 0;
    o->msb_rose = 0;
    o->pulled_last = 0;
    o->cycle_mode = 0;
    o->shift_pipeline = 0;
    o->sr_reset = 0;
    o->defer = 0;
    o->owed = 0;
    o->sr = 0x7fffffu;
    o->test_or_reset = 1;
    o->latch = o->sr;
    sid_noise_shift_phase2(o, 0, 0);
    o->src_mask = 0xffffffu;
    if (!o->pulldown) {
        sid_osc_set_model(o, 1);
    }
    sid_osc_update_mode(o);
}

static inline uint32_t sid_wave_ramp(uint32_t wf, uint32_t ix) {
    switch (wf & 3u) {
    case 1: return ((ix & 0x800u) ? ix ^ 0xfffu : ix) << 1 & 0xfffu;
    case 2: return ix;
    case 3: return ix & (ix << 1);
    default: return 0xfffu;
    }
}

static inline uint32_t sid_wave_index(uint32_t control, uint32_t acc, uint32_t src_acc) {
    if ((control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING) {
        acc ^= ~src_acc & 0x800000u;
    }
    return (acc >> 12) & 0xfffu;
}

static inline uint32_t sid_src_at(const sid_osc* o, uint32_t src_acc, uint32_t src_freq,
                                  uint32_t k) {
    return (src_acc + k * src_freq) & o->src_mask;
}

static inline uint32_t sid_osc_combine_ramp(const sid_osc* o, uint32_t ramp, uint32_t pulse,
                                            uint32_t noise) {
    uint32_t x = ramp & (o->no_pulse | pulse) & (o->no_noise | noise) & o->wf_mask;
    return o->pd_row ? sid_pulldown(o->pd_row, x) : x;
}

static inline uint32_t sid_osc_combine(const sid_osc* o, uint32_t ix, uint32_t pulse,
                                       uint32_t noise) {
    return sid_osc_combine_ramp(o, sid_wave_ramp(o->ramp_kind, ix), pulse, noise);
}

static inline uint32_t sid_osc_pulse(const sid_osc* o, uint32_t prev_acc) {
    return ((o->control & SID_CTRL_TEST) || (prev_acc >> 12) >= o->pw) ? 0xfffu : 0u;
}

static inline uint32_t sid_osc_acc_last(const sid_osc* o) {
    return o->pulled_last ? o->acc | 0x800000u : o->acc;
}

static inline uint32_t sid_osc_acc_before(const sid_osc* o) {
    return o->control & SID_CTRL_TEST ? o->acc : (sid_osc_acc_last(o) - o->freq) & 0xffffffu;
}

static inline uint32_t sid_noise_jump(uint32_t sr, uint32_t n) {
    if (n <= 32u) {
        while (n--) {
            sr = (sr >> 1) | (((sr ^ (sr >> 5)) & 1u) << 22);
        }
        return sr;
    }
    n %= 0x7fffffu;
    for (uint32_t k = 0; n; ++k, n >>= 1) {
        if (n & 1u) {
            uint32_t r = 0;
            for (uint32_t i = 0; i < 23u; ++i) {
                if (sr >> i & 1u) {
                    r ^= SID_NOISE_JUMP[k][i];
                }
            }
            sr = r;
        }
    }
    return sr;
}

extern void (*const sid_noise_settle_deferred_far)(sid_osc* o);
#ifdef SID_VOICE_FAR
#define sid_noise_settle_deferred sid_noise_settle_deferred_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_noise_settle_deferred(sid_osc* o) {
    o->defer = 0;
    uint32_t a0 = sid_osc_acc_last(o) + 0x2000000u;
    uint32_t a1 = a0 - o->freq;
    uint32_t a2 = a1 - o->freq;
    uint32_t pending = 0;
    if (((a0 + 0x80000u) >> 20) != ((a1 + 0x80000u) >> 20)) {
        pending = 2;
    } else if (((a1 + 0x80000u) >> 20) != ((a2 + 0x80000u) >> 20)) {
        pending = 1;
    }
    uint32_t shifts = o->owed - (pending ? 1u : 0u);
    o->owed = 0;
    if (shifts) {
        o->sr = sid_noise_jump(o->sr, shifts);
        o->test_or_reset = 0;
        o->noise = sid_noise_output(o->sr);
    }
    if (pending == 1) {
        o->test_or_reset = 0;
        o->latch = o->sr;
    }
    o->shift_pipeline = (uint8_t)pending;
}
#endif

static inline void sid_noise_settle(sid_osc* o) {
    if (o->defer) {
        sid_noise_settle_deferred(o);
    }
}

static inline void sid_osc_latch_ramp(sid_osc* o, uint32_t src_acc) {
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!o->is6581 && (wf & 3u)) {
        o->tri_saw = sid_wave_ramp(wf, sid_wave_index(o->control, sid_osc_acc_last(o), src_acc));
    }
}

static inline void sid_osc_write_freq(sid_osc* o, uint32_t freq) {
    sid_noise_settle(o);
    o->freq = freq & 0xffffu;
    o->quiet = 0;
}

static inline void sid_osc_write_pw(sid_osc* o, uint32_t pw) {
    if (!o->pulse_held) {
        o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
        o->pulse_held = 1;
    }
    o->pw = pw & 0xfffu;
    o->quiet = 0;
}

static inline void sid_osc_write_control(sid_osc* o, uint8_t v) {
    sid_noise_settle(o);
    uint32_t wf_old = o->control >> 4;
    uint32_t test_old = o->control & SID_CTRL_TEST;
    if (!o->pulse_held) {
        o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
        o->pulse_held = 1;
    }
    o->control = v;
    sid_osc_update_mode(o);
    if ((v & SID_CTRL_TEST) && !test_old) {
        o->acc = 0;
        o->pulled_last = 0;
        o->shift_pipeline = 0;
        o->latch = o->sr;
        o->sr_reset = o->is6581 ? SID_NOISE_RESET_6581 : SID_NOISE_RESET_8580;
    } else if (!(v & SID_CTRL_TEST) && test_old) {
        sid_noise_shift_phase2(o, wf_old, (uint32_t)v >> 4);
    }
}

static inline void sid_noise_fade(sid_osc* o, uint32_t cycles) {
    o->test_or_reset = 1;
    while (o->sr_reset && cycles >= o->sr_reset) {
        cycles -= o->sr_reset;
        o->sr |= o->sr >> 1;
        o->sr |= 0x400000u;
        o->latch = o->sr;
        o->noise = sid_noise_output(o->sr);
        o->sr_reset = o->sr != 0x7fffffu ? (o->is6581 ? SID_NOISE_FADE_6581 : SID_NOISE_FADE_8580)
                                         : 0;
    }
    if (o->sr_reset) {
        o->sr_reset -= cycles;
    }
}

static inline void sid_osc_cycle(sid_osc* o, uint32_t src_acc) {
    uint32_t c = o->control;
    uint32_t wf = c >> 4;
    int test = (c & SID_CTRL_TEST) != 0;
    o->pulled_last = 0;
    if (test) {
        sid_noise_fade(o, 1);
        o->pulse = 0xfffu;
    } else {
        uint32_t old = o->acc;
        o->acc = (old + o->freq) & 0xffffffu;
        uint32_t rising = ~old & o->acc;
        if (o->msb_rose) {
            o->overshoot += o->freq;
        }
        if (rising & 0x800000u) {
            o->msb_rose = 1;
            o->overshoot = (o->acc - 0x800000u) & 0xffffffu;
        }
        if (rising & 0x080000u) {
            o->shift_pipeline = 2;
        } else if (o->shift_pipeline) {
            if (--o->shift_pipeline) {
                o->test_or_reset = 0;
                o->latch = o->sr;
            } else {
                sid_noise_shift_phase2(o, wf, wf);
            }
        }
    }
    if (wf) {
        uint32_t ix = sid_wave_index(c, o->acc, src_acc);
        uint32_t mask = (o->no_noise | o->noise) & (o->no_pulse | o->pulse);
        uint32_t ramp = sid_wave_ramp(o->ramp_kind, ix);
        const uint16_t* row = o->pd_row;
        uint32_t x = ramp & mask;
        o->out = row ? sid_pulldown(row, x) : x;
        if (!o->is6581 && o->ramp_kind) {
            x = o->tri_saw & mask;
            o->osc3 = row ? sid_pulldown(row, x) : x;
            o->tri_saw = ramp;
        } else {
            o->osc3 = o->out;
        }
        if (o->is6581 && (wf & 2u) && !(o->out & 0x800u) && (o->acc & 0x800000u)) {
            o->msb_rose = 0;
            o->acc &= 0x7fffffu;
            o->pulled_last = 1;
        }
        if (wf > 8u) {
            if (o->shift_pipeline != 1 && !test) {
                o->sr &= ~SID_NOISE_TAPS | sid_noise_writeback_for(o, o->out);
                o->noise &= o->out | o->noise_keep;
            } else {
                o->noise = (o->out & ~o->noise_keep) | (o->noise & o->noise_keep);
            }
        }
    }
    o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
}

static inline uint32_t sid_osc_pull_msb(sid_osc* o, uint32_t a, uint32_t cycles,
                                        uint32_t src_acc) {
    int first = 1;
    while (cycles && (a & 0x800000u)) {
        uint32_t before = a;
        a = (a + o->freq) & 0xffffffu;
        --cycles;
        if (!(a & 0x800000u)) {
            break;
        }
        uint32_t pulse = first && o->pulse_held ? o->pulse : sid_osc_pulse(o, before);
        first = 0;
        uint32_t out = sid_osc_combine(o, sid_wave_index(o->control, a, src_acc), pulse,
                                       o->noise);
        if (!(out & 0x800u)) {
            a &= 0x7fffffu;
            o->pulled_last = cycles == 0;
        }
    }
    if (cycles) {
        uint32_t next = a + o->freq * cycles;
        if (next >= 0x800000u) {
            a = next & 0x7fffffu;
            o->pulled_last = a < o->freq;
        } else {
            a = next;
        }
    }
    o->msb_rose = 0;
    return a;
}

extern void (*const sid_noise_clock_far)(sid_osc* o, uint32_t prev, uint32_t next, uint32_t cycles);
#ifdef SID_VOICE_FAR
#define sid_noise_clock sid_noise_clock_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_noise_clock(sid_osc* o, uint32_t prev, uint32_t next, uint32_t cycles) {
    uint32_t f = o->freq;
    while (o->shift_pipeline && cycles) {
        uint32_t a = prev + f;
        if (((a + 0x80000u) ^ (prev + 0x80000u)) >> 20) {
            o->shift_pipeline = 2;
        } else if (o->shift_pipeline == 2) {
            o->test_or_reset = 0;
            o->latch = o->sr;
            o->shift_pipeline = 1;
        } else {
            uint32_t l = o->latch;
            o->sr = (l >> 1) | (((l ^ (l >> 5)) & 1u) << 22);
            o->noise = sid_noise_output(o->sr);
            o->shift_pipeline = 0;
        }
        prev = a;
        --cycles;
    }
    uint32_t rises = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    if (!cycles || !rises) {
        return;
    }
    uint32_t pending = 0;
    uint32_t last = next - f;
    if (((next + 0x80000u) >> 20) != ((last + 0x80000u) >> 20)) {
        pending = 2;
    } else if (cycles >= 2 && ((last + 0x80000u) >> 20) != ((last - f + 0x80000u) >> 20)) {
        pending = 1;
    }
    uint32_t shifts = rises - (pending ? 1u : 0u);
    if (!(o->sr | o->latch | o->noise | o->test_or_reset)) {
        o->shift_pipeline = (uint8_t)pending;
        return;
    }
    if (shifts) {
        uint32_t sr = o->sr;
        do {
            sr = (sr >> 1) | (((sr ^ (sr >> 5)) & 1u) << 22);
        } while (--shifts);
        o->sr = sr;
        o->test_or_reset = 0;
        o->noise = sid_noise_output(sr);
    }
    if (pending == 1) {
        o->test_or_reset = 0;
        o->latch = o->sr;
    }
    o->shift_pipeline = (uint8_t)pending;
}
#endif

static inline uint32_t sid_osc_seg19(uint32_t a) {
    return (a + 0x80000u) >> 20;
}

static inline uint32_t sid_osc_cycles_to_rise(uint32_t acc, uint32_t f) {
    if (!f) {
        return 0xffffffffu;
    }
    uint32_t rise = ((acc + 0x80000u) | 0xfffffu) + 1u - 0x80000u;
    return (rise - acc + f - 1u) / f;
}

static inline uint32_t sid_osc_rise_phase(uint32_t prev, uint32_t next, uint32_t f,
                                          uint32_t left) {
    uint32_t s = sid_osc_seg19(prev);
    if (sid_osc_seg19(next) == s) {
        return 0;
    }
    if (sid_osc_seg19(next - f) == s) {
        return 2;
    }
    return left >= 2u && sid_osc_seg19(next - 2u * f) == s ? 1u : 3u;
}

static inline void sid_osc_bulk_pipeline(sid_osc* o, uint32_t phase) {
    SID_DEBUG_ASSERT(phase != 3u);
    if (phase == 2u) {
        o->shift_pipeline = 2;
    } else if (phase == 1u) {
        o->shift_pipeline = 1;
        o->test_or_reset = 0;
        o->latch = o->sr;
    }
}

static inline void sid_osc_bulk_commit(sid_osc* o, uint32_t left, uint32_t src_acc,
                                       uint32_t src_freq) {
    uint32_t wf = (uint32_t)o->control >> 4;
    uint32_t f = o->freq;
    uint32_t prev = o->acc;
    uint32_t next = prev + f * left;
    uint32_t n = o->noise;
    int locked = !n && !o->sr && !o->test_or_reset && !(o->shift_pipeline == 1 && o->out);
    uint32_t phase = locked ? 0u : sid_osc_rise_phase(prev, next, f, left);
    if (!n) {
        o->sr &= ~SID_NOISE_TAPS;
        if (locked) {
            sid_noise_clock(o, prev, next, left);
        }
        o->pulled_last = 0;
        if (o->is6581 && (wf & 2u)) {
            o->acc = sid_osc_pull_msb(o, prev, left, src_acc);
        } else {
            if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
                o->msb_rose = 1;
                o->overshoot = (next - 0x800000u) & 0xffffffu;
            } else if (o->msb_rose) {
                o->overshoot += f * left;
            }
            o->acc = next & 0xffffffu;
        }
        if (wf & 3u) {
            uint32_t ix = sid_wave_index(o->control, sid_osc_acc_last(o),
                                         sid_src_at(o, src_acc, src_freq, left));
            o->tri_saw = sid_wave_ramp(wf, ix);
        }
        o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
        o->out = 0;
        o->osc3 = 0;
        sid_osc_bulk_pipeline(o, phase);
        return;
    }
    if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    } else if (o->msb_rose) {
        o->overshoot += f * left;
    }
    if (!o->is6581 && (wf & 3u) && left == 1) {
        uint32_t x = o->tri_saw & (o->no_pulse | o->pulse) & n;
        o->osc3 = o->pd_row ? sid_pulldown(o->pd_row, x) : x;
    } else {
        o->osc3 = n;
    }
    o->acc = next & 0xffffffu;
    o->pulled_last = 0;
    o->sr &= ~SID_NOISE_TAPS | sid_noise_writeback_for(o, n);
    o->out = n;
    if (wf & 3u) {
        o->tri_saw = sid_wave_ramp(wf, sid_wave_index(o->control, o->acc,
                                                      sid_src_at(o, src_acc, src_freq, left)));
    }
    o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
    sid_osc_bulk_pipeline(o, phase);
}

#define SID_OSC_QUIET_HORIZON 4096u

static inline uint32_t sid_osc_bulk_limit(const sid_osc* o, uint32_t left, uint32_t src_acc,
                                          uint32_t src_freq) {
    uint32_t f = o->freq;
    uint32_t prev = o->acc;
    uint32_t n = o->noise;
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!n && !o->sr && !o->test_or_reset && !(o->shift_pipeline == 1 && o->out)) {
        return left;
    }
    if (o->shift_pipeline) {
        return 0;
    }
    if (!f) {
        uint32_t flip_now = (o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING;
        if (!n) {
            return left;
        }
        if (flip_now || (o->pd_row && sid_pulldown(o->pd_row, n) != n) ||
            (o->is6581 && (wf & 2u) && !(n & 0x800u) && (prev & 0x800000u))) {
            return 0;
        }
        uint32_t lim = left;
        if (wf & 4u) {
            if (o->pulse != 0xfffu) {
                return 0;
            }
            if ((prev >> 12) < o->pw && lim > 1u) {
                lim = 1;
            }
        }
        return (sid_wave_ramp(o->ramp_kind, prev >> 12) & n) == n ? lim : 0;
    }
    uint32_t lim = left;
    uint32_t k = sid_osc_cycles_to_rise(prev, f);
    if (k + 1u < lim) {
        lim = k + 1u;
    }
    if (o->is6581 && (wf & 2u) && !(n & 0x800u)) {
        if (prev & 0x800000u) {
            return 0;
        }
        k = (0x800000u - prev + f - 1u) / f;
        if (k - 1u < lim) {
            lim = k - 1u;
        }
    }
    if (!n || !lim) {
        return lim;
    }
    if (o->pd_row && sid_pulldown(o->pd_row, n) != n) {
        return 0;
    }
    if (wf & 4u) {
        if (o->pulse != 0xfffu) {
            return 0;
        }
        if (lim >= 2 && ((prev + f) >> 12) < o->pw) {
            lim = 1;
        }
        if (o->pw) {
            k = (0x1000000u - prev + f - 1u) / f;
            if (k < lim) {
                lim = k;
            }
        }
    }
    uint32_t flip = 0;
    if ((o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING) {
        uint32_t s1 = src_acc + src_freq;
        if (src_freq && o->src_mask == 0xffffffu) {
            uint32_t edge = (s1 | 0x7fffffu) + 1u;
            uint32_t j = (edge - s1 + src_freq - 1u) / src_freq;
            if (j < lim) {
                lim = j;
            }
        }
        flip = (s1 & o->src_mask & 0x800000u) ? 0u : 0x800u;
    }
    uint32_t kind = o->ramp_kind;
    uint32_t need = kind == 2u ? n : kind == 3u ? (n | (n >> 1)) : kind == 1u ? n >> 1 : 0u;
    uint32_t low = need & (0u - need);
    uint32_t walks = 0;
    for (uint32_t kk = 1; kk <= lim;) {
        if (++walks > 16) {
            return kk - 1u;
        }
        uint32_t x = (prev + kk * f) >> 12;
        uint32_t xf = (x & 0xfffu) ^ flip;
        if ((sid_wave_ramp(kind, xf) & n) != n) {
            return kk - 1u;
        }
        uint32_t y = (x | 0x7ffu) + 1u;
        if (need) {
            uint32_t cand = x + (((xf | (low - 1u)) + 1u) - xf);
            if (cand < y) {
                y = cand;
            }
        }
        uint32_t next = ((y << 12) - prev + f - 1u) / f;
        kk = next > kk ? next : kk + 1u;
    }
    return lim;
}

static inline uint32_t sid_osc_test_bulk(sid_osc* o, uint32_t left, uint32_t src_acc,
                                         uint32_t src_freq) {
    uint32_t wf = (uint32_t)o->control >> 4;
    uint32_t src_end = src_acc + left * src_freq;
    if ((o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING &&
        o->src_mask == 0xffffffu && ((src_acc + src_freq) >> 23) != (src_end >> 23)) {
        return 0;
    }
    uint32_t ramp = sid_wave_ramp(wf, sid_wave_index(o->control, o->acc, src_end & o->src_mask));
    uint32_t n = o->noise;
    uint32_t x = ramp & n;
    uint32_t out = o->pd_row ? sid_pulldown(o->pd_row, x) : x;
    uint32_t osc3 = n;
    if (!o->is6581 && (wf & 3u)) {
        x = ramp & n;
        osc3 = o->pd_row ? sid_pulldown(o->pd_row, x) : x;
    }
    uint32_t pulse = o->pw ? 0u : 0xfffu;
    if (out != n || o->out != n || (!o->is6581 && (wf & 3u) && o->tri_saw != ramp) ||
        o->osc3 != osc3 ||
        o->pulse != pulse || !o->test_or_reset) {
        return 0;
    }
    uint32_t run = left;
    if (o->sr_reset && o->sr_reset <= left) {
        run = o->sr_reset - 1;
    }
    if (o->sr_reset) {
        o->sr_reset -= run;
    }
    o->pulled_last = 0;
    return run;
}

extern void (*const sid_osc_run_combined_far)(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq);
#ifdef SID_VOICE_FAR
#define sid_osc_run_combined sid_osc_run_combined_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_osc_run_combined(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq) {
    int test = (o->control & SID_CTRL_TEST) != 0;
    uint32_t k = 0;
    while (k < cycles) {
        uint32_t left = cycles - k;
        uint32_t s = sid_src_at(o, src_acc, src_freq, k);
        uint32_t done = 0;
        if (o->shift_pipeline && !test &&
            (o->noise | o->sr | o->test_or_reset | (o->shift_pipeline == 1 ? o->out : 0u))) {
            ++k;
            sid_osc_cycle(o, sid_src_at(o, src_acc, src_freq, k));
            continue;
        }
        if (test) {
            done = sid_osc_test_bulk(o, left, s, src_freq);
            int ring = (o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING;
            if (done == left && o->sr_reset != 1 && (!ring || !src_freq)) {
                o->quiet = o->sr_reset ? o->sr_reset - 1u : 0xffffffffu;
                o->quiet_src = sid_src_at(o, src_acc, src_freq, cycles);
                o->quiet_src_freq = src_freq;
            }
        } else {
            uint32_t w = sid_osc_bulk_limit(o, SID_OSC_QUIET_HORIZON, s, src_freq);
            uint32_t run = w < left ? w : left;
            if (run) {
                sid_osc_bulk_commit(o, run, s, src_freq);
                done = run;
                if (w > left && !(!o->noise && !o->sr && !o->test_or_reset) &&
                    !o->shift_pipeline) {
                    uint32_t q = w - left;
                    uint32_t to_rise = sid_osc_cycles_to_rise(o->acc, o->freq);
                    o->quiet = q < to_rise ? q : to_rise - 1u;
                    o->quiet_src = sid_src_at(o, src_acc, src_freq, cycles);
                    o->quiet_src_freq = src_freq;
                }
            }
        }
        if (done) {
            k += done;
            continue;
        }
        ++k;
        sid_osc_cycle(o, sid_src_at(o, src_acc, src_freq, k));
    }
}
#endif

static inline void sid_osc_advance_cycles(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                          uint32_t src_freq) {
    o->msb_rose = 0;
    o->cycle_mode = 1;
    for (uint32_t k = 1; k <= cycles; ++k) {
        sid_osc_cycle(o, sid_src_at(o, src_acc, src_freq, k));
    }
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline int sid_osc_quiet_step(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                     uint32_t src_freq) {
    if (o->quiet < cycles ||
        ((o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING &&
         (src_acc != o->quiet_src || src_freq != o->quiet_src_freq))) {
        return 0;
    }
    o->quiet -= cycles;
    o->quiet_src = sid_src_at(o, src_acc, src_freq, cycles);
    o->pulled_last = 0;
    o->msb_rose = 0;
    if (o->control & SID_CTRL_TEST) {
        if (o->sr_reset) {
            o->sr_reset -= cycles;
        }
        return 1;
    }
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;
    if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
    o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
    o->osc3 = o->noise;
    if (!o->is6581 && o->ramp_kind) {
        o->tri_saw = sid_wave_ramp(o->ramp_kind,
                                   sid_wave_index(o->control, o->acc, o->quiet_src));
    }
    return 1;
}

extern void (*const sid_osc_advance_slow_far)(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq);
#ifdef SID_VOICE_FAR
#define sid_osc_advance_slow sid_osc_advance_slow_far
#else
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_osc_advance_slow(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq) {
    if (sid_osc_quiet_step(o, cycles, src_acc, src_freq)) {
        return;
    }
    uint32_t wf = (uint32_t)o->control >> 4;
    sid_noise_settle(o);
    o->msb_rose = 0;
    if (wf > 8u) {
        o->cycle_mode = 1;
        o->quiet = 0;
        if (!(o->sr | o->noise | o->test_or_reset | (o->control & SID_CTRL_TEST)) &&
            !(o->shift_pipeline == 1 && o->out)) {
            uint32_t prev = o->acc;
            uint32_t next = prev + o->freq * cycles;
            if (o->shift_pipeline || (((next + 0x80000u) ^ (prev + 0x80000u)) >> 20)) {
                sid_noise_clock(o, prev, next, cycles);
            }
            o->pulled_last = 0;
            if (o->is6581 && (wf & 2u)) {
                o->acc = sid_osc_pull_msb(o, prev, cycles, src_acc);
            } else {
                if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
                    o->msb_rose = 1;
                    o->overshoot = (next - 0x800000u) & 0xffffffu;
                }
                o->acc = next & 0xffffffu;
            }
            if (!o->is6581 && (wf & 3u)) {
                uint32_t ix = sid_wave_index(o->control, o->acc,
                                             sid_src_at(o, src_acc, src_freq, cycles));
                o->tri_saw = sid_wave_ramp(wf, ix);
            }
            o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
            o->out = 0;
            o->osc3 = 0;
            o->slow = 0;
            o->cycle_mode = 0;
            o->pull_low = (uint8_t)(o->is6581 && (wf & 2u));
            return;
        }
        sid_osc_run_combined(o, cycles, src_acc, src_freq);
        return;
    }
    o->cycle_mode = 0;
    o->pulled_last = 0;
    if (o->control & SID_CTRL_TEST) {
        sid_noise_fade(o, cycles);
        return;
    }
    if (!cycles) {
        return;
    }
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;

    if (o->shift_pipeline || ((next + 0x80000u) ^ (prev + 0x80000u)) >> 20) {
        sid_noise_clock(o, prev, next, cycles);
    }

    if (o->is6581 && (wf & 2u) && wf != 2u) {
        o->acc = sid_osc_pull_msb(o, prev, cycles, src_acc);
        if (!(o->acc & 0x800000u)) {
            o->slow = 0;
            o->pull_low = 1;
        }
        return;
    }
    if (((next + 0x800000u) >> 24) != ((prev + 0x800000u) >> 24)) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}
#endif

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_osc_advance(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                   uint32_t src_freq) {
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;
    if (o->slow) {
        sid_osc_advance_slow(o, cycles, src_acc, src_freq);
        return;
    }
    uint32_t rises = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    if (rises | o->shift_pipeline) {
        if (o->defer) {
            o->owed += rises;
        } else if (!o->shift_pipeline &&
                   (o->no_noise || !(o->sr | o->latch | o->noise | o->test_or_reset))) {
            o->defer = 1;
            o->owed = rises;
        } else {
            sid_noise_clock(o, prev, next, cycles);
        }
    }
    o->msb_rose = 0;
    if (o->pull_low) {
        o->acc = next & 0x7fffffu;
        o->pulled_last = next >= 0x800000u && o->acc < o->freq;
        return;
    }
    if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline uint32_t sid_osc_output(const sid_osc* o, uint32_t src_acc) {
    switch (o->out_kind) {
    case SID_OUT_NONE: return 0;
    case SID_OUT_SAW: return o->acc >> 12;
    case SID_OUT_TRI: return sid_wave_ramp(1, o->acc >> 12);
    case SID_OUT_PULSE: return sid_osc_pulse(o, sid_osc_acc_before(o));
    case SID_OUT_NOISE: return o->noise;
    default: break;
    }
    if (o->cycle_mode) {
        return o->out;
    }
    if (!(o->no_noise | o->noise)) {
        return 0;
    }
    uint32_t acc = sid_osc_acc_last(o);
    uint32_t ix = sid_wave_index(o->control, acc, src_acc);
    uint32_t pulse = 0;
    if (!o->no_pulse) {
        pulse = sid_osc_pulse(o, sid_osc_acc_before(o));
    }
    return sid_osc_combine(o, ix, pulse, o->noise);
}

static inline void sid_osc_sync(sid_osc* o, const sid_osc* src) {
    if (o->control & SID_CTRL_TEST) {
        return;
    }
    sid_noise_settle(o);
    uint32_t cycles_after = src->overshoot / src->freq;
    o->acc = (o->freq * cycles_after) & 0xffffffu;
    o->pulled_last = 0;
    o->quiet = 0;
    if (o->pull_low) {
        o->pull_low = 0;
        o->slow = 1;
    }
}

#endif
