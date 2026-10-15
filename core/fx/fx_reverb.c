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
#include "fx_reverb.h"

#define MASK (SID_FX_REVERB_LEN - 1u)

/* The original's lines at 48 kHz: ap1-ap4, dap1a, dap1b, del1, dap2a, dap2b, del2. */
static const float LENGTH_48K[SID_FX_REVERB_LINES] = {150, 214, 319, 527, 2182,
                                                      2690, 4501, 2525, 2197, 6312};
/* The original's STATIC_ASSERT(D::base + D::length <= size): its layout at 48 kHz, each
 * line one sample apart, fits the ring. */
#define LAYOUT_48K (150 + 214 + 319 + 527 + 2182 + 2690 + 4501 + 2525 + 2197 + 6312 + 10)
_Static_assert(LAYOUT_48K <= SID_FX_REVERB_LEN, "the reverb's lines do not fit its ring");
enum { AP1, AP2, AP3, AP4, DAP1A, DAP1B, DEL1, DAP2A, DAP2B, DEL2 };

void sid_fx_reverb_init(sid_fx_reverb* r, uint32_t sample_rate) {
    /* Scaled to the sample rate, but never past the ring: above about 70 kHz the lines
     * keep their length in samples (shorter in time, a smaller room). */
    float scale = (float)sample_rate / 48000.0f;
    float room = (float)(SID_FX_REVERB_LEN - 64u) / (float)LAYOUT_48K;
    if (scale > room) {
        scale = room;
    }
    uint32_t base = 0;
    for (int i = 0; i < SID_FX_REVERB_LINES; ++i) {
        uint32_t len = (uint32_t)(int32_t)(LENGTH_48K[i] * scale + 0.5f);
        r->base[i] = base;
        r->length[i] = len;
        base += len + 1u; /* the original's DelayLine base: previous base + length + 1 */
    }
    r->del2_offset = 6261.0f * scale;
    r->del1_offset = 4460.0f * scale;
    r->del2_amp = 50.0f * scale;
    r->del1_amp = 40.0f * scale;
    r->write_ptr = 0;
    /* LFO_1 0.5 Hz, LFO_2 0.3 Hz, stepped every 32 samples. */
    r->lfo_inc[0] = sid_fx_phase_inc(0.5f * 32.0f, sample_rate);
    r->lfo_inc[1] = sid_fx_phase_inc(0.3f * 32.0f, sample_rate);
    r->lfo_phase[0] = 0;
    r->lfo_phase[1] = 0;
    r->lfo_value[0] = 1.0f;
    r->lfo_value[1] = 1.0f;
    r->lp_decay_1 = 0.0f;
    r->lp_decay_2 = 0.0f;
}

/* ---- the FX engine's context, as plain functions ---- */

typedef struct {
    int16_t* buf;
    const sid_fx_reverb* r;
    int32_t wp;
    float acc;
    float previous_read;
} ctx;

static inline float decompress(int16_t v) {
    return (float)v * (1.0f / 32768.0f);
}

static inline int16_t compress(float v) {
    float s = v * 32768.0f;
    if (s > 32767.0f) {
        s = 32767.0f;
    } else if (s < -32768.0f) {
        s = -32768.0f;
    }
    return (int16_t)(int32_t)s; /* truncation, as the original's static_cast<int32_t> */
}

static inline uint32_t at(const ctx* c, int line, int32_t offset) {
    return (uint32_t)(c->wp + (int32_t)c->r->base[line] + offset) & MASK;
}

/* Read(line TAIL, scale) */
static inline void read_tail(ctx* c, int line, float scale) {
    float v = decompress(c->buf[at(c, line, (int32_t)c->r->length[line] - 1)]);
    c->previous_read = v;
    c->acc += v * scale;
}

/* Write(line, scale): at the line's head (offset 0) */
static inline void write_line(ctx* c, int line, float scale) {
    c->buf[at(c, line, 0)] = compress(c->acc);
    c->acc *= scale;
}

static inline void write_all_pass(ctx* c, int line, float scale) {
    write_line(c, line, scale);
    c->acc += c->previous_read;
}

/* Interpolate(line, offset, LFO, amplitude, scale) */
static inline void interpolate(ctx* c, int line, float offset, float lfo, float amplitude,
                               float scale) {
    offset += amplitude * lfo;
    int32_t integral = (int32_t)offset;
    float fractional = offset - (float)integral;
    float a = decompress(c->buf[at(c, line, integral)]);
    float b = decompress(c->buf[at(c, line, integral + 1)]);
    float x = a + (b - a) * fractional;
    c->previous_read = x;
    c->acc += x * scale;
}

static inline void lp(ctx* c, float* state, float coefficient) {
    *state = sid_fx_flush(*state + coefficient * (c->acc - *state));
    c->acc = *state;
}

void sid_fx_reverb_tick(sid_fx_reverb* r, int16_t* buf, float m, float time, float damping,
                        float diffusion, float* l, float* rr) {
    const float kap = 0.5f + 0.25f * diffusion;
    const float klp = 0.9f - 0.6f * damping;
    const float krt = 0.35f + 0.47f * time;

    /* engine_.Start(): the write pointer moves back; the LFOs step every 32 samples. */
    if (--r->write_ptr < 0) {
        r->write_ptr += (int32_t)SID_FX_REVERB_LEN;
    }
    if ((r->write_ptr & 31) == 0) {
        for (int i = 0; i < 2; ++i) {
            r->lfo_phase[i] += r->lfo_inc[i];
            r->lfo_value[i] = sid_fx_cos(r->lfo_phase[i]);
        }
    }
    ctx c = {buf, r, r->write_ptr, 0.0f, 0.0f};

    c.acc += m * SID_FX_REVERB_INPUT_GAIN;

    /* Diffuse through 4 allpasses. */
    read_tail(&c, AP1, kap);
    write_all_pass(&c, AP1, -kap);
    read_tail(&c, AP2, kap);
    write_all_pass(&c, AP2, -kap);
    read_tail(&c, AP3, kap);
    write_all_pass(&c, AP3, -kap);
    read_tail(&c, AP4, kap);
    write_all_pass(&c, AP4, -kap);
    float apout = c.acc;

    /* Main reverb loop. */
    c.acc = apout;
    interpolate(&c, DEL2, r->del2_offset, r->lfo_value[1], r->del2_amp, krt);
    lp(&c, &r->lp_decay_1, klp);
    read_tail(&c, DAP1A, -kap);
    write_all_pass(&c, DAP1A, kap);
    read_tail(&c, DAP1B, kap);
    write_all_pass(&c, DAP1B, -kap);
    write_line(&c, DEL1, 2.0f);
    *l = c.acc;

    c.acc = apout;
    interpolate(&c, DEL1, r->del1_offset, r->lfo_value[0], r->del1_amp, krt);
    lp(&c, &r->lp_decay_2, klp);
    read_tail(&c, DAP2A, kap);
    write_all_pass(&c, DAP2A, -kap);
    read_tail(&c, DAP2B, -kap);
    write_all_pass(&c, DAP2B, kap);
    write_line(&c, DEL2, 2.0f);
    *rr = c.acc;
}
