// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_OSC_H
#define SIDOU_SID_OSC_H

#include <stdint.h>

#include "gen/grid.h"
#include "gen/wave_6581.h"
#include "gen/wave_8580.h"

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
 * generated from reSIDfp (gen/wave_*.h, rows ST, PT, PS, PST and NP), one per point of
 * the combined-waveform strength grid (gen/grid.h; reSIDfp's AVERAGE by default). On the 6581, a
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
    uint32_t acc;
    uint32_t freq;
    uint32_t pw;
    uint32_t sr;       /* noise shift register */
    uint32_t latch;    /* noise shift latch */
    uint32_t noise;    /* noise waveform (12-bit); combined waveforms pull it down */
    uint32_t out;      /* waveform output of the last cycle (12-bit) */
    uint32_t osc3;     /* combined with noise: OSC3 of the last cycle */
    uint32_t pulse;    /* combined with noise: the pulse comparison of the last cycle */
    uint32_t tri_saw;  /* combined with noise, 8580: triangle/saw of the last cycle */
    uint32_t sr_reset; /* with TEST: cycles until the register fades a bit; 0 for none */
    uint32_t overshoot; /* accumulator units past the last MSB rise, for hard sync */
    const uint8_t (*pulldown)[6144]; /* the model's pulldown rows */
    uint8_t control;
    uint8_t msb_rose;
    uint8_t shift_pipeline; /* cycles until the next phase of a pending shift */
    uint8_t test_or_reset;  /* forces the new bit to 1 in the next shift */
    uint8_t is6581;
    uint8_t pulled_last; /* 6581: the MSB was pulled low in the last cycle */
    uint8_t cycle_mode;  /* combined with noise: the last step ran cycle by cycle */
    uint8_t wf_last;     /* the waveform (control >> 4) before the last control write */
    /* Derived from control and the pulldown tables (sid_osc_update_mode). */
    uint8_t ramp_kind;   /* control bits 4-5: none, triangle, sawtooth, both */
    uint8_t out_kind;    /* SID_OUT_*: a single waveform, or any other combination */
    uint8_t slow;        /* sid_osc_advance needs sid_osc_advance_slow */
    uint32_t no_pulse;   /* 0xfff without pulse, else 0 */
    uint32_t no_noise;   /* 0xfff without noise, else 0 */
    uint32_t wf_mask;    /* 0xfff with any waveform, else 0 */
    const uint8_t* pd_row; /* the pulldown row, or 0 for none */
} sid_osc;

/* The accumulator at power-on, as in reSIDfp: its even bits come up high. */
#define SID_OSC_POWER_ON_ACC 0x555555u

/* reSIDfp's delays for the register's fade while TEST is set (6581R3, 8580R5). */
#define SID_NOISE_RESET_6581 50000u
#define SID_NOISE_FADE_6581 15000u
#define SID_NOISE_RESET_8580 986000u
#define SID_NOISE_FADE_8580 314300u

