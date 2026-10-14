// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_FILTER_H
#define SIDOU_SID_FILTER_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_math.h"

/* Cytomic trapezoidal SVF. drive > 0 adds the 6581 band-pass saturation. */
typedef struct {
    float ic1, ic2;
    float lp, bp, hp;
    float a1, a2, a3, k;
    float drive, inv_drive;
} sid_svf;

void sid_svf_reset(sid_svf* f, float drive);
void sid_svf_set(sid_svf* f, float g, float k);
float sid_cutoff_hz(sid_model model, uint32_t fc);

static inline void sid_svf_tick(sid_svf* f, float v0) {
    float v3 = v0 - f->ic2;
    float v1 = f->a1 * f->ic1 + f->a2 * v3;
    float v2 = f->ic2 + f->a2 * f->ic1 + f->a3 * v3;
    f->ic1 = 2.0f * v1 - f->ic1;
    f->ic2 = 2.0f * v2 - f->ic2;
    if (f->drive > 0.0f) {
        f->ic1 = sidou_soft_clip(f->ic1 * f->drive) * f->inv_drive;
    }
    f->lp = v2;
    f->bp = v1;
    f->hp = v0 - f->k * v1 - v2;
}

#endif
