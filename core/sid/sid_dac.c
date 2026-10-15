// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_dac.h"

#include "gen/dac.h"

/* The ladder's weights at `kink`, interpolated linearly between grid points (exactly a
 * grid point's within 1e-3 of a step). */
static void weights(float* w, uint32_t bits, const float* grid, float kink) {
    float t = (kink - SID_DAC_KINK_FIRST) / SID_DAC_KINK_STEP;
    if (t < 0.0f) {
        t = 0.0f;
    }
    if (t > (float)(SID_DAC_KINKS - 1)) {
        t = (float)(SID_DAC_KINKS - 1);
    }
    uint32_t k = (uint32_t)(t + 0.5f);
    float f = t - (float)k;
    if (f < 1e-3f && f > -1e-3f) {
        for (uint32_t i = 0; i < bits; ++i) {
            w[i] = grid[k * bits + i];
        }
        return;
    }
    k = (uint32_t)t;
    f = t - (float)k;
    for (uint32_t i = 0; i < bits; ++i) {
        w[i] = grid[k * bits + i] * (1.0f - f) + grid[(k + 1) * bits + i] * f;
    }
}

/* reSIDfp's Dac::getOutput: an on bit adds its weight, an off bit its leakage. */
static float ladder_out(const float* w, uint32_t bits, float leakage, uint32_t input) {
    float v = 0.0f;
    for (uint32_t i = 0; i < bits; ++i) {
        v += (input >> i & 1u) ? w[i] : w[i] * leakage;
    }
    return v;
}

static float saturate(float v) {
    return SID_DAC_SAT_GAIN * v + (1.0f - SID_DAC_SAT_GAIN) * SID_DAC_SAT * v * v * v;
}

void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage) {
    uint32_t m = is6581 ? 0u : 1u;
    float w[12];
    weights(w, 12, &SID_DAC_WAVE_WEIGHTS[m][0][0], kink);
    uint32_t zero = is6581 ? SID_DAC_ZERO_6581 : SID_DAC_ZERO_8580;
    float z = ladder_out(w, 12, leakage, zero);
    if (is6581) {
        z = saturate(z);
    }
    /* Twice reSIDfp's scale, where the waveform DAC spans about 0 to 1. */
    float dc = is6581 ? SID_VOICE_DC_6581 : 0.0f;
    for (uint32_t i = 0; i < 4096u; ++i) {
        float v = ladder_out(w, 12, leakage, i);
        if (is6581) {
            v = saturate(v);
        }
        d->wave[i] = 2.0f * (v - z + dc);
    }
    weights(w, 8, &SID_DAC_ENV_WEIGHTS[m][0][0], kink);
    for (uint32_t i = 0; i < 256u; ++i) {
        d->env[i] = ladder_out(w, 8, leakage, i);
    }
}

void sid_dac_build_default(sid_dac* d, int is6581) {
    if (is6581) {
        sid_dac_build(d, 1, SID_DAC_KINK_6581, SID_DAC_LEAKAGE_6581);
    } else {
        sid_dac_build(d, 0, SID_DAC_KINK_8580, SID_DAC_LEAKAGE_8580);
    }
}
