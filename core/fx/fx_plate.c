// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_plate.h"

#define MASK (SID_FX_PLATE_LEN - 1u)

/* Lines, in ring order. The paper's lengths at 29,761 Hz; the modulated allpasses get
 * their swing added, the pre-delay its maximum. */
enum {
    PRE,                    /* pre-delay */
    IN1, IN2, IN3, IN4,     /* input diffusers: 142, 107, 379, 277 */
    L_AP, L_D1, L_AP2, L_D2, /* left half: mod allpass 672, 4453, allpass 1800, 3720 */
    R_AP, R_D1, R_AP2, R_D2  /* right half: mod allpass 908, 4217, allpass 2656, 3163 */
};
static const float LENGTH_29K[SID_FX_PLATE_LINES] = {0,    142,  107,  379,  277,  672, 4453,
                                                     1800, 3720, 908,  4217, 2656, 3163};
/* Output taps (paper, table 2): left from the right half and right from the left. */
static const float TAP_29K[14] = {266, 2974, 1913, 1996, 1990, 187,  1066,
                                  353, 3627, 1228, 2673, 2111, 335, 121};

void sid_fx_plate_init(sid_fx_plate* p, uint32_t sample_rate) {
    float fs = (float)sample_rate;
    float scale = fs / 29761.0f;
    p->excursion = 16.0f * scale;
    p->max_predelay = SID_FX_PLATE_PREDELAY_MS * 0.001f * fs;
    uint32_t base = 0;
    for (int i = 0; i < SID_FX_PLATE_LINES; ++i) {
        float len = LENGTH_29K[i] * scale;
        if (i == L_AP || i == R_AP) {
            len += p->excursion + 2.0f;
        } else if (i == PRE) {
            len = p->max_predelay + 2.0f;
        }
        uint32_t n = (uint32_t)(int32_t)(len + 0.5f);
        p->base[i] = base;
        p->length[i] = n;
        base += n + 1u;
    }
    for (int i = 0; i < 14; ++i) {
        p->tap[i] = TAP_29K[i] * scale;
    }
    p->predelay = 0.0f;
    p->predelay_amount = -1.0f;
    p->predelay_target = 0.0f;
    p->wp = 0;
    p->lfo_phase = 0;
    p->lfo_inc = sid_fx_phase_inc(SID_FX_PLATE_LFO_HZ, sample_rate);
    p->bw = 0.0f;
    p->damp_l = 0.0f;
    p->damp_r = 0.0f;
    p->tank_l = 0.0f;
    p->tank_r = 0.0f;
}

typedef struct {
    int16_t* buf;
    const sid_fx_plate* p;
} ring;

static inline float get(const ring* g, int line, uint32_t offset) {
    return (float)g->buf[(g->p->wp + g->p->base[line] + offset) & MASK] * (1.0f / 32768.0f);
}

/* Linear interpolation at a fractional offset. */
static inline float get_frac(const ring* g, int line, float offset) {
    uint32_t i = (uint32_t)(int32_t)offset;
    float f = offset - (float)(int32_t)i;
    float a = get(g, line, i);
    return a + f * (get(g, line, i + 1u) - a);
}

/* Truncated towards zero (and clamped), so the tank decays to exact silence. */
static inline void put(const ring* g, int line, float v) {
    float s = v * 32768.0f;
    s = s > 32767.0f ? 32767.0f : s < -32768.0f ? -32768.0f : s;
    g->buf[(g->p->wp + g->p->base[line]) & MASK] = (int16_t)(int32_t)s;
}

/* The last sample of a line (written length - 1 samples ago, read before this sample's
 * write). */
static inline float tail(const ring* g, int line) {
    return get(g, line, g->p->length[line] - 1u);
}

/* Schroeder allpass on a line of its delay: w = x + g d; y = d - g w; d = w delayed. The
 * paper's sign convention: a positive g here is its "+" diffusion. */
static inline float allpass(const ring* g, int line, float x, float coef, float delayed) {
    float w = x + coef * delayed;
    put(g, line, w);
    return delayed - coef * w;
}

