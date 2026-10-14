// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_filter.h"

#include "sid_curves_resid016.h"

#define COUNT(a) (sizeof(a) / sizeof((a)[0]))

void sid_svf_reset(sid_svf* f, float drive) {
    f->ic1 = f->ic2 = 0.0f;
    f->lp = f->bp = f->hp = 0.0f;
    f->a1 = 1.0f;
    f->a2 = f->a3 = 0.0f;
    f->k = 1.0f;
    f->drive = drive;
    f->inv_drive = drive > 0.0f ? 1.0f / drive : 0.0f;
}

static float interpolate(const float (*points)[2], uint32_t n, uint32_t fc) {
    for (uint32_t i = 1; i < n; ++i) {
        float x0 = points[i - 1][0], x1 = points[i][0];
        if ((float)fc <= x1) {
            float t = ((float)fc - x0) / (x1 - x0);
            return points[i - 1][1] + t * (points[i][1] - points[i - 1][1]);
        }
    }
    return points[n - 1][1];
}

float sid_cutoff_hz(sid_model model, uint32_t fc) {
    return model == SID_MODEL_6581
               ? interpolate(SID_F0_POINTS_6581, COUNT(SID_F0_POINTS_6581), fc)
               : interpolate(SID_F0_POINTS_8580, COUNT(SID_F0_POINTS_8580), fc);
}
