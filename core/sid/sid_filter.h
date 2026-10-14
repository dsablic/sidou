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
float sid_cutoff_hz(sid_model model, uint32_t fc);

/* Inline: sid_write (the audio path, in SRAM on the device) and sid_init (in flash) both call
 * it, and a call between flash and SRAM is out of the pi32v2's direct call range
 * (platform/fm1/app.ld). */
static inline void sid_svf_set(sid_svf* f, float g, float k) {
    f->k = k;
    f->a1 = 1.0f / (1.0f + g * (g + k));
    f->a2 = g * f->a1;
    f->a3 = g * f->a2;
}

static inline void sid_svf_tick(sid_svf* f, float v0) {
    float v3 = v0 - f->ic2;
    float v1 = f->a1 * f->ic1 + f->a2 * v3;
    float v2 = f->ic2 + f->a2 * f->ic1 + f->a3 * v3;
    f->ic1 = 2.0f * v1 - f->ic1;
    f->ic2 = 2.0f * v2 - f->ic2;
    /* Only ic1 is clipped, so bp (v1) can briefly exceed 1/drive: a deliberate M0 stand-in. */
    if (f->drive > 0.0f) {
        f->ic1 = sidou_soft_clip(f->ic1 * f->drive) * f->inv_drive;
    }
    f->lp = v2;
    f->bp = v1;
    f->hp = v0 - f->k * v1 - v2;
    /* On silence the state decays into subnormals, which may be slow or trap on the pi32v2
     * FPU. Flush explicitly rather than relying on an FPU flush-to-zero mode. */
    if (f->ic1 < 1e-15f && f->ic1 > -1e-15f) {
        f->ic1 = 0.0f;
    }
    if (f->ic2 < 1e-15f && f->ic2 > -1e-15f) {
        f->ic2 = 0.0f;
    }
}

#endif