void sid_fx_plate_tick(sid_fx_plate* p, int16_t* buf, float m, float decay, float damping,
                       float predelay, float* l, float* r) {
    ring g = {buf, p};
    p->wp = (p->wp - 1u) & MASK;

    float dec = 0.30f + 0.55f * decay;
    float dd2 = dec + 0.15f;
    dd2 = dd2 < 0.25f ? 0.25f : dd2 > 0.5f ? 0.5f : dd2;
    float damp = 0.05f + 0.65f * damping;
    if (predelay != p->predelay_amount) {
        p->predelay_amount = predelay;
        p->predelay_target = predelay * p->max_predelay;
    }
    p->predelay = sid_fx_slew(p->predelay, p->predelay_target, SID_FX_PLATE_GLIDE);

    /* Pre-delay and bandwidth. */
    put(&g, PRE, m);
    float x = get_frac(&g, PRE, p->predelay);
    p->bw = sid_fx_flush(p->bw + 0.9995f * (x - p->bw));
    x = p->bw;

    /* Input diffusers. */
    x = allpass(&g, IN1, x, 0.75f, tail(&g, IN1));
    x = allpass(&g, IN2, x, 0.75f, tail(&g, IN2));
    x = allpass(&g, IN3, x, 0.625f, tail(&g, IN3));
    x = allpass(&g, IN4, x, 0.625f, tail(&g, IN4));

    /* The tank. Each half's modulated allpass reads its line at length - 1 - swing - lfo. */
    float s = sid_fx_sin(p->lfo_phase);
    float c = sid_fx_cos(p->lfo_phase);
    p->lfo_phase += p->lfo_inc;
    float mod_l = (float)(p->length[L_AP] - 1u) - p->excursion - 1.0f + p->excursion * s;
    float mod_r = (float)(p->length[R_AP] - 1u) - p->excursion - 1.0f + p->excursion * c;

    float in_l = x + dec * p->tank_r;
    float in_r = x + dec * p->tank_l;

    /* Left half: allpass (decay diffusion 1 = 0.70, sign as in the paper: -0.7 into the
     * tank), delay, damping, decay, allpass (decay diffusion 2), delay. */
    float yl = allpass(&g, L_AP, in_l, -0.70f, get_frac(&g, L_AP, mod_l));
    put(&g, L_D1, yl);
    float t = tail(&g, L_D1);
    p->damp_l = sid_fx_flush(t + damp * (p->damp_l - t));
    yl = allpass(&g, L_AP2, dec * p->damp_l, dd2, tail(&g, L_AP2));
    put(&g, L_D2, yl);
    float out_l = tail(&g, L_D2);

    float yr = allpass(&g, R_AP, in_r, -0.70f, get_frac(&g, R_AP, mod_r));
    put(&g, R_D1, yr);
    t = tail(&g, R_D1);
    p->damp_r = sid_fx_flush(t + damp * (p->damp_r - t));
    yr = allpass(&g, R_AP2, dec * p->damp_r, dd2, tail(&g, R_AP2));
    put(&g, R_D2, yr);
    float out_r = tail(&g, R_D2);

    p->tank_l = out_l;
    p->tank_r = out_r;

    /* Output taps (each read at its offset from the line's head). */
    const float* k = p->tap;
    *l = 0.6f * (get_frac(&g, R_D1, k[0]) + get_frac(&g, R_D1, k[1]) -
                 get_frac(&g, R_AP2, k[2]) + get_frac(&g, R_D2, k[3]) -
                 get_frac(&g, L_D1, k[4]) - get_frac(&g, L_AP2, k[5]) -
                 get_frac(&g, L_D2, k[6]));
    *r = 0.6f * (get_frac(&g, L_D1, k[7]) + get_frac(&g, L_D1, k[8]) -
                 get_frac(&g, L_AP2, k[9]) + get_frac(&g, L_D2, k[10]) -
                 get_frac(&g, R_D1, k[11]) - get_frac(&g, R_AP2, k[12]) -
                 get_frac(&g, R_D2, k[13]));
}
