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

/* The tables (gen/filter.h) stay in flash: sid_init and sid_set_filter_curve read them,
 * never the audio path. */
uint32_t sid_filter_curves(sid_model model) {
    return model == SID_MODEL_6581 ? SID_FILTER_CURVES_6581 : SID_FILTER_CURVES_8580;
}

float sid_cutoff_hz(sid_model model, uint32_t curve, uint32_t fc) {
    /* Separate model branches pay for decoding with cheaper bounds and address selection. */
    uint32_t code;
    if (model == SID_MODEL_6581) {
        if (curve >= SID_FILTER_CURVES_6581) {
            curve = 0;
        }
        code = SID_CUTOFF_HZ_6581[curve][fc & 2047u];
    } else {
        if (curve >= SID_FILTER_CURVES_8580) {
            curve = 0;
        }
        code = SID_CUTOFF_HZ_8580[curve][fc & 2047u];
    }
    union {
        uint32_t bits;
        float hz;
    } value = {(code << 11) + (SID_CUTOFF_CODE_BIAS << 23)};
    return value.hz;
}

float sid_resonance_k(sid_model model, uint32_t curve, uint32_t res) {
    res &= 15u;
    if (curve >= sid_filter_curves(model)) {
        curve = 0;
    }
    return model == SID_MODEL_6581 ? SID_RES_K_6581[curve][res] : SID_RES_K_8580[curve][res];
}
