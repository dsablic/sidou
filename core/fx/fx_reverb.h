// SPDX-FileCopyrightText: 2014 Emilie Gillet <emilie.o.gillet@gmail.com>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: MIT
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.
#ifndef SIDOU_FX_REVERB_H
#define SIDOU_FX_REVERB_H

/* The light reverb: a C port of the reverb in pichenettes/eurorack (rings/dsp/fx/reverb.h
 * and fx_engine.h at f4cfa7697095a28d292bff758beb51deb49b5a86, MIT). The Griesinger
 * topology from Dattorro's paper: four allpass diffusers on the input, then a loop of two
 * branches, each a modulated delay read, a damping one-pole, two allpasses and a delay;
 * the two branches feed each other and give the left and right outputs. Every line lives
 * in one 16-bit ring with one write pointer that moves back a sample at a time.
 *
 * Changes from the original:
 * - C instead of C++ templates; the lines' bases are computed at init;
 * - delay lengths and modulation offsets scaled from 48 kHz to the sample rate (rounded);
 * - the LFOs are this project's phase-accumulator cosine (sid_fx_cos) instead of stmlib's
 *   CosineOscillator, still updated every 32 samples;
 * - the input is mono (FX slot: (L + R) / 2), so the input gain is doubled to match the
 *   original's L + R; the wet/dry amount is the slot's Mix (the output here is all wet);
 * - values are clamped into the int16 range before conversion (the original converts and
 *   then clips; the results are the same in range, and nothing undefined happens out of
 *   it).
 *
 * - reverb time capped at 0.82 (RT60 about 8 s) where Clouds goes to 0.98 (minutes):
 *   SIDOU's reverb is a song's tail, not a freeze.
 *
 * Controls (slot): Amount -> reverb time 0.35 .. 0.82, A -> damping (the loop low-pass
 * coefficient 0.9 .. 0.3), B -> diffusion 0.5 .. 0.75 (0.625 at 0.5, the original's). */

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_REVERB_LEN 32768u /* ring length (int16), the original's */
#define SID_FX_REVERB_LINES 10
#define SID_FX_REVERB_INPUT_GAIN 0.4f /* 0.2 on L + R in the original */

typedef struct {
    uint32_t base[SID_FX_REVERB_LINES];
    uint32_t length[SID_FX_REVERB_LINES];
    float del2_offset, del1_offset, del2_amp, del1_amp;
    int32_t write_ptr;
    uint32_t lfo_phase[2], lfo_inc[2]; /* the increments are for 32 samples */
    float lfo_value[2];
    float lp_decay_1, lp_decay_2;
} sid_fx_reverb;

void sid_fx_reverb_init(sid_fx_reverb* r, uint32_t sample_rate);
/* m is the mono input; buf the ring (SID_FX_REVERB_LEN samples, cleared); time, damping
 * and diffusion are the slot's Amount, A and B. */
void sid_fx_reverb_tick(sid_fx_reverb* r, int16_t* buf, float m, float time, float damping,
                        float diffusion, float* l, float* rr);

#ifdef __cplusplus
}
#endif

#endif
