// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_FILTER_H
#define SIDOU_SID_FILTER_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_math.h"

typedef struct {
    float ic1, ic2;
    float lp, bp, hp;
    float a1, a2, a3, k;
    float g;
    float load_at;
    float bpn;
} sid_svf;

typedef struct {
    float level;
    float level_ref;
    float level_off;
    float load;
    float load_ref;
    float floor;
} sid_nonlin;

#define SID_NL_M_MIN 0.35f
#define SID_NL_M_MAX 1.3f

void sid_svf_reset(sid_svf* f);
uint32_t sid_filter_curves(sid_model model);
float sid_cutoff_hz(sid_model model, uint32_t curve, uint32_t fc);

typedef struct {
    const uint8_t* run;
    const uint16_t* at;
    const uint8_t* p;
    uint32_t fc;
    uint32_t code;
} sid_cutoff_seq;

void sid_cutoff_seq_init(sid_cutoff_seq* q, sid_model model, uint32_t curve);
uint32_t sid_cutoff_seq_seek(sid_cutoff_seq* q, uint32_t fc);

#define SID_CUTOFF_CODE_BIAS 126u

static inline float sid_cutoff_seq_hz(sid_cutoff_seq* q, uint32_t fc) {
    uint32_t code;
    fc &= 2047u;
    if (fc == q->fc + 1u && (fc & 31u)) {
        const uint8_t* p = q->p;
        int32_t d = (int8_t)*p++;
        if (d == -128) {
            code = (uint32_t)p[0] << 8 | p[1];
            p += 2;
        } else {
            code = (q->code + (uint32_t)d) & 0xffffu;
        }
        q->p = p;
        q->fc = fc;
        q->code = code;
    } else {
        code = sid_cutoff_seq_seek(q, fc);
    }
    union {
        uint32_t bits;
        float hz;
    } value = {(code << 11) + (SID_CUTOFF_CODE_BIAS << 23)};
    return value.hz;
}
float sid_resonance_k(sid_model model, uint32_t curve, uint32_t res);

static inline void sid_svf_set(sid_svf* f, float g, float k) {
    f->k = k;
    f->g = g;
    f->a1 = 1.0f / (1.0f + g * (g + k));
    f->a2 = g * f->a1;
    f->a3 = g * f->a2;
}

static inline void sid_svf_set_6581(sid_svf* f, const sid_nonlin* n, float g, float k,
                                    float dc) {
    sid_svf_set(f, g, k);
    f->load_at = n->load_ref - dc;
}

static inline void sid_svf_flush(sid_svf* f) {
    if (f->ic1 < 1e-15f && f->ic1 > -1e-15f) {
        f->ic1 = 0.0f;
    }
    if (f->ic2 < 1e-15f && f->ic2 > -1e-15f) {
        f->ic2 = 0.0f;
    }
}

static inline void sid_svf_tick(sid_svf* f, float v0) {
    float v3 = v0 - f->ic2;
    float v1 = f->a1 * f->ic1 + f->a2 * v3;
    float v2 = f->ic2 + f->a2 * f->ic1 + f->a3 * v3;
    f->ic1 = 2.0f * v1 - f->ic1;
    f->ic2 = 2.0f * v2 - f->ic2;
    f->lp = v2;
    f->bp = v1;
    f->hp = v0 - f->k * v1 - v2;
    sid_svf_flush(f);
}

typedef struct {
    float a, b;
    float u_lo, u_hi;
    float level, floor;
    float p, q;
    float a1_min;
    float k;
} sid_nl_coefs;

static inline sid_nl_coefs sid_nl_coefs_of(const sid_svf* f, const sid_nonlin* n) {
    const float g0 = f->g, a10 = f->a1, off = n->level_off;
    sid_nl_coefs c;
    c.a = g0 * (1.0f + n->load * f->load_at) + off;
    c.b = g0 * n->load;
    c.u_lo = SID_NL_M_MIN * g0 + off;
    c.u_hi = SID_NL_M_MAX * g0 + off;
    c.level = n->level;
    c.floor = n->floor;
    c.p = a10 * (2.0f - a10);
    c.q = a10 * a10;
    c.a1_min = 0.1f * a10;
    c.k = f->k;
    return c;
}

static inline void sid_nl_tick(float* ic1, float* ic2, float* lp, float* bp, float* hp,
                               float* bpn, float v0, const sid_nl_coefs* c) {
    float u = __builtin_fminf(__builtin_fmaxf(c->a - c->b * *ic2, c->u_lo), c->u_hi);
    float swing = *hp < 0.0f ? -*hp : *hp;
    float g = __builtin_fmaxf(u + c->level * swing, c->floor);
    float a1 = __builtin_fmaxf(c->p - c->q * (g * (g + c->k)), c->a1_min);
    float a2 = g * a1;
    float a3 = g * a2;
    float c1 = *ic1, c2 = *ic2;
    float v3 = v0 - c2;
    float v1 = a1 * c1 + a2 * v3;
    float v2 = c2 + a2 * c1 + a3 * v3;
    *bpn = (c1 - v1 + g * (v3 - g * v1)) / g;
    c1 = 2.0f * v1 - c1;
    c2 = 2.0f * v2 - c2;
    *lp = v2;
    *bp = v1;
    *hp = v0 - c->k * v1 - v2;
    *ic1 = c1 < 1e-15f && c1 > -1e-15f ? 0.0f : c1;
    *ic2 = c2 < 1e-15f && c2 > -1e-15f ? 0.0f : c2;
}

static inline void sid_svf_tick_6581(sid_svf* f, const sid_nonlin* n, float v0) {
    const sid_nl_coefs c = sid_nl_coefs_of(f, n);
    sid_nl_tick(&f->ic1, &f->ic2, &f->lp, &f->bp, &f->hp, &f->bpn, v0, &c);
}

#endif
