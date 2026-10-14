// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_plate.h"

#define MASK (SID_FX_PLATE_LEN - 1u)

/* Lines, in ring order: the paper's lengths at 29,761 Hz; the modulated allpasses get
 * their swing added. */
enum {
    IN1, IN2, IN3, IN4,      /* input diffusers: 142, 107, 379, 277 */
    L_AP, L_D1, L_AP2, L_D2, /* left half: mod allpass 672, 4453, allpass 1800, 3720 */
    R_AP, R_D1, R_AP2, R_D2  /* right half: mod allpass 908, 4217, allpass 2656, 3163 */
};
static const float LENGTH_29K[SID_FX_PLATE_LINES] = {142,  107,  379, 277,  672,  4453,
                                                     1800, 3720, 908, 4217, 2656, 3163};
/* Output taps (paper, table 2): left from the right half and right from the left. */
static const float TAP_29K[14] = {266, 2974, 1913, 1996, 1990, 187,  1066,
                                  353, 3627, 1228, 2673, 2111, 335, 121};

void sid_fx_plate_init(sid_fx_plate* p, uint32_t sample_rate) {
    float scale = (float)sample_rate / 29761.0f;
    /* Fit the lines (plus a sample of spacing each) into the ring. */
    float total = 0.0f;
    for (int i = 0; i < SID_FX_PLATE_LINES; ++i) {
        total += LENGTH_29K[i] * scale + 1.0f;
    }
    float swing = 16.0f * scale;
    total += 2.0f * (swing + 2.0f);
    if (total > (float)(SID_FX_PLATE_LEN - 8u)) {
        scale *= (float)(SID_FX_PLATE_LEN - 8u) / total;
    }
    p->excursion = 16.0f * scale;
    uint32_t base = 0;
    for (int i = 0; i < SID_FX_PLATE_LINES; ++i) {
        float len = LENGTH_29K[i] * scale;
        if (i == L_AP || i == R_AP) {
            len += p->excursion + 2.0f;
        }
        uint32_t n = (uint32_t)(int32_t)(len + 0.5f);
        p->base[i] = base;
        p->length[i] = n;
        base += n + 1u;
    }
    for (int i = 0; i < 14; ++i) {
        p->tap[i] = (uint32_t)(int32_t)(TAP_29K[i] * scale + 0.5f);
    }
    p->wp = 0;
    p->lfo_phase = 0;
    p->lfo_inc = sid_fx_phase_inc(SID_FX_PLATE_LFO_HZ, sample_rate) * SID_FX_PLATE_SEGMENT;
    p->left = 0;
    p->mod_l = (float)(p->length[L_AP] - 1u) - p->excursion - 1.0f;
    p->mod_r = (float)(p->length[R_AP] - 1u) - p->excursion - 1.0f;
    p->step_l = 0.0f;
    p->step_r = 0.0f;
    p->bw = 0.0f;
    p->damp_l = 0.0f;
    p->damp_r = 0.0f;
    p->tank_l = 0.0f;
    p->tank_r = 0.0f;
}

/* The sample written `offset` samples ago on `line` (h: its head index this sample). */
#define AT(h, offset) buf[((h) + (offset)) & MASK]

static inline float get_frac(const float* buf, uint32_t h, float offset) {
    uint32_t i = (uint32_t)(int32_t)offset;
    float f = offset - (float)(int32_t)i;
    float a = AT(h, i);
    return a + f * (AT(h, i + 1u) - a);
}

/* Schroeder allpass on a line: w = x + g d; y = d - g w; the line takes w. */
static inline float allpass(float* buf, uint32_t h, float x, float coef, float delayed) {
    float w = sid_fx_flush(x + coef * delayed);
    AT(h, 0) = w;
    return delayed - coef * w;
}

