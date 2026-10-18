// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_OSC_H
#define SIDOU_SID_OSC_H

#include <stdint.h>

#include "gen/grid.h"
#include "gen/noise_jump.h"
#include "gen/pulldown.h"

/* An internal invariant, checked in host builds (the tests and fuzzers trap on it, as
 * UBSan does) and compiled out on the device and with NDEBUG. No <assert.h>: the
 * freestanding ILP32 layout pass (sidou-sys build.rs) has no libc headers. */
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

/* The oscillator follows reSIDfp's WaveformGenerator (libresidfp c1791383) closely
 * enough that OSC3 reads the same values at the same cycles, for every waveform.
 *
 * Combined waveforms: the AND of the selected waveforms goes through a pulldown table
 * generated from reSIDfp (gen/pulldown.h, rows ST, PT, PS, PST and NP), one per
 * combined-waveform profile (gen/grid.h; reSIDfp's AVERAGE by default). On the 6581, a
 * combination with sawtooth whose output has its top bit low pulls the accumulator's
 * MSB low too; since the output's top bit is always low when the accumulator enters
 * its upper half (checked against the tables in the unit tests), such a waveform never
 * stays in the upper half.
 *
 * Noise: a 23-bit LFSR, kept in reSIDfp's bit order (bit 22 is where the new bit
 * enters). It shifts in two phases, one and two cycles after accumulator bit 19
 * rises. Combined with other waveforms, the output pulls the register's output bits
 * down every cycle (write-back), which can lock it at 0; TEST does not clear it but
 * lets it fade to all ones, after a delay that depends on the model. */

/* The register's output bits, in reSIDfp's order. */
#define SID_NOISE_TAPS 0x522914u /* bits 2, 4, 8, 11, 13, 17, 20, 22 */

typedef struct {
    /* Used every step, first so their offsets stay short. */
    uint32_t acc;
    uint32_t freq;
    uint32_t pw;
    uint32_t noise;    /* noise waveform (12-bit); combined waveforms pull it down */
    uint32_t sr;       /* noise shift register */
    uint32_t latch;    /* noise shift latch */
    uint32_t overshoot; /* accumulator units past the last MSB rise, for hard sync */
    uint32_t out;      /* waveform output of the last cycle (12-bit) */
    uint32_t owed;     /* defer: bit-19 rises not yet applied to the noise register */
    /* Derived from control and the pulldown tables (sid_osc_update_mode). */
    uint32_t no_pulse; /* 0xfff without pulse, else 0 */
    uint32_t no_noise; /* 0xfff without noise, else 0 */
    uint32_t wf_mask;  /* 0xfff with any waveform, else 0 */
    const uint16_t* pd_row; /* the pulldown row (SID_PD.idx), or 0 for none */
    uint8_t slow;      /* sid_osc_advance needs sid_osc_advance_slow */
    uint8_t pull_low;  /* 6581, sawtooth combination in the lower half: every MSB rise is
                        * pulled low at once (sid_osc_advance's inline path) */
    uint8_t pulse_held; /* a write since the last cycle has latched `pulse`; cleared by
                         * the caller after each step (clock_voices) */
    uint8_t shift_pipeline; /* cycles until the next phase of a pending shift */
    uint8_t msb_rose;
    uint8_t pulled_last; /* 6581: the MSB was pulled low in the last cycle */
    uint8_t cycle_mode;  /* combined with noise: the last step ran cycle by cycle */
    uint8_t test_or_reset; /* forces the new bit to 1 in the next shift */
    uint8_t out_kind;  /* SID_OUT_*: a single waveform, or any other combination */
    uint8_t ramp_kind; /* control bits 4-5: none, triangle, sawtooth, both */
    uint8_t control;
    uint8_t is6581;
    uint8_t defer;     /* noise not selected, or the register locked up at 0: shifts are
                        * counted in `owed`, and sr, latch, noise, test_or_reset and
                        * shift_pipeline are stale until sid_noise_settle */
    /* Mostly for noise combined with other waveforms. */
    uint32_t osc3;     /* combined with noise: OSC3 of the last cycle */
    uint32_t pulse;    /* the pulse comparison of the last cycle: kept cycle by cycle
                        * with noise combined, latched before writes otherwise */
    uint32_t tri_saw;  /* 8580: OSC3's triangle/saw latch (sid_osc_latch_ramp) */
    uint32_t sr_reset; /* with TEST: cycles until the register fades a bit; 0 for none */
    /* Cycles from now in which nothing changes but the accumulator (or with TEST, the
     * fade countdown); 0 for unknown. Every write that can change that (control,
     * frequency, pulse width, profile, hard sync) zeroes it. */
    uint32_t quiet;
    uint32_t quiet_src;      /* with ring modulation: the source's accumulator and */
    uint32_t quiet_src_freq; /* frequency the quiet stretch assumed for the next step */
    const uint16_t (*pulldown)[256]; /* the profile's pulldown rows (SID_PD.idx) */
    /* The ring source's accumulator as this voice sees it, cycle by cycle: masked to its
     * lower half when the source pulls every MSB rise low before this voice's output
     * looks at it (reSIDfp clocks outputs in voice order; clock_voices sets it). */
    uint32_t src_mask;
} sid_osc;

