// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_filter.h"

#include "gen/filter.h"

void sid_svf_reset(sid_svf* f) {
    f->ic1 = f->ic2 = 0.0f;
    f->lp = f->bp = f->hp = f->bpn = 0.0f;
    f->a1 = 1.0f;
    f->a2 = f->a3 = 0.0f;
    f->k = 1.0f;
    f->g = 0.0f;
}

uint32_t sid_filter_curves(sid_model model) {
    return model == SID_MODEL_6581 ? SID_FILTER_CURVES_6581 : SID_FILTER_CURVES_8580;
}

#define CUTOFF_BLOCK 32u
_Static_assert(sizeof SID_CUTOFF_AT_6581[0] / sizeof SID_CUTOFF_AT_6581[0][0] ==
                   2048u / CUTOFF_BLOCK,
               "gen/filter.h's cutoff blocks are 32 FC values");

void sid_cutoff_seq_init(sid_cutoff_seq* q, sid_model model, uint32_t curve) {
    if (model == SID_MODEL_6581) {
        q->run = SID_CUTOFF_RUN_6581;
        q->at = SID_CUTOFF_AT_6581[curve < SID_FILTER_CURVES_6581 ? curve : 0u];
    } else {
        q->run = SID_CUTOFF_RUN_8580;
        q->at = SID_CUTOFF_AT_8580[curve < SID_FILTER_CURVES_8580 ? curve : 0u];
    }
    q->p = q->run;
    q->fc = 0xffffffffu;
    q->code = 0;
}

uint32_t sid_cutoff_seq_seek(sid_cutoff_seq* q, uint32_t fc) {
    fc &= 2047u;
    if (fc != q->fc) {
        const uint8_t* p = q->run + q->at[fc / CUTOFF_BLOCK];
        uint32_t code = (uint32_t)p[0] << 8 | p[1];
        p += 2;
        for (uint32_t n = fc & (CUTOFF_BLOCK - 1u); n; --n) {
            int32_t d = (int8_t)*p++;
            if (d == -128) {
                code = (uint32_t)p[0] << 8 | p[1];
                p += 2;
            } else {
                code = (code + (uint32_t)d) & 0xffffu;
            }
        }
        q->p = p;
        q->fc = fc;
        q->code = code;
    }
    return q->code;
}

float sid_cutoff_hz(sid_model model, uint32_t curve, uint32_t fc) {
    sid_cutoff_seq q;
    sid_cutoff_seq_init(&q, model, curve);
    return sid_cutoff_seq_hz(&q, fc);
}

float sid_resonance_k(sid_model model, uint32_t curve, uint32_t res) {
    res &= 15u;
    if (curve >= sid_filter_curves(model)) {
        curve = 0;
    }
    return model == SID_MODEL_6581 ? SID_RES_K_6581[curve][res] : SID_RES_K_8580[curve][res];
}
