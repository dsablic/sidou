// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_CHORUS_H
#define SIDOU_FX_CHORUS_H

/* Chorus/ensemble, mono in, stereo out: one delay line read by three taps per side,
 * modulated by a slow (~0.6 Hz) and a fast (~6 Hz) sine, the taps 120 degrees apart and
 * the right side's offset by 60 (the string-machine ensemble; reimplemented from
 * Dattorro, "Effect Design" part 2, with the topology of Plaits' ensemble (Emilie Gillet,
 * MIT) as a reference). The phase-shifted sines come from one sin/cos pair per LFO by
 * rotation, so six taps cost two sine evaluations per LFO.
 *
 * - Amount: depth, the excursion around the centre delay (up to SLOW_MS and FAST_MS);
 * - A: rate, 0.25x .. 2x the base rates (log), so A = 2/3 is 0.6 Hz and 6 Hz;
 * - B: ensemble, how much of the fast LFO is in (0: a plain slow chorus).
 * Any amounts in [0, 1] may arrive on any sample. The LFOs, depth and rate are evaluated
 * every SID_FX_MOD_SEGMENT samples, at the segment's end, and the tap delays move
 * linearly towards those values in between; the depth goes through one more one-pole
 * here (per segment), so even a smoothed jump changes the taps' read speed (their pitch)
 * a little per segment rather than all at once. */

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_CHORUS_LEN 1024u
#define SID_FX_CHORUS_CENTRE_MS 7.0f
#define SID_FX_CHORUS_SLOW_HZ 0.6f
#define SID_FX_CHORUS_FAST_HZ 6.0f
#define SID_FX_CHORUS_SLOW_MS 3.0f
#define SID_FX_CHORUS_FAST_MS 0.4f
#define SID_FX_MOD_SEGMENT 16u /* samples between LFO evaluations (modulation blocks) */

typedef struct {
    float buf[SID_FX_CHORUS_LEN];
    sid_fx_delay line;
    uint32_t slow_phase, fast_phase, slow_inc, fast_inc;
    float slow_hz_inc, fast_hz_inc; /* base increments as floats, 2^32 per turn / 2 */
    float centre, slow_depth, fast_depth; /* samples */
    float depth, depth_coef;              /* depth after the block's own smoothing */
    float delay[6];                       /* tap delays: L0 L1 L2 R0 R1 R2 */
    float step[6];                        /* their change per sample in this segment */
    uint32_t left;                        /* samples left in the segment */
} sid_fx_chorus;

static inline void sid_fx_chorus_set_rate(sid_fx_chorus* c, float rate) {
    float mult = 0.25f * sid_fx_exp2(3.0f * rate);
    c->slow_inc = (uint32_t)(int32_t)(c->slow_hz_inc * mult) << 1;
    c->fast_inc = (uint32_t)(int32_t)(c->fast_hz_inc * mult) << 1;
}

/* At a segment start: advance the LFOs to its end and aim the taps there. */
static inline void sid_fx_chorus_plan(sid_fx_chorus* c, float depth, float rate, float ensemble) {
    sid_fx_chorus_set_rate(c, rate);
    c->slow_phase += c->slow_inc * SID_FX_MOD_SEGMENT;
    c->fast_phase += c->fast_inc * SID_FX_MOD_SEGMENT;
    c->depth += c->depth_coef * (depth - c->depth);
    const float h = 0.8660254f; /* sin 60 and 120 degrees */
    float ss = sid_fx_sin(c->slow_phase);
    float sc = sid_fx_cos(c->slow_phase);
    float fs_ = sid_fx_sin(c->fast_phase);
    float fc = sid_fx_cos(c->fast_phase);
    float ds = c->depth * c->slow_depth;
    float df = c->depth * ensemble * c->fast_depth;
    /* sin(t + k 60 degrees) for k = 0, 2, 4 (left) and 1, 3, 5 (right). */
    float s1 = 0.5f * ss + h * sc, f1 = 0.5f * fs_ + h * fc;
    float s2 = -0.5f * ss + h * sc, f2 = -0.5f * fs_ + h * fc;
    float to[6] = {c->centre + ds * ss + df * fs_, c->centre + ds * s2 + df * f2,
                   c->centre - ds * s1 - df * f1,  c->centre + ds * s1 + df * f1,
                   c->centre - ds * ss - df * fs_, c->centre - ds * s2 - df * f2};
    for (int i = 0; i < 6; ++i) {
        c->step[i] = (to[i] - c->delay[i]) * (1.0f / (float)SID_FX_MOD_SEGMENT);
    }
    c->left = SID_FX_MOD_SEGMENT;
}

/* sid_fx_chorus_init but for the line's samples, which it leaves as they are (the effect
 * slots keep this part as a template and clear the line at a start: fx_slots.c). */
static inline void sid_fx_chorus_setup(sid_fx_chorus* c, uint32_t sample_rate) {
    float fs = (float)sample_rate;
    sid_fx_delay_init(&c->line, SID_FX_CHORUS_LEN);
    c->slow_phase = 0;
    c->fast_phase = 0x2aaaaaabu; /* start the two LFOs out of step */
    c->slow_hz_inc = SID_FX_CHORUS_SLOW_HZ / fs * 2147483648.0f;
    c->fast_hz_inc = SID_FX_CHORUS_FAST_HZ / fs * 2147483648.0f;
    c->centre = SID_FX_CHORUS_CENTRE_MS * 0.001f * fs;
    c->slow_depth = SID_FX_CHORUS_SLOW_MS * 0.001f * fs;
    c->fast_depth = SID_FX_CHORUS_FAST_MS * 0.001f * fs;
    for (int i = 0; i < 6; ++i) {
        c->delay[i] = c->centre;
        c->step[i] = 0.0f;
    }
    c->depth = 0.0f;
    /* The one-pole per segment: 1 - (1 - k)^SEGMENT. */
    float k = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    float keep = 1.0f;
    for (uint32_t i = 0; i < SID_FX_MOD_SEGMENT; ++i) {
        keep *= 1.0f - k;
    }
    c->depth_coef = 1.0f - keep;
    c->left = 0;
    sid_fx_chorus_set_rate(c, 2.0f / 3.0f);
}

static inline void sid_fx_chorus_init(sid_fx_chorus* c, uint32_t sample_rate) {
    for (uint32_t i = 0; i < SID_FX_CHORUS_LEN; ++i) {
        c->buf[i] = 0.0f;
    }
    sid_fx_chorus_setup(c, sample_rate);
}

/* x is normalised; depth, rate and ensemble are in [0, 1]. */
static inline void sid_fx_chorus_tick(sid_fx_chorus* c, float x, float depth, float rate,
                                      float ensemble, float* l, float* r) {
    if (c->left == 0) {
        sid_fx_chorus_plan(c, depth, rate, ensemble);
    }
    --c->left;
    sid_fx_delay_push(&c->line, c->buf, x);
    for (int i = 0; i < 6; ++i) {
        c->delay[i] += c->step[i];
    }
    const float third = 1.0f / 3.0f;
    *l = third * (sid_fx_delay_read(&c->line, c->buf, c->delay[0]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[1]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[2]));
    *r = third * (sid_fx_delay_read(&c->line, c->buf, c->delay[3]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[4]) +
                  sid_fx_delay_read(&c->line, c->buf, c->delay[5]));
}

#ifdef __cplusplus
}
#endif

#endif
