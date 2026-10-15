// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_filter.h"

#include "gen/filter.h"

void sid_svf_reset(sid_svf* f, float drive) {
    f->ic1 = f->ic2 = 0.0f;
    f->lp = f->bp = f->hp = 0.0f;
    f->a1 = 1.0f;
    f->a2 = f->a3 = 0.0f;
    f->k = 1.0f;
    f->drive = drive;
    f->inv_drive = drive > 0.0f ? 1.0f / drive : 0.0f;
}

/* The tables (gen/filter.h) stay in flash: sid_init reads them, never the audio path. */
float sid_cutoff_hz(sid_model model, uint32_t fc) {
    fc &= 2047u;
    return model == SID_MODEL_6581 ? SID_CUTOFF_HZ_6581[0][fc] : SID_CUTOFF_HZ_8580[0][fc];
}

float sid_resonance_k(sid_model model, uint32_t res) {
    res &= 15u;
    return model == SID_MODEL_6581 ? SID_RES_K_6581[0][res] : SID_RES_K_8580[0][res];
}
