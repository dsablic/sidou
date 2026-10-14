// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_PLATE_H
#define SIDOU_FX_PLATE_H

/* The lush reverb: Dattorro's plate (J. Dattorro, "Effect Design, Part 1: Reverberator
 * and Other Filters", JAES 45(9), 1997, figure 1 and table 2), reimplemented from the
 * paper: pre-delay, a bandwidth low-pass, four input diffusers, then a figure-8 tank of two
 * halves, each a modulated allpass, a delay, a damping low-pass, the decay gain, a second
 * allpass and a second delay, crossing into the other half. Seven taps per side. Lengths
 * are the paper's at 29,761 Hz scaled to the sample rate; every line is int16 in one ring
 * (the FX pool) with one write pointer.
 *
 * - Amount: decay, 0.30 .. 0.85 (RT60 about 8 s at the top; the paper's default is 0.5);
 *   decay diffusion 2 follows
 *   it as in the paper (decay + 0.15, kept within 0.25 .. 0.5);
 * - A: damping, 0.05 .. 0.7 (the tank low-pass y = (1 - d) x + d y);
 * - B: pre-delay, 0 .. PREDELAY_MS, gliding (like the delay's time) so it never clicks.
 * Bandwidth 0.9995, input diffusion 0.75 / 0.625 and decay diffusion 1 0.70 are the
 * paper's. The modulated allpasses swing by 16 samples (scaled) at about 1 Hz, the two
 * halves in quadrature. */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_PLATE_LEN 65536u
#define SID_FX_PLATE_LINES 13
#define SID_FX_PLATE_PREDELAY_MS 100.0f
#define SID_FX_PLATE_LFO_HZ 1.0f
#define SID_FX_PLATE_GLIDE 0.5f

typedef struct {
    uint32_t base[SID_FX_PLATE_LINES];
    uint32_t length[SID_FX_PLATE_LINES];
    float tap[14];   /* output tap offsets, scaled */
    float excursion; /* modulated allpass swing, samples */
    float max_predelay, predelay, predelay_amount, predelay_target;
    uint32_t wp; /* write pointer, moves back one per sample */
    uint32_t lfo_phase, lfo_inc;
    float bw, damp_l, damp_r; /* filter states */
    float tank_l, tank_r;     /* each half's output, fed to the other */
} sid_fx_plate;

void sid_fx_plate_init(sid_fx_plate* p, uint32_t sample_rate);
/* m is the mono input; buf the ring (SID_FX_PLATE_LEN samples, cleared). */
void sid_fx_plate_tick(sid_fx_plate* p, int16_t* buf, float m, float decay, float damping,
                       float predelay, float* l, float* r);

#ifdef __cplusplus
}
#endif

#endif