/* The accumulator at power-on, as in reSIDfp: its even bits come up high. */
#define SID_OSC_POWER_ON_ACC 0x555555u

/* reSIDfp's delays for the register's fade while TEST is set (6581R3, 8580R5). */
#define SID_NOISE_RESET_6581 50000u
#define SID_NOISE_FADE_6581 15000u
#define SID_NOISE_RESET_8580 986000u
#define SID_NOISE_FADE_8580 314300u

/* Entry x of a pulldown row (gen/pulldown.h): its block starts row[x >> 4] bytes after the
 * row, and holds 16 entries packed two to three bytes. Both reads are flash (XIP), through
 * the pointer sid_osc_update_mode set up. */
static inline uint32_t sid_pulldown(const uint16_t* row, uint32_t x) {
    const uint8_t* p = (const uint8_t*)row + row[x >> 4] + ((x >> 1) & 7u) * 3u;
    return x & 1u ? (uint32_t)(p[1] >> 4) | ((uint32_t)p[2] << 4)
                  : (uint32_t)p[0] | ((uint32_t)(p[1] & 0x0f) << 8);
}

/* The pulldown row for a waveform nibble (wf = control >> 4), or -1 for none: noise
 * combinations behave as the same waveform without noise, and noise with pulse only
 * has its own row. */
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

/* The register bits an output pulls down: the inverse of sid_noise_output, a nibble at a
 * time. On the device in .data (RAM), not with the other constant tables in flash
 * (platform/fm1/app.ld): the audio path reads it every cycle while noise is combined,
 * which makes the SID's most expensive block (decision record 0004, "Worst blocks"). */
#define SID_WB_LO(n)                                                                       \
    ((((n) & 8u) ? 1u << 13 : 0u) | (((n) & 4u) ? 1u << 17 : 0u) |                          \
     (((n) & 2u) ? 1u << 20 : 0u) | (((n) & 1u) ? 1u << 22 : 0u))
#define SID_WB_HI(n)                                                                       \
    ((((n) & 8u) ? 1u << 2 : 0u) | (((n) & 4u) ? 1u << 4 : 0u) | (((n) & 2u) ? 1u << 8 : 0u) | \
     (((n) & 1u) ? 1u << 11 : 0u))
#define SID_WB_ROW(f)                                                                      \
    {f(0u), f(1u), f(2u),  f(3u),  f(4u),  f(5u),  f(6u),  f(7u),                            \
     f(8u), f(9u), f(10u), f(11u), f(12u), f(13u), f(14u), f(15u)}
#ifdef __PI32V2__
__attribute__((section(".data.sid_noise_wb")))
#endif
static const uint32_t SID_NOISE_WB[2][16] = {SID_WB_ROW(SID_WB_LO), SID_WB_ROW(SID_WB_HI)};

static inline uint32_t sid_noise_writeback(uint32_t out) {
    return SID_NOISE_WB[0][(out >> 4) & 15u] | SID_NOISE_WB[1][(out >> 8) & 15u];
}

/* reSIDfp's do_writeback: whether a shift's second phase takes the output's bits. */
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

/* The second phase of a shift: the latched bits move up one, and the new bit is
 * (bit 0 | TEST or reset) ^ bit 5 of the latch. */
static inline void sid_noise_shift_phase2(sid_osc* o, uint32_t wf_old, uint32_t wf_new) {
    if (sid_noise_do_writeback(wf_old, wf_new, o->is6581)) {
        o->latch = (o->sr & ~SID_NOISE_TAPS) | sid_noise_writeback(o->out);
    }
    uint32_t bit = ((o->test_or_reset | o->latch) ^ (o->latch >> 5)) & 1u;
    o->sr = (o->latch >> 1) | (bit << 22);
    o->noise = sid_noise_output(o->sr);
}

