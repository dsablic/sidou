// SPDX-FileCopyrightText: 2011-2025 Leandro Nini <drfiemost@users.sourceforge.net>
// SPDX-FileCopyrightText: 2007-2010 Antti Lankila
// SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-2.0-or-later
#ifndef SIDOU_SID_DAC_H
#define SIDOU_SID_DAC_H

#include <stdint.h>

/* reSIDfp's DAC parameters (Dac.cpp, SID.cpp, FilterModelConfig6581.cpp). */
/* The R-2R ladder's 2R/R ratio, the "kink": the 6581's is non-linear, the 8580's not. */
#define SID_DAC_KINK_6581 2.20f
#define SID_DAC_KINK_8580 2.00f
/* MOSFET leakage: an off bit still adds this fraction of its weight. */
#define SID_DAC_LEAKAGE_6581 0.0075f
#define SID_DAC_LEAKAGE_8580 0.0035f

/* The voice DACs, as reSIDfp models them (Dac::kinkedDac). A voice outputs
 * wave[waveform output] * env[envelope counter]. */
typedef struct {
    /* The waveform DAC (6581: saturating) minus its value at the voice's zero (0x380 on
     * the 6581, 0x800 on the 8580), plus the 6581's voice DC, which follows the envelope
     * DAC and so rides on env too. In units where a full-scale 8580 voice spans about
     * -1 to 1 (twice reSIDfp's). */
    float wave[4096];
    float env[256]; /* the envelope DAC: about counter / 255, never 0 (leakage) */
} sid_dac;

/* Builds both DACs for a model, from the ladder's 2R/R ratio and leakage. Pure
 * arithmetic, about 13k float operations (94k instructions on the pi32v2). Never into a
 * table an active chip points at (sid_set_dac, sid.h). */
void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage);

/* sid_dac_build in SID_DAC_BUILD_STEPS calls of about equal cost (the first builds the
 * ladders, the waveform DAC's raw outputs and the envelope DAC; each later one finishes a
 * share of the waveform DAC), for a main loop that must not stall: start, then step until it
 * returns 1. The table is then sid_dac_build's, bit for bit (sid_dac_build is these steps);
 * until then it is half built, so the same rule holds: no chip points at it. */
#define SID_DAC_BUILD_STEPS 4u
typedef struct {
    sid_dac* d;
    float kink, leakage;
    float zero; /* the waveform DAC's output at the voice's zero, saturated */
    uint8_t is6581;
    uint8_t step; /* the next step, 0..SID_DAC_BUILD_STEPS */
} sid_dac_job;
void sid_dac_job_start(sid_dac_job* j, sid_dac* d, int is6581, float kink, float leakage);
/* Runs the next step: 1 once the table is done (and for every call after). */
int sid_dac_job_step(sid_dac_job* j);

/* The DACs at reSIDfp's defaults for a model, built on first use and shared by every
 * chip of that model (17,408 bytes each). Call from one context at a time on the
 * device (sid_init runs in the main loop); thread-safe on hosts. */
const sid_dac* sid_dac_default(int is6581);

#endif
