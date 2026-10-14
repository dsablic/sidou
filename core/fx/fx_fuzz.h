// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_FUZZ_H
#define SIDOU_FX_FUZZ_H

/* Fuzz: a pre low-pass (~7 kHz, which also tames the SID's saw and pulse highs), up to
 * +40 dB of gain into a hard clip, and an octave-up blend from the rectified clip; both
 * shapers through closed-form ADAA of the whole function (fx_adaa.h), then a DC blocker. Reimplemented from
 * Parker et al. (DAFx-16), Bilbao et al. (IEEE SPL 2017) and Zolzer, DAFX (nonlinear
 * processing).
 *
 * - Amount: gain 1 + 99 a^2. Makeup max(REF, 1 / gain): unity while the gain is low,
 *   then the clipped output held at REF, a square with the loudness of a 0.25 full-scale
 *   sine (a SID voice).
 * - A: octave-up blend, from the hard clip (0) to twice its rectified value (1).
 * - B: "starved" bias (up to 0.5 into the clip), gated by an envelope follower so it
 *   only acts while there is signal, as in a fuzz with a sagging supply: silence stays
 *   silent, notes come out sputtery and asymmetric.
 * Any amounts in [0, 1] may arrive on any sample. */

#include <stdint.h>

#include "fx_adaa.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_FUZZ_PRE_HZ 7000.0f
#define SID_FX_FUZZ_GAIN 99.0f
#define SID_FX_FUZZ_REF 0.18f /* a clipped square this high has the RMS of a 0.25 sine */
#define SID_FX_FUZZ_BIAS 0.5f
#define SID_FX_FUZZ_ENV_MS 10.0f
#define SID_FX_FUZZ_DC_HZ 10.0f

typedef struct {
    float pre_a, env_a, dc_r;
    float pre_lp, env, u_prev, dc_x, dc_y;
    /* Derived from the amount. */
    float amount, drive, inv_drive, makeup;
} sid_fx_fuzz;

static inline void sid_fx_fuzz_set_amount(sid_fx_fuzz* f, float a) {
    f->amount = a;
    f->drive = 1.0f + SID_FX_FUZZ_GAIN * a * a;
    sid_fx_recip_track(&f->inv_drive, f->drive);
    f->makeup = f->inv_drive > SID_FX_FUZZ_REF ? f->inv_drive : SID_FX_FUZZ_REF;
}

static inline void sid_fx_fuzz_reset(sid_fx_fuzz* f) {
    f->pre_lp = 0.0f;
    f->env = 0.0f;
    f->u_prev = 0.0f;
    f->dc_x = 0.0f;
    f->dc_y = 0.0f;
    f->inv_drive = 1.0f;
    sid_fx_fuzz_set_amount(f, 0.0f);
}

static inline void sid_fx_fuzz_init(sid_fx_fuzz* f, uint32_t sample_rate) {
    float w_scale = 2.0f * SIDOU_PI / (float)sample_rate;
    f->pre_a = 1.0f - sid_fx_exp_init(-w_scale * SID_FX_FUZZ_PRE_HZ);
    f->env_a = sid_fx_smooth_coef(SID_FX_FUZZ_ENV_MS, sample_rate);
    f->dc_r = sid_fx_exp_init(-w_scale * SID_FX_FUZZ_DC_HZ);
    sid_fx_fuzz_reset(f);
}

/* x is normalised to [-1, 1); amount (gain), octave and bias are in [0, 1]. */
static inline float sid_fx_fuzz_tick(sid_fx_fuzz* f, float x, float amount, float octave,
                                     float bias) {
    if (amount != f->amount) {
        sid_fx_fuzz_set_amount(f, amount);
    }
    f->pre_lp = sid_fx_flush(f->pre_lp + f->pre_a * (x - f->pre_lp));
    f->env = sid_fx_flush(f->env + f->env_a * (sid_fx_abs(f->pre_lp) - f->env));
    float gate = 4.0f * f->env;
    float u = f->drive * f->pre_lp + bias * SID_FX_FUZZ_BIAS * (gate < 1.0f ? gate : 1.0f);
    float hard = sid_fx_adaa_hard_clip(u, f->u_prev);
    float rect = sid_fx_adaa_rect(u, f->u_prev);
    f->u_prev = u;
    float y = hard + octave * (2.0f * rect - hard);
    f->dc_y = sid_fx_flush(y - f->dc_x + f->dc_r * f->dc_y);
    f->dc_x = y;
    return f->dc_y * f->makeup;
}

#ifdef __cplusplus
}
#endif

#endif