/* Entry x of a packed pulldown row (see gen/wave_6581.h). */
static inline uint32_t sid_pulldown(const uint8_t* row, uint32_t x) {
    const uint8_t* p = row + (x >> 1) * 3;
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

/* The register bits an output pulls down: the inverse of sid_noise_output. */
static inline uint32_t sid_noise_writeback(uint32_t out) {
    return ((out & (1u << 11)) >> 9) | ((out & (1u << 10)) >> 6) | ((out & (1u << 9)) >> 1) |
           ((out & (1u << 8)) << 3) | ((out & (1u << 7)) << 6) | ((out & (1u << 6)) << 11) |
           ((out & (1u << 5)) << 15) | ((out & (1u << 4)) << 18);
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

/* The pulldown tables at a point of the strength grid (gen/grid.h). Swapping them is
 * a few stores, safe between two sid_render calls. */
static inline void sid_osc_set_strength(sid_osc* o, uint32_t point) {
    if (point >= SID_STRENGTH_POINTS) {
        point = SID_STRENGTH_DEFAULT;
    }
    o->pulldown = o->is6581 ? SID_PULLDOWN_6581[point] : SID_PULLDOWN_8580[point];
    sid_osc_update_mode(o);
}

static inline void sid_osc_set_model(sid_osc* o, int is6581) {
    o->is6581 = (uint8_t)(is6581 != 0);
    sid_osc_set_strength(o, SID_STRENGTH_DEFAULT);
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
    o->sr = 0x7fffffu;
    o->test_or_reset = 1;
    o->latch = o->sr;
    sid_noise_shift_phase2(o, 0, 0);
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

/* As reSIDfp's writeCONTROL_REG. Setting TEST zeroes the accumulator, drops a pending
 * shift, latches the register and starts its fade delay; releasing TEST runs the
 * second phase of a shift, with the new bit forced to 1. */
static inline void sid_osc_write_control(sid_osc* o, uint8_t v) {
    uint32_t wf_old = o->control >> 4;
    uint32_t test_old = o->control & SID_CTRL_TEST;
    o->wf_last = (uint8_t)wf_old;
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
        uint32_t noise = wf & 8u ? o->noise : 0xfffu;
        uint32_t pulse = wf & 4u ? o->pulse : 0xfffu;
        int row = sid_pulldown_row(wf);
        uint32_t x = sid_wave_ramp(wf, ix) & pulse & noise;
        o->out = row < 0 ? x : sid_pulldown(o->pulldown[row], x);
        if (!o->is6581 && (wf & 3u)) {
            x = o->tri_saw & pulse & noise;
            o->osc3 = row < 0 ? x : sid_pulldown(o->pulldown[row], x);
            o->tri_saw = sid_wave_ramp(wf, ix);
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
    while (cycles && (a & 0x800000u)) {
        uint32_t before = a;
        a = (a + o->freq) & 0xffffffu;
        --cycles;
        if (!(a & 0x800000u)) {
            break; /* wrapped into the lower half */
        }
        uint32_t out = sid_osc_combine(o, sid_wave_index(o->control, a, src_acc),
                                       sid_osc_pulse(o, before), o->noise);
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

/* sid_osc_advance for TEST, noise combined with other waveforms and the 6581's sawtooth
 * combinations, and for steps where the noise register shifts. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static inline void sid_osc_advance_slow(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq) {
    uint32_t wf = (uint32_t)o->control >> 4;
    o->msb_rose = 0;
    if (wf > 8u) {
        /* Noise combined with other waveforms writes the output back into the register
         * every cycle. Once register and noise are 0 (locked up), the output is 0 and
         * nothing depends on the cycle any more. */
        if (!o->cycle_mode) {
            o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
            uint32_t ix = sid_wave_index(o->control, sid_osc_acc_last(o), src_acc);
            o->tri_saw = sid_wave_ramp(o->wf_last, ix);
        }
        if (o->sr | o->noise | (o->control & SID_CTRL_TEST)) {
            o->cycle_mode = 1;
            uint32_t prev = o->acc;
            uint32_t next = prev + o->freq * cycles;
            if (!(o->control & SID_CTRL_TEST) && !o->noise && !(o->sr & SID_NOISE_TAPS) &&
                !o->shift_pipeline && !(((next + 0x80000u) ^ (prev + 0x80000u)) >> 20)) {
                /* Noise and the register's output bits are 0 and no shift is due: the
                 * output is 0 and writes nothing back, so only the accumulator moves
                 * (the register's other bits wait for the next shift). */
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
                uint32_t ix = sid_wave_index(o->control, sid_osc_acc_last(o),
                                             (src_acc + cycles * src_freq) & 0xffffffu);
                o->tri_saw = sid_wave_ramp(wf, ix);
                o->pulse = (o->acc >> 12) >= o->pw ? 0xfffu : 0u;
                o->out = 0;
                o->osc3 = 0;
                return;
            }
            for (uint32_t k = 1; k <= cycles; ++k) {
                sid_osc_cycle(o, (src_acc + k * src_freq) & 0xffffffu);
            }
            return;
        }
    }
    o->cycle_mode = 0;
    o->pulled_last = 0;
    if (wf > 8u) {
        o->out = 0; /* locked up: the output a shift's write-back takes is 0 */
    }
    if (o->control & SID_CTRL_TEST) {
        sid_noise_fade(o, cycles);
        return;
    }
    if (!cycles) {
        return;
    }
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles; /* not wrapped */

    /* The noise register. A pending shift finishes first (rises are at least 16 cycles
     * apart, so none can interfere); then each rise of bit 19 shifts it two cycles
     * later, and a rise in the last cycle or the one before stays pending. */
    if (o->shift_pipeline == 2) {
        o->test_or_reset = 0;
        o->latch = o->sr;
        o->shift_pipeline = 1;
        if (cycles >= 2) {
            sid_noise_shift_phase2(o, wf, wf);
            o->shift_pipeline = 0;
        }
    } else if (o->shift_pipeline == 1) {
        sid_noise_shift_phase2(o, wf, wf);
        o->shift_pipeline = 0;
    }
    uint32_t rises = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    if (rises) {
        uint32_t last = next - o->freq; /* the accumulator a cycle before the end */
        uint32_t pending = 0;
        if (((next + 0x80000u) >> 20) != ((last + 0x80000u) >> 20)) {
            pending = 2;
        } else if (cycles >= 2 &&
                   ((last + 0x80000u) >> 20) != ((last - o->freq + 0x80000u) >> 20)) {
            pending = 1;
        }
        for (uint32_t i = rises - (pending ? 1u : 0u); i; --i) {
            o->test_or_reset = 0;
            o->latch = o->sr;
            sid_noise_shift_phase2(o, wf, wf);
        }
        if (pending == 1) {
            o->test_or_reset = 0;
            o->latch = o->sr;
        }
        o->shift_pipeline = (uint8_t)pending;
    }

    if (o->is6581 && (wf & 2u) && wf != 2u) {
        o->acc = sid_osc_pull_msb(o, prev, cycles, src_acc);
        return;
    }
    if (((next + 0x800000u) >> 24) != ((prev + 0x800000u) >> 24)) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}

/* The noise register over a step from prev to next (not wrapped) when nothing writes
 * back into it (sid_osc_advance's fast path): a shift is the plain LFSR step, and only
 * a shift still pending at the end needs its latch and TEST-or-reset flag. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static inline void sid_noise_clock(sid_osc* o, uint32_t prev, uint32_t next, uint32_t cycles) {
    if (!cycles) {
        return;
    }
    if (o->shift_pipeline == 2 && cycles == 1) {
        o->test_or_reset = 0; /* phase 1 */
        o->latch = o->sr;
        o->shift_pipeline = 1;
        return;
    }
    uint32_t shifts = o->shift_pipeline ? 1u : 0u;
    uint32_t pending = 0;
    uint32_t rises = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    if (rises) {
        uint32_t last = next - o->freq;
        if (((next + 0x80000u) >> 20) != ((last + 0x80000u) >> 20)) {
            pending = 2;
        } else if (cycles >= 2 &&
                   ((last + 0x80000u) >> 20) != ((last - o->freq + 0x80000u) >> 20)) {
            pending = 1;
        }
        shifts += rises - (pending ? 1u : 0u);
    }
    if (shifts) {
        uint32_t sr = o->shift_pipeline == 1 ? o->latch : o->sr;
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

/* Advances `cycles` (at most 256, and freq * cycles < 2^24). src_acc and src_freq are
 * the ring modulation source's accumulator at the start and its frequency; only noise
 * combined with other waveforms uses them, cycle by cycle. Inline, the common case:
 * the accumulator moves, and the noise register shifts as an LFSR. */
static inline void sid_osc_advance(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                   uint32_t src_freq) {
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;
    if (o->slow) {
        sid_osc_advance_slow(o, cycles, src_acc, src_freq);
        return;
    }
    if (o->shift_pipeline || ((next + 0x80000u) ^ (prev + 0x80000u)) >> 20) {
        sid_noise_clock(o, prev, next, cycles);
    }
    o->msb_rose = 0;
    if (((next + 0x800000u) ^ (prev + 0x800000u)) >> 24) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}

/* The waveform output of the last cycle, reSIDfp's waveform_output: what the voice's
 * DAC sees, and OSC3 on the 6581. src is the ring modulation source. */
static inline uint32_t sid_osc_output(const sid_osc* o, const sid_osc* src) {
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
    uint32_t acc = sid_osc_acc_last(o);
    uint32_t ix = sid_wave_index(o->control, acc, sid_osc_acc_last(src));
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
    uint32_t cycles_after = src->overshoot / src->freq;
    o->acc = (o->freq * cycles_after) & 0xffffffu;
    o->pulled_last = 0;
}

#endif
