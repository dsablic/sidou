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
 * arithmetic, about 13k float operations. */
void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage);

/* The DACs at reSIDfp's defaults for a model, built on first use and shared by every
 * chip of that model (17,408 bytes each). Call from one context at a time on the
 * device (sid_init runs in the main loop); thread-safe on hosts. */
const sid_dac* sid_dac_default(int is6581);

#endif
