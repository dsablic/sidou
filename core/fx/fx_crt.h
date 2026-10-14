// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CRT_H
#define SIDOU_FX_CRT_H

/* CRT/TV speaker: a small speaker's band-pass plus quiet mains hum, with separate amounts.
 *
 * - Tone (amount t): two cascaded one-pole low-passes y += a (x - y), each with its pole
 *   at f_lp = LP_MIN_HZ (LP_MAX_HZ / LP_MIN_HZ)^(1 - t) (log-frequency: equal knob
 *   travel, equal musical interval), into a one-pole high-pass at
 *   f_hp = HP_MIN_HZ (HP_MAX_HZ / HP_MIN_HZ)^t, the thin TV-speaker low end. Each pole
 *   is matched, a = 1 - e^-w (low-pass) and r = e^-w (high-pass) with w = 2 pi f / fs,
 *   so a pole sits at f; the low-pass cascade's -3 dB point is at 0.64 f_lp (about
 *   2.6 kHz at t = 1). The filtered signal is mixed in over the first WET_SPAN of
 *   travel (a smoothstep, no corner), so the tone is the identity at t = 0 although the
 *   filters are not. (Pulling the coefficients to 1 instead does not work: the high-pass
 *   then integrates and holds an offset that steps out when the block leaves.) The mix
 *   is the part most sensitive to how finely the tone moves, so the chain smooths this
 *   amount on every sample.
 * - Hum (amount h): 50 Hz with its 2nd and 3rd harmonics, fundamental at h HUM_LEVEL,
 *   read from a one-period wavetable built at init.
 *
 * The coefficients come from tables over tone, built at init on SID_FX_CRT_STEPS
 * log-spaced points and interpolated linearly between them, so any amounts in [0, 1]
 * may arrive on any sample at little cost. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CRT_LP_MIN_HZ 4000.0f
#define SID_FX_CRT_LP_MAX_HZ 18000.0f
#define SID_FX_CRT_LP_LN_RANGE 1.5040774f /* ln(18000 / 4000) */
#define SID_FX_CRT_HP_MIN_HZ 20.0f
#define SID_FX_CRT_HP_MAX_HZ 150.0f
#define SID_FX_CRT_HP_LN_RANGE 2.0149030f /* ln(150 / 20) */
#define SID_FX_CRT_WET_SPAN 0.25f /* tone over which the filtered signal fades in */
#define SID_FX_CRT_STEPS 32u /* table intervals over the tone range */
#define SID_FX_HUM_STEPS 64u /* hum wavetable points per period */
#define SID_FX_HUM_HZ 50.0f
#define SID_FX_HUM_LEVEL 0.003f /* fundamental peak at amount 1, full scale 1 (-50 dBFS) */
#define SID_FX_HUM_H2 0.5f      /* harmonic levels relative to the fundamental */
#define SID_FX_HUM_H3 0.3f

typedef struct {
    float a_lp_table[SID_FX_CRT_STEPS + 1u];
    float r_hp_table[SID_FX_CRT_STEPS + 1u];
    float hum_table[SID_FX_HUM_STEPS + 1u]; /* one period, fundamental peak 1 */
    float lp1, lp2;  /* low-pass states */
    float hp_x, hp_y; /* high-pass input and output states */
    uint32_t hum_phase, hum_inc;
    /* Derived from `tone`. */
    float tone, a_lp, r_hp, wet;
} sid_fx_crt;

static inline void sid_fx_crt_set_tone(sid_fx_crt* c, float tone) {
    c->tone = tone;
    float pos = tone * (float)SID_FX_CRT_STEPS;
    int32_t i = (int32_t)pos;
    if (i >= (int32_t)SID_FX_CRT_STEPS) {
        i = (int32_t)SID_FX_CRT_STEPS - 1;
    }
    float f = pos - (float)i;
    c->a_lp = sid_fx_lerp(c->a_lp_table[i], c->a_lp_table[i + 1], f);
    c->r_hp = sid_fx_lerp(c->r_hp_table[i], c->r_hp_table[i + 1], f);
    float w = tone * (1.0f / SID_FX_CRT_WET_SPAN);
    c->wet = w < 1.0f ? sid_fx_smoothstep(w) : 1.0f;
}

/* Re-entry: the filters start settled on the current input x, so nothing jumps. */
static inline void sid_fx_crt_reset(sid_fx_crt* c, float x) {
    c->lp1 = x;
    c->lp2 = x;
    c->hp_x = x;
    c->hp_y = x; /* the next output is r_hp times its input: continuous */
    c->hum_phase = 0;
    sid_fx_crt_set_tone(c, 0.0f);
}

static inline void sid_fx_crt_init(sid_fx_crt* c, uint32_t sample_rate) {
    float w_scale = 2.0f * SIDOU_PI / (float)sample_rate;
    float lp_limit_hz = 0.45f * (float)sample_rate;
    for (uint32_t i = 0; i <= SID_FX_CRT_STEPS; ++i) {
        float tone = (float)i / (float)SID_FX_CRT_STEPS;
        float f_lp =
            SID_FX_CRT_LP_MIN_HZ * sid_fx_exp_init(SID_FX_CRT_LP_LN_RANGE * (1.0f - tone));
        if (f_lp > lp_limit_hz) {
            f_lp = lp_limit_hz;
        }
        c->a_lp_table[i] = 1.0f - sid_fx_exp_init(-w_scale * f_lp);
        float f_hp = SID_FX_CRT_HP_MIN_HZ * sid_fx_exp_init(SID_FX_CRT_HP_LN_RANGE * tone);
        c->r_hp_table[i] = sid_fx_exp_init(-w_scale * f_hp);
    }
    for (uint32_t i = 0; i <= SID_FX_HUM_STEPS; ++i) {
        /* sin 2t = 2 s c, sin 3t = 3 s - 4 s^3 */
        uint32_t phase = i * (0xffffffffu / SID_FX_HUM_STEPS + 1u);
        float s = sid_fx_sin(phase);
        float co = sid_fx_cos(phase);
        c->hum_table[i] =
            s + SID_FX_HUM_H2 * 2.0f * s * co + SID_FX_HUM_H3 * (3.0f - 4.0f * s * s) * s;
    }
    c->hum_inc = sid_fx_phase_inc(SID_FX_HUM_HZ, sample_rate);
    sid_fx_crt_reset(c, 0.0f);
}

/* x is normalised to [-1, 1); tone and hum are amounts in [0, 1]. */
static inline float sid_fx_crt_tick(sid_fx_crt* c, float x, float tone, float hum) {
    if (tone != c->tone) {
        sid_fx_crt_set_tone(c, tone);
    }
    c->lp1 = sid_fx_flush(c->lp1 + c->a_lp * (x - c->lp1));
    c->lp2 = sid_fx_flush(c->lp2 + c->a_lp * (c->lp1 - c->lp2));
    c->hp_y = sid_fx_flush(c->r_hp * (c->hp_y + c->lp2 - c->hp_x));
    c->hp_x = c->lp2;
    float y = x + c->wet * (c->hp_y - x);

    uint32_t p = c->hum_phase >> 26; /* 64 steps */
    float f = (float)((c->hum_phase >> 3) & 0x7fffffu) * (1.0f / 8388608.0f);
    c->hum_phase += c->hum_inc;
    float h = sid_fx_lerp(c->hum_table[p], c->hum_table[p + 1u], f);
    return y + hum * SID_FX_HUM_LEVEL * h;
}

#ifdef __cplusplus
}
#endif

#endif