/* Output shortcuts for single waveforms, which need no table (sid_osc_output). */
enum { SID_OUT_ANY, SID_OUT_NONE, SID_OUT_TRI, SID_OUT_SAW, SID_OUT_PULSE, SID_OUT_NOISE };

/* Recomputes the fields derived from the control register and the tables. */
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
        /* The fast path keeps these 0 (sid_osc_advance). */
        o->cycle_mode = 0;
        o->pulled_last = 0;
    }
}

/* The pulldown tables of a combined-waveform profile (SID_PROFILE_* in gen/grid.h;
 * anything else, or a profile not compiled in, gets AVERAGE). Swapping them is a few
 * stores, safe between two sid_render calls. */
static inline void sid_osc_set_profile(sid_osc* o, uint32_t profile) {
    if (profile >= SID_PROFILES) {
        profile = SID_PROFILE_AVERAGE;
    }
    o->pulldown = o->is6581
                      ? &SID_PD.idx[5u * SID_COMBINED_TABLE_6581(profile)]
                      : &SID_PD.idx[SID_PD_8580_ROW + 5u * SID_COMBINED_TABLE_8580(profile)];
    sid_osc_update_mode(o);
}

static inline void sid_osc_set_model(sid_osc* o, int is6581) {
    o->is6581 = (uint8_t)(is6581 != 0);
    sid_osc_set_profile(o, SID_PROFILE_AVERAGE);
}

/* Like the chip's reset, this keeps the accumulator ("accumulator is not changed on
 * reset", reSIDfp) and the model. The noise register is set to all ones, and the
 * released reset shifts it once, which clears the new bit. sid_init sets
 * SID_OSC_POWER_ON_ACC and the model first. */
