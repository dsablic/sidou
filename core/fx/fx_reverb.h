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

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_REVERB_LEN 32768u
#define SID_FX_REVERB_LINES 10
#define SID_FX_REVERB_INPUT_GAIN 0.4f

typedef struct {
    uint32_t base[SID_FX_REVERB_LINES];
    uint32_t length[SID_FX_REVERB_LINES];
    float del2_offset, del1_offset, del2_amp, del1_amp;
    int32_t write_ptr;
    uint32_t lfo_phase[2], lfo_inc[2];
    float lfo_value[2];
    float lp_decay_1, lp_decay_2;
} sid_fx_reverb;

void sid_fx_reverb_init(sid_fx_reverb* r, uint32_t sample_rate);
void sid_fx_reverb_tick(sid_fx_reverb* r, int16_t* buf, float m, float time, float damping,
                        float diffusion, float* l, float* rr);

#ifdef __cplusplus
}
#endif

#endif
