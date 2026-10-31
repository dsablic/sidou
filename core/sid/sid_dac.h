// SPDX-FileCopyrightText: 2011-2025 Leandro Nini <drfiemost@users.sourceforge.net>
// SPDX-FileCopyrightText: 2007-2010 Antti Lankila
// SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-2.0-or-later
#ifndef SIDOU_SID_DAC_H
#define SIDOU_SID_DAC_H

#include <stdint.h>

#define SID_DAC_KINK_6581 2.20f
#define SID_DAC_KINK_8580 2.00f
#define SID_DAC_LEAKAGE_6581 0.0075f
#define SID_DAC_LEAKAGE_8580 0.0035f

typedef struct {
    float wave[4096];
    float env[256];
} sid_dac;

extern const float SID_DAC_MISMATCH_WAVE[12];
extern const float SID_DAC_MISMATCH_ENV[8];

void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage);

#define SID_DAC_BUILD_STEPS 4u
typedef struct {
    sid_dac* d;
    float kink, leakage;
    float mismatch;
    float zero;
    float voice_dc;
    uint8_t is6581;
    uint8_t step;
} sid_dac_job;
void sid_dac_job_start(sid_dac_job* j, sid_dac* d, int is6581, float kink, float leakage);
int sid_dac_job_step(sid_dac_job* j);

const sid_dac* sid_dac_default(int is6581);
int sid_dac_default_step(int is6581);

#endif
