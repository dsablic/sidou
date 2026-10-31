// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CRT_H
#define SIDOU_FX_CRT_H

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CRT_LP_MIN_HZ 4000.0f
#define SID_FX_CRT_LP_MAX_HZ 18000.0f
#define SID_FX_CRT_LP_LN_RANGE 1.5040774f
#define SID_FX_CRT_HP_MIN_HZ 20.0f
#define SID_FX_CRT_HP_MAX_HZ 150.0f
#define SID_FX_CRT_HP_LN_RANGE 2.0149030f
#define SID_FX_CRT_WET_SPAN 0.25f
#define SID_FX_CRT_STEPS 32u
#define SID_FX_HUM_STEPS 64u
#define SID_FX_HUM_HZ 50.0f
#define SID_FX_HUM_LEVEL 0.003f
#define SID_FX_HUM_H2 0.5f
#define SID_FX_HUM_H3 0.3f

typedef struct {
    float a_lp_table[SID_FX_CRT_STEPS + 1u];
    float r_hp_table[SID_FX_CRT_STEPS + 1u];
    float hum_table[SID_FX_HUM_STEPS + 1u];
    float lp1, lp2;
    float hp_x, hp_y;
    uint32_t hum_phase, hum_inc;
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

static inline void sid_fx_crt_reset(sid_fx_crt* c, float x) {
    c->lp1 = x;
    c->lp2 = x;
    c->hp_x = x;
    c->hp_y = x;
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
        uint32_t phase = i * (0xffffffffu / SID_FX_HUM_STEPS + 1u);
        float s = sid_fx_sin(phase);
        float co = sid_fx_cos(phase);
        c->hum_table[i] =
            s + SID_FX_HUM_H2 * 2.0f * s * co + SID_FX_HUM_H3 * (3.0f - 4.0f * s * s) * s;
    }
    c->hum_inc = sid_fx_phase_inc(SID_FX_HUM_HZ, sample_rate);
    sid_fx_crt_reset(c, 0.0f);
}

static inline float sid_fx_crt_tick(sid_fx_crt* c, float x, float tone, float hum) {
    if (tone != c->tone) {
        sid_fx_crt_set_tone(c, tone);
    }
    c->lp1 = sid_fx_flush(c->lp1 + c->a_lp * (x - c->lp1));
    c->lp2 = sid_fx_flush(c->lp2 + c->a_lp * (c->lp1 - c->lp2));
    c->hp_y = sid_fx_flush(c->r_hp * (c->hp_y + c->lp2 - c->hp_x));
    c->hp_x = c->lp2;
    float y = x + c->wet * (c->hp_y - x);

    uint32_t p = c->hum_phase >> 26;
    float f = (float)((c->hum_phase >> 3) & 0x7fffffu) * (1.0f / 8388608.0f);
    c->hum_phase += c->hum_inc;
    float h = sid_fx_lerp(c->hum_table[p], c->hum_table[p + 1u], f);
    return y + hum * SID_FX_HUM_LEVEL * h;
}

#ifdef __cplusplus
}
#endif

#endif
