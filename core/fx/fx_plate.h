// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_PLATE_H
#define SIDOU_FX_PLATE_H

/* The lush reverb: Dattorro's plate (J. Dattorro, "Effect Design, Part 1: Reverberator
 * and Other Filters", JAES 45(9), 1997, figure 1 and table 2), reimplemented from the
 * paper: a bandwidth low-pass, four input diffusers, then a figure-8 tank of two halves,
 * each a modulated allpass, a delay, a damping low-pass, the decay gain, a second allpass
 * and a second delay, crossing into the other half. Seven taps per side.
 *
 * Every line is float in one ring of SID_FX_PLATE_LEN (the 128 KiB FX pool) with one write
 * pointer; float, so reads and writes need no conversion. The paper's lengths at
 * 29,761 Hz are scaled to the sample rate and then by FIT so they fill but do not exceed
 * the ring (0.98 at 44.1 kHz). Values written are flushed so the tank never decays into
 * denormals.
 *
 * - Amount: decay, 0.30 .. 0.85 (RT60 about 8 s at the top; the paper's default is 0.5);
 *   decay diffusion 2 follows it as in the paper (decay + 0.15, within 0.25 .. 0.5);
 * - A: damping, 0.05 .. 0.7 (the tank low-pass y = (1 - d) x + d y);
 * - B: bandwidth, the input low-pass coefficient from 0.3 (dark) to the paper's 0.9995.
 * Input diffusion 0.75 / 0.625 and decay diffusion 1 0.70 are the paper's. The modulated
 * allpasses swing by 16 samples (scaled) at about 1 Hz, the two halves in quadrature, the
 * LFO evaluated every SEGMENT samples and the read offsets moving linearly in between.
 * The output taps are rounded to whole samples. There is no pre-delay: at 0.98 the tank
 * fills the ring. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_PLATE_LEN 32768u /* floats */
#define SID_FX_PLATE_LINES 12
#define SID_FX_PLATE_LFO_HZ 1.0f
#define SID_FX_PLATE_SEGMENT 16u

typedef struct {
    uint32_t base[SID_FX_PLATE_LINES];
    uint32_t length[SID_FX_PLATE_LINES];
    uint32_t tap[14]; /* output tap offsets, scaled and rounded */
    float excursion;  /* modulated allpass swing, samples */
    uint32_t wp;      /* write pointer, moves back one per sample */
    uint32_t lfo_phase, lfo_inc; /* lfo_inc is per segment */
    uint32_t left;               /* samples left in the LFO segment */
    float mod_l, mod_r, step_l, step_r; /* modulated allpass read offsets and slopes */
    float bw, damp_l, damp_r;           /* filter states */
    float tank_l, tank_r;               /* each half's output, fed to the other */
} sid_fx_plate;

void sid_fx_plate_init(sid_fx_plate* p, uint32_t sample_rate);
/* m is the mono input; buf the ring (SID_FX_PLATE_LEN floats, cleared). */
void sid_fx_plate_tick(sid_fx_plate* p, float* buf, float m, float decay, float damping,
                       float bandwidth, float* l, float* r);

#ifdef __cplusplus
}
#endif

#endif