void sid_fx_plate_tick(sid_fx_plate* p, float* buf, float m, float decay, float damping,
                       float bandwidth, float* l, float* r) {
    uint32_t wp = (p->wp - 1u) & MASK;
    p->wp = wp;
    uint32_t h[SID_FX_PLATE_LINES];
    for (int i = 0; i < SID_FX_PLATE_LINES; ++i) {
        h[i] = wp + p->base[i];
    }
#define TAIL(line) AT(h[line], p->length[line] - 1u)

    float dec = 0.30f + 0.55f * decay;
    float dd2 = dec + 0.15f;
    dd2 = dd2 < 0.25f ? 0.25f : dd2 > 0.5f ? 0.5f : dd2;
    float damp = 0.05f + 0.65f * damping;
    float bw = 0.3f + 0.6995f * bandwidth;

    /* Bandwidth, then the input diffusers. */
    p->bw = sid_fx_flush(p->bw + bw * (m - p->bw));
    float x = p->bw;
    x = allpass(buf, h[IN1], x, 0.75f, TAIL(IN1));
    x = allpass(buf, h[IN2], x, 0.75f, TAIL(IN2));
    x = allpass(buf, h[IN3], x, 0.625f, TAIL(IN3));
    x = allpass(buf, h[IN4], x, 0.625f, TAIL(IN4));

    /* The tank. Each half's modulated allpass reads its line at length - 2 - swing + swing
     * lfo, the LFO taken at segment ends. */
    if (p->left == 0) {
        p->lfo_phase += p->lfo_inc;
        const float seg = 1.0f / (float)SID_FX_PLATE_SEGMENT;
        float base_l = (float)(p->length[L_AP] - 1u) - p->excursion - 1.0f;
        float base_r = (float)(p->length[R_AP] - 1u) - p->excursion - 1.0f;
        p->step_l = (base_l + p->excursion * sid_fx_sin(p->lfo_phase) - p->mod_l) * seg;
        p->step_r = (base_r + p->excursion * sid_fx_cos(p->lfo_phase) - p->mod_r) * seg;
        p->left = SID_FX_PLATE_SEGMENT;
    }
    --p->left;
    p->mod_l += p->step_l;
    p->mod_r += p->step_r;

    float in_l = x + dec * p->tank_r;
    float in_r = x + dec * p->tank_l;

    /* Left half: allpass (decay diffusion 1, -0.70 into the tank as in the paper), delay,
     * damping, decay, allpass (decay diffusion 2), delay. */
    float y = allpass(buf, h[L_AP], in_l, -0.70f, get_frac(buf, h[L_AP], p->mod_l));
    AT(h[L_D1], 0) = y;
    float t = TAIL(L_D1);
    p->damp_l = sid_fx_flush(t + damp * (p->damp_l - t));
    y = allpass(buf, h[L_AP2], dec * p->damp_l, dd2, TAIL(L_AP2));
    AT(h[L_D2], 0) = y;
    float out_l = TAIL(L_D2);

    y = allpass(buf, h[R_AP], in_r, -0.70f, get_frac(buf, h[R_AP], p->mod_r));
    AT(h[R_D1], 0) = y;
    t = TAIL(R_D1);
    p->damp_r = sid_fx_flush(t + damp * (p->damp_r - t));
    y = allpass(buf, h[R_AP2], dec * p->damp_r, dd2, TAIL(R_AP2));
    AT(h[R_D2], 0) = y;
    float out_r = TAIL(R_D2);

    p->tank_l = out_l;
    p->tank_r = out_r;

    /* Output taps (each at its offset from the line's head). */
    const uint32_t* k = p->tap;
    *l = 0.6f * (AT(h[R_D1], k[0]) + AT(h[R_D1], k[1]) - AT(h[R_AP2], k[2]) +
                 AT(h[R_D2], k[3]) - AT(h[L_D1], k[4]) - AT(h[L_AP2], k[5]) -
                 AT(h[L_D2], k[6]));
    *r = 0.6f * (AT(h[L_D1], k[7]) + AT(h[L_D1], k[8]) - AT(h[L_AP2], k[9]) +
                 AT(h[L_D2], k[10]) - AT(h[R_D1], k[11]) - AT(h[R_AP2], k[12]) -
                 AT(h[R_D2], k[13]));
#undef TAIL
}
