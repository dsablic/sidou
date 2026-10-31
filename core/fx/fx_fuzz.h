// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_FUZZ_H
#define SIDOU_FX_FUZZ_H

#include <stdint.h>

#include "fx_adaa.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_FUZZ_PRE_HZ 7000.0f
#define SID_FX_FUZZ_GAIN 99.0f
#define SID_FX_FUZZ_REF 0.18f
#define SID_FX_FUZZ_BIAS 0.5f
#define SID_FX_FUZZ_ENV_MS 10.0f
#define SID_FX_FUZZ_DC_HZ 10.0f

typedef struct {
    float pre_a, env_a, dc_r;
    float pre_lp, env, u_prev, dc_x, dc_y;
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