static inline void sid_osc_reset(sid_osc* o) {
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

/* The waveform from the accumulator's top 12 bits, before pulse, noise and pulldown:
 * reSIDfp's wave tables (none, triangle, sawtooth, saw & saw << 1). */
static inline uint32_t sid_wave_ramp(uint32_t wf, uint32_t ix) {
    switch (wf & 3u) {
    case 1: return ((ix & 0x800u) ? ix ^ 0xfffu : ix) << 1 & 0xfffu;
    case 2: return ix;
    case 3: return ix & (ix << 1);
    default: return 0xfffu;
    }
}

/* The accumulator's top 12 bits as the triangle sees them: with ring modulation (and
 * no sawtooth), the MSB is inverted while the source's MSB is low. */
static inline uint32_t sid_wave_index(uint32_t control, uint32_t acc, uint32_t src_acc) {
    if ((control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING) {
        acc ^= ~src_acc & 0x800000u;
    }
    return (acc >> 12) & 0xfffu;
}

/* The ring source's accumulator k cycles after src_acc, as o sees it. */
static inline uint32_t sid_src_at(const sid_osc* o, uint32_t src_acc, uint32_t src_freq,
                                  uint32_t k) {
    return (src_acc + k * src_freq) & o->src_mask;
}

/* reSIDfp's waveform output from a triangle/saw value (sid_wave_ramp), pulse
 * comparison and noise: the AND of the selected waveforms, through the pulldown table. */
static inline uint32_t sid_osc_combine_ramp(const sid_osc* o, uint32_t ramp, uint32_t pulse,
                                            uint32_t noise) {
    uint32_t x = ramp & (o->no_pulse | pulse) & (o->no_noise | noise) & o->wf_mask;
    return o->pd_row ? sid_pulldown(o->pd_row, x) : x;
}

/* The same from the accumulator's top bits. */
static inline uint32_t sid_osc_combine(const sid_osc* o, uint32_t ix, uint32_t pulse,
                                       uint32_t noise) {
    return sid_osc_combine_ramp(o, sid_wave_ramp(o->ramp_kind, ix), pulse, noise);
}

/* The pulse comparison as reSIDfp uses it in a cycle: the previous cycle's accumulator
 * against the pulse width, high while TEST is set. */
static inline uint32_t sid_osc_pulse(const sid_osc* o, uint32_t prev_acc) {
    return ((o->control & SID_CTRL_TEST) || (prev_acc >> 12) >= o->pw) ? 0xfffu : 0u;
}

/* The accumulator in the last cycle before its MSB was pulled low, and in the cycle
 * before. Exact unless the frequency, TEST or a hard sync changed in those cycles, or
 * the MSB was pulled low in the cycle before last. */
static inline uint32_t sid_osc_acc_last(const sid_osc* o) {
    return o->pulled_last ? o->acc | 0x800000u : o->acc;
}

static inline uint32_t sid_osc_acc_before(const sid_osc* o) {
    return o->control & SID_CTRL_TEST ? o->acc : (sid_osc_acc_last(o) - o->freq) & 0xffffffu;
}

/* The noise register after `n` plain shifts (no TEST): a loop for a few, else the
 * 2^k-shift matrices of gen/noise_jump.h for the set bits of n mod 2^23 - 1, the
 * register's period. */
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

/* With noise not selected, or the register locked up at 0, sid_osc_advance only counts
 * bit-19 rises (`defer`). This applies them: every counted rise is a shift, except one
 * in the last two cycles, which is still in the pipeline (found from the accumulator
 * and frequency of those cycles, which nothing has changed: writes and hard sync
 * settle first). Exact, because nothing writes back into the register meanwhile (a
 * locked-up register's output is 0, and so is all it would write back). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_noise_settle_deferred(sid_osc* o) {
    o->defer = 0;
    uint32_t a0 = sid_osc_acc_last(o) + 0x2000000u; /* unwrapped, with headroom */
    uint32_t a1 = a0 - o->freq;
    uint32_t a2 = a1 - o->freq;
    uint32_t pending = 0;
    if (((a0 + 0x80000u) >> 20) != ((a1 + 0x80000u) >> 20)) {
        pending = 2; /* rose in the last cycle */
    } else if (((a1 + 0x80000u) >> 20) != ((a2 + 0x80000u) >> 20)) {
        pending = 1; /* rose in the cycle before: the first phase ran */
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

static inline void sid_noise_settle(sid_osc* o) {
    if (o->defer) {
        sid_noise_settle_deferred(o);
    }
}

/* The 8580's triangle/saw latch for OSC3 (reSIDfp's tri_saw_pipeline): every cycle
 * with triangle or saw selected it takes that cycle's value, and it holds otherwise.
 * Call before a control write, with the ring source's accumulator in the last cycle:
 * it latches the old waveform's value (cycle by cycle it is kept current anyway). */
static inline void sid_osc_latch_ramp(sid_osc* o, uint32_t src_acc) {
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!o->is6581 && (wf & 3u)) {
        o->tri_saw = sid_wave_ramp(wf, sid_wave_index(o->control, sid_osc_acc_last(o), src_acc));
    }
}

/* A frequency write: the deferred noise shifts were timed with the old frequency. */
static inline void sid_osc_write_freq(sid_osc* o, uint32_t freq) {
    sid_noise_settle(o);
    o->freq = freq & 0xffffu;
    o->quiet = 0;
}

/* A pulse width write, after latching the comparison made with the old width. */
static inline void sid_osc_write_pw(sid_osc* o, uint32_t pw) {
    if (!o->pulse_held) {
        o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
        o->pulse_held = 1;
    }
    o->pw = pw & 0xfffu;
    o->quiet = 0;
}

/* As reSIDfp's writeCONTROL_REG. Setting TEST zeroes the accumulator, drops a pending
 * shift, latches the register and starts its fade delay; releasing TEST runs the
 * second phase of a shift, with the new bit forced to 1. */
static inline void sid_osc_write_control(sid_osc* o, uint8_t v) {
    sid_noise_settle(o);
    uint32_t wf_old = o->control >> 4;
    uint32_t test_old = o->control & SID_CTRL_TEST;
    if (!o->pulse_held) {
        /* reSIDfp's pulse_output: compared at the end of the last cycle, with the pulse
         * width of then; noise combined with other waveforms starts from it. */
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

/* With TEST set, `cycles` cycles of the register's fade: after the delay, every fade
 * period one more bit becomes 1, from the input end, until all are. */
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

/* One cycle, exactly as reSIDfp's clock() then output(), for noise combined with other
 * waveforms, where the output writes back into the register every cycle. src_acc is
 * the ring modulation source's accumulator in this cycle. */
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
            o->overshoot += o->freq; /* hard sync wants it counted to the end of the step */
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
        /* The mode's fields (sid_osc_update_mode) stand for wf: pd_row its pulldown row,
         * ramp_kind its triangle/saw, no_noise and no_pulse what it leaves out. */
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
                o->sr &= ~SID_NOISE_TAPS | sid_noise_writeback(o->out);
                o->noise &= o->out;
            } else {
                o->noise = o->out;
            }
        }
    }
    o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
}

