// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_DAC_H
#define SIDOU_SID_DAC_H

#include <stdint.h>

/* The voice DACs, as reSIDfp models them (Dac::kinkedDac): R-2R ladders whose bit
 * weights are generated over a grid of the 2R/R ratio (gen/dac.h). A voice outputs
 * wave[waveform output] * env[envelope counter]. */
typedef struct {
    /* The waveform DAC (6581: saturating) minus its value at the voice's zero, plus the
     * 6581's voice DC, which follows the envelope DAC, so it rides on env too. In units
     * where a full-scale 8580 voice spans -1 to 1. */
    float wave[4096];
    float env[256]; /* the envelope DAC, about counter / 255 */
} sid_dac;

/* Builds both DACs for a model from the ladder's 2R/R ratio (`kink`, clamped to the
 * grid in gen/dac.h; reSIDfp: SID_DAC_KINK_6581/_8580) and the MOSFET leakage (the
 * fraction of its weight an off bit still adds; SID_DAC_LEAKAGE_6581/_8580). */
void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage);
/* sid_dac_build at reSIDfp's defaults for the model. */
void sid_dac_build_default(sid_dac* d, int is6581);

#endif