/* 6581, sawtooth combined with another waveform: `cycles` cycles from an accumulator
 * `a`, pulling its MSB low whenever the output's top bit is low in a cycle where the
 * MSB is high. Entering the upper half from below, the output's top bit is always low
 * (the unit tests check the tables), so the MSB never survives its rise and only the
 * accumulator's low 23 bits move on. Starting a step in the upper half (right after
 * a control write) takes a cycle-by-cycle look until the MSB drops. */
static inline uint32_t sid_osc_pull_msb(sid_osc* o, uint32_t a, uint32_t cycles,
                                        uint32_t src_acc) {
    int first = 1;
    while (cycles && (a & 0x800000u)) {
        uint32_t before = a;
        a = (a + o->freq) & 0xffffffu;
        --cycles;
        if (!(a & 0x800000u)) {
            break; /* wrapped into the lower half */
        }
        /* The first cycle compares with the pulse width before any write since. */
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

/* The noise register over a step from prev to next (not wrapped) when nothing writes
 * back into it (sid_osc_advance's fast path): a shift is the plain LFSR step, and only
 * a shift still pending at the end needs its latch and TEST-or-reset flag. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void sid_noise_clock(sid_osc* o, uint32_t prev, uint32_t next, uint32_t cycles) {
    uint32_t f = o->freq;
    /* A pending shift's phases, cycle by cycle: a rise in one of them restarts the
     * pipeline and drops the pending phase (possible only right after a hard sync). */
    while (o->shift_pipeline && cycles) {
        uint32_t a = prev + f;
        if (((a + 0x80000u) ^ (prev + 0x80000u)) >> 20) {
            o->shift_pipeline = 2;
        } else if (o->shift_pipeline == 2) {
            o->test_or_reset = 0; /* phase 1 */
            o->latch = o->sr;
            o->shift_pipeline = 1;
        } else {
            uint32_t l = o->latch; /* phase 2: nothing writes back without noise */
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
    /* Each further rise shifts two cycles later; one in the last cycle or the one
     * before stays pending. */
    uint32_t pending = 0;
    uint32_t last = next - f;
    if (((next + 0x80000u) >> 20) != ((last + 0x80000u) >> 20)) {
        pending = 2;
    } else if (cycles >= 2 && ((last + 0x80000u) >> 20) != ((last - f + 0x80000u) >> 20)) {
        pending = 1;
    }
    uint32_t shifts = rises - (pending ? 1u : 0u);
    if (!(o->sr | o->latch | o->noise | o->test_or_reset)) {
        /* All zero: shifts move in 0s and change nothing but the pipeline. */
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

/* The bit-19 segment of an (unwrapped) accumulator value: a rise moves to the next. */
static inline uint32_t sid_osc_seg19(uint32_t a) {
    return (a + 0x80000u) >> 20;
}

/* The cycle, counted from acc, in which bit 19 next rises (0xffffffff never). */
static inline uint32_t sid_osc_cycles_to_rise(uint32_t acc, uint32_t f) {
    if (!f) {
        return 0xffffffffu;
    }
    uint32_t rise = ((acc + 0x80000u) | 0xfffffu) + 1u - 0x80000u;
    return (rise - acc + f - 1u) / f;
}

/* Where bit 19 rises in a window of `left` cycles from prev to next (not wrapped): 0 not
 * at all, 2 in its last cycle, 1 in the cycle before (so the last cycle is the shift's
 * first phase), 3 earlier (the shift's second phase is inside the window). Rises are at
 * least 16 cycles apart. */
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

/* After a window that ends with a shift pending (sid_osc_rise_phase's phase): its rise
 * cycle is like any other but starts the pipeline, and its first phase latches the
 * register, which writes nothing back in that cycle (the output, the noise, keeps it). */
static inline void sid_osc_bulk_pipeline(sid_osc* o, uint32_t phase) {
    /* sid_osc_bulk_limit ends every window by the shift's first phase: the second (phase
     * 3) changes the noise and must run as a cycle, which this would skip. */
    SID_DEBUG_ASSERT(phase != 3u);
    if (phase == 2u) {
        o->shift_pipeline = 2;
    } else if (phase == 1u) {
        o->shift_pipeline = 1;
        o->test_or_reset = 0;
        o->latch = o->sr;
    }
}

/* Noise combined with other waveforms, TEST clear: runs `left` cycles at once, a window
 * sid_osc_bulk_limit accepted, in which nothing changes but the accumulator (and, locked
 * up, the shift pipeline). That holds when the register and noise are 0 (locked up:
 * every shift moves in a 0), or when each cycle's output equals the noise: the pulldown
 * keeps the noise as it is, and the other waveforms and the pulse cover all its bits. A
 * window may end on a bit-19 rise or on the shift's first phase (sid_osc_rise_phase);
 * the second phase changes the noise and runs as a cycle. */
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
        /* The output is 0 and writes back nothing but clearing the register's output
         * bits; the accumulator moves (the 6581 pulls its MSB at every rise), and
         * locked up, shifts move 0s (sid_noise_clock's plain shift is then exact). */
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
                o->overshoot += f * left; /* rose earlier in this step */
            }
            o->acc = next & 0xffffffu;
        }
        if (wf & 3u) { /* reSIDfp updates the 8580's latch only with triangle/saw */
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
    /* Every cycle outputs the noise unchanged. */
    if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    } else if (o->msb_rose) {
        o->overshoot += f * left; /* rose earlier in this step */
    }
    if (!o->is6581 && (wf & 3u) && left == 1) {
        uint32_t x = o->tri_saw & (o->no_pulse | o->pulse) & n;
        o->osc3 = o->pd_row ? sid_pulldown(o->pd_row, x) : x;
    } else {
        o->osc3 = n;
    }
    o->acc = next & 0xffffffu;
    o->pulled_last = 0;
    o->sr &= ~SID_NOISE_TAPS | sid_noise_writeback(n);
    o->out = n;
    if (wf & 3u) {
        o->tri_saw = sid_wave_ramp(wf, sid_wave_index(o->control, o->acc,
                                                      sid_src_at(o, src_acc, src_freq, left)));
    }
    o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
    sid_osc_bulk_pipeline(o, phase);
}

/* How far sid_osc_run_combined looks ahead for a quiet stretch. */
#define SID_OSC_QUIET_HORIZON 4096u

/* The longest window, at most `left` cycles, that sid_osc_bulk_commit can run, found
 * directly: the cycle after the next bit-19 rise (the shift's first phase; its second
 * changes the noise), of the MSB rising (6581, pulled low),
 * of the pulse wrapping or the ring source's MSB changing, and, walking from one
 * candidate to the next, of the first cycle whose waveform no longer covers the
 * noise. With the noise covered by the bits `need` of the (possibly inverted) index,
 * the next index that can fail is (x | (lowest bit of need - 1)) + 1, within a half. */
static inline uint32_t sid_osc_bulk_limit(const sid_osc* o, uint32_t left, uint32_t src_acc,
                                          uint32_t src_freq) {
    uint32_t f = o->freq;
    uint32_t prev = o->acc;
    uint32_t n = o->noise;
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!n && !o->sr && !o->test_or_reset && !(o->shift_pipeline == 1 && o->out)) {
        return left; /* locked up */
    }
    if (o->shift_pipeline) {
        return 0;
    }
    if (!f) {
        /* The accumulator stands still: one index decides (the ring source may still
         * move, so leave that case to the range check). */
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
            /* The first cycle compares as stored, the others with the accumulator. */
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
    uint32_t k = sid_osc_cycles_to_rise(prev, f); /* bit 19 rises in cycle k */
    if (k + 1u < lim) {
        lim = k + 1u; /* through the rise and the shift's first phase (sid_osc_bulk_pipeline) */
    }
    if (o->is6581 && (wf & 2u) && !(n & 0x800u)) {
        if (prev & 0x800000u) {
            return 0;
        }
        k = (0x800000u - prev + f - 1u) / f; /* the MSB rises (and is pulled) in cycle k */
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
            k = (0x1000000u - prev + f - 1u) / f; /* the accumulator wraps in cycle k */
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
            uint32_t j = (edge - s1 + src_freq - 1u) / src_freq; /* changes in cycle 1 + j */
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
            return kk - 1u; /* cycles before kk are shown; enough for one call */
        }
        uint32_t x = (prev + kk * f) >> 12; /* the index in cycle kk, not wrapped */
        uint32_t xf = (x & 0xfffu) ^ flip;
        if ((sid_wave_ramp(kind, xf) & n) != n) {
            return kk - 1u;
        }
        uint32_t y = (x | 0x7ffu) + 1u; /* the next half */
        if (need) {
            uint32_t cand = x + (((xf | (low - 1u)) + 1u) - xf);
            if (cand < y) {
                y = cand;
            }
        }
        uint32_t next = ((y << 12) - prev + f - 1u) / f; /* the first cycle at or past y */
        kk = next > kk ? next : kk + 1u;
    }
    return lim;
}

/* Noise combined with other waveforms, TEST set: the accumulator stands still and the
 * pulse is high, so the output feeds back into the noise until it settles at a fixed
 * point, and from there only the register's fade (every few thousand cycles) changes
 * anything. Returns how many of the `left` cycles it ran at once: up to the cycle
 * before the next fade, or 0 if the state is not settled. */
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
    uint32_t pulse = o->pw ? 0u : 0xfffu; /* (0 >> 12) >= pw */
    if (out != n || o->out != n || (!o->is6581 && (wf & 3u) && o->tri_saw != ramp) ||
        o->osc3 != osc3 ||
        o->pulse != pulse || !o->test_or_reset) {
        return 0;
    }
    uint32_t run = left;
    if (o->sr_reset && o->sr_reset <= left) {
        run = o->sr_reset - 1; /* stop before the fade */
    }
    if (o->sr_reset) {
        o->sr_reset -= run;
    }
    o->pulled_last = 0;
    return run;
}

/* `cycles` cycles of noise combined with other waveforms (or TEST with noise and
 * more), cycle by cycle where something changes and in bulk where provably nothing
 * but the accumulator or the fade countdown does. Exact either way. */
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
            /* A shift in progress: its cycles run one at a time. */
            ++k;
            sid_osc_cycle(o, sid_src_at(o, src_acc, src_freq, k));
            continue;
        }
        if (test) {
            done = sid_osc_test_bulk(o, left, s, src_freq);
            int ring = (o->control & (SID_CTRL_RING | SID_CTRL_SAW)) == SID_CTRL_RING;
            if (done == left && o->sr_reset != 1 && (!ring || !src_freq)) {
                /* Settled: nothing changes until the cycle before the next fade (with
                 * ring modulation, while the source stands still). */
                o->quiet = o->sr_reset ? o->sr_reset - 1u : 0xffffffffu;
                o->quiet_src = sid_src_at(o, src_acc, src_freq, cycles);
                o->quiet_src_freq = src_freq;
            }
        } else {
            /* Look ahead past this step and remember how long the state stays quiet.
             * With ring modulation that assumes the source keeps going as it does now;
             * the quiet path checks that it did. */
            uint32_t w = sid_osc_bulk_limit(o, SID_OSC_QUIET_HORIZON, s, src_freq);
            uint32_t run = w < left ? w : left;
            if (run) {
                sid_osc_bulk_commit(o, run, s, src_freq);
                done = run;
                if (w > left && !(!o->noise && !o->sr && !o->test_or_reset) &&
                    !o->shift_pipeline) {
                    /* Not through the next rise, which the quiet path would not start. */
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

/* For tests: sid_osc_advance for noise combined with other waveforms, but strictly
 * cycle by cycle (the reference for sid_osc_run_combined's bulk steps). */
static inline void sid_osc_advance_cycles(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                          uint32_t src_freq) {
    o->msb_rose = 0;
    o->cycle_mode = 1;
    for (uint32_t k = 1; k <= cycles; ++k) {
        sid_osc_cycle(o, sid_src_at(o, src_acc, src_freq, k));
    }
}

/* Within a quiet stretch (sid_osc_run_combined) of noise combined with other waveforms:
 * the output stays the noise (TEST: settled), only the accumulator moves (TEST: the fade
 * counts down). With ring modulation (and no sawtooth) only while the source keeps the
 * course the stretch assumed. Returns 0, doing nothing, outside one. */
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

/* sid_osc_advance for TEST, noise combined with other waveforms and the 6581's sawtooth
 * combinations, and for steps where the noise register shifts. */
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
        /* Noise combined with other waveforms writes the output back into the register
         * every cycle (sid_osc_run_combined). It is entered only after a control write,
         * which latched the last cycle's pulse comparison and the 8580's triangle/saw
         * (sid_osc_latch_ramp). */
        o->cycle_mode = 1;
        /* Not within a quiet stretch (sid_osc_quiet_step). */
        o->quiet = 0;
        if (!(o->sr | o->noise | o->test_or_reset | (o->control & SID_CTRL_TEST)) &&
            !(o->shift_pipeline == 1 && o->out)) {
            /* (A shift whose first phase ran before a control write takes the old
             * waveform's output in its second: not locked up yet.) */
            /* Locked up: the common case, sid_osc_bulk_commit's first branch without
             * its checks. */
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
            /* Locked up for good (until a control write): the inline path does it,
             * shifting 0s, and the output and OSC3 computed from the state are 0; on the
             * 6581 with sawtooth that output pulls every MSB rise low (pull_low). */
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
    uint32_t next = prev + o->freq * cycles; /* not wrapped */

    if (o->shift_pipeline || ((next + 0x80000u) ^ (prev + 0x80000u)) >> 20) {
        sid_noise_clock(o, prev, next, cycles); /* no noise combined: no write-back */
    }

    if (o->is6581 && (wf & 2u) && wf != 2u) {
        o->acc = sid_osc_pull_msb(o, prev, cycles, src_acc);
        if (!(o->acc & 0x800000u)) {
            o->slow = 0; /* from here every rise is pulled at once: inline (pull_low) */
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

/* Advances `cycles` (at most 256, and freq * cycles < 2^24). src_acc and src_freq are
 * the ring modulation source's accumulator at the start and its frequency; only noise
 * combined with other waveforms uses them, cycle by cycle, taking the source's
 * accumulator as src_acc + k * src_freq (masked by src_mask for a source that pulls
 * every rise low): wrong for up to the rest of the step if, within it, the source had
 * TEST set, was hard-synced, or for voice 1, had its MSB pulled low (voice 1 sees the
 * source before the pull, which this does not model). A 6581 MSB pulled low cycle by
 * cycle also clears msb_rose, so a hard sync from a rise earlier in the same step is
 * dropped. Inline, the common case:
 * the accumulator moves, and the noise register shifts as an LFSR. */
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
            o->owed += rises; /* count now, apply when needed (sid_noise_settle) */
        } else if (!o->shift_pipeline &&
                   (o->no_noise || !(o->sr | o->latch | o->noise | o->test_or_reset))) {
            /* Noise not selected, or a locked-up (all-zero) register: nothing reads or
             * writes back into the register until a write, so defer the shifts. */
            o->defer = 1;
            o->owed = rises;
        } else {
            sid_noise_clock(o, prev, next, cycles);
        }
    }
    o->msb_rose = 0;
    if (o->pull_low) {
        /* Starting in the lower half, each rise is pulled low as it happens. */
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

/* The waveform output of the last cycle, reSIDfp's waveform_output: what the voice's
 * DAC sees, and OSC3 on the 6581. src_acc is the ring source's accumulator as this
 * voice's output sees it (after the source's own MSB pull for voices 2 and 3, before
 * it for voice 1: reSIDfp computes the outputs in voice order). */
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
        return 0; /* noise selected and all 0: so is the AND, and pulldown keeps 0 */
    }
    uint32_t acc = sid_osc_acc_last(o);
    uint32_t ix = sid_wave_index(o->control, acc, src_acc);
    uint32_t pulse = 0;
    if (!o->no_pulse) {
        pulse = sid_osc_pulse(o, sid_osc_acc_before(o));
    }
    return sid_osc_combine(o, ix, pulse, o->noise);
}

/* Hard sync: restart o as if it had been reset at the moment src's MSB rose.
 * Call only when src->msb_rose is set, which implies src->freq != 0.
 * Known simplifications, deliberate for M0 (inaudible): o's own noise clocks and
 * msb_rose from the discarded part of the step are not recomputed, and reSID's
 * special case of a sync source that is itself being synced is not modelled. */
static inline void sid_osc_sync(sid_osc* o, const sid_osc* src) {
    if (o->control & SID_CTRL_TEST) {
        return; /* TEST holds the accumulator at 0 (reSIDfp skips it too) */
    }
    sid_noise_settle(o); /* while the accumulator still says when bit 19 rose */
    uint32_t cycles_after = src->overshoot / src->freq;
    o->acc = (o->freq * cycles_after) & 0xffffffu;
    o->pulled_last = 0;
    o->quiet = 0;
    if (o->pull_low) {
        o->pull_low = 0; /* the restart may land in the upper half: the slow path */
        o->slow = 1;
    }
}

#endif
