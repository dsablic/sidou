// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_ECHO_H
#define SIDOU_FX_ECHO_H

/* Delay, mono in, stereo out, on one int16 line (the FX pool). Mono: one tap at T on both
 * sides. Ping-pong: taps at T (left) and 2T (right) with the feedback from 2T, so echoes
 * alternate left, right, left at T apart from a single line. The feedback goes through
 * a one-pole damping low-pass, and what is written (input plus feedback) through the soft
 * clip, so the loop saturates smoothly and the line never reaches its rails.
 *
 * - Amount: feedback, 0 .. 0.95;
 * - A: time, MIN_MS .. the line (1.49 s mono, 0.74 s ping-pong at 44.1 kHz), log;
 * - B: damping, the feedback low-pass from 18 kHz (0) down to 1 kHz (1), log.
 * The time glides instead of jumping: its speed eases up to at most GLIDE samples per
 * sample over GLIDE_TURN samples and eases off approaching the target (a tape-style pitch
 * bend of at most an octave down or a fifth up, which itself starts and ends smoothly),
 * so moving it never clicks; a fresh start takes the asked time at once. The whole write
 * (input plus feedback) goes through the soft clip, so a hot loop saturates smoothly; at
 * ordinary levels the clip's cubic is the only colouring (under 1 % at 0.25 full scale).
 * Any amounts in [0, 1] may arrive on any sample. */

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"
#include "sid/sid_math.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_ECHO_MIN_MS 10.0f
#define SID_FX_ECHO_FEEDBACK 0.95f
#define SID_FX_ECHO_GLIDE 0.5f
#define SID_FX_ECHO_GLIDE_TURN 256.0f /* samples for the glide speed to go from 0 to full */
#define SID_FX_ECHO_GLIDE_EASE 64.0f  /* samples: the approach slows within this of the end */
#define SID_FX_ECHO_DAMP_MAX_HZ 18000.0f
#define SID_FX_ECHO_DAMP_LN_RANGE 2.8903718f /* ln(18000 / 1000) */
#define SID_FX_ECHO_DAMP_STEPS 16u

typedef struct {
    sid_fx_delay line;
    uint32_t pingpong;
    uint32_t fresh;    /* no tick yet: take the asked time at once */
    float min_time;    /* samples */
    float octaves;     /* log2(max time / min time) */
    float time;        /* the current (gliding) tap time, samples */
    float speed;       /* its change per sample */
    float time_amount; /* the A the target was made for */
    float target;
    float damp_table[SID_FX_ECHO_DAMP_STEPS + 1u];
    float damp, damp_a, lp;
} sid_fx_echo;

/* `len` is the line's length (a power of two): the pool. */
static inline void sid_fx_echo_init(sid_fx_echo* e, uint32_t sample_rate, uint32_t len,
                                    int pingpong) {
    float fs = (float)sample_rate;
    sid_fx_delay_init(&e->line, len);
    e->pingpong = pingpong ? 1u : 0u;
    e->fresh = 1;
    e->min_time = SID_FX_ECHO_MIN_MS * 0.001f * fs;
    float max_time = (float)(len - 4u);
    if (pingpong) {
        max_time *= 0.5f;
    }
    /* log2 of the ratio, by halving: no libm. */
    float octaves = 0.0f;
    float r = max_time / e->min_time;
    while (r >= 2.0f) {
        r *= 0.5f;
        octaves += 1.0f;
    }
    /* log2(r) for r in [1, 2): ln(r) / ln 2 by the series of atanh. */
    float z = (r - 1.0f) / (r + 1.0f);
    float z2 = z * z;
    octaves += 2.8853901f * z * (1.0f + z2 * (1.0f / 3.0f + z2 * (0.2f + z2 / 7.0f)));
    e->octaves = octaves;
    e->time = e->min_time;
    e->speed = 0.0f;
    e->target = e->min_time;
    e->time_amount = -1.0f;
    float w_scale = 2.0f * SIDOU_PI / fs;
    float limit = 0.45f * fs;
    for (uint32_t i = 0; i <= SID_FX_ECHO_DAMP_STEPS; ++i) {
        float b = (float)i / (float)SID_FX_ECHO_DAMP_STEPS;
        float hz = SID_FX_ECHO_DAMP_MAX_HZ * sid_fx_exp_init(-SID_FX_ECHO_DAMP_LN_RANGE * b);
        e->damp_table[i] = 1.0f - sid_fx_exp_init(-w_scale * (hz < limit ? hz : limit));
    }
    e->damp = -1.0f;
    e->damp_a = 1.0f;
    e->lp = 0.0f;
}

/* m is the mono input; buf the line. */
static inline void sid_fx_echo_tick(sid_fx_echo* e, int16_t* buf, float m, float feedback,
                                    float time, float damping, float* l, float* r) {
    if (time != e->time_amount) {
        e->time_amount = time;
        e->target = e->min_time * sid_fx_exp2(e->octaves * time);
    }
    if (e->fresh) {
        e->fresh = 0;
        e->time = e->target;
    } else if (e->time != e->target || e->speed != 0.0f) {
        /* Ping-pong's 2T tap moves twice as fast: halve its glide so it stays within the
         * same bend (at 1 sample per sample it would freeze). */
        float max = e->pingpong ? 0.5f * SID_FX_ECHO_GLIDE : SID_FX_ECHO_GLIDE;
        float want = sid_fx_slew(0.0f, (e->target - e->time) * (1.0f / SID_FX_ECHO_GLIDE_EASE),
                                 max);
        e->speed = sid_fx_slew(e->speed, want, max * (1.0f / SID_FX_ECHO_GLIDE_TURN));
        e->time += e->speed;
        float d = e->target - e->time;
        if (d < 1e-3f && d > -1e-3f && e->speed < 1e-4f && e->speed > -1e-4f) {
            e->time = e->target;
            e->speed = 0.0f;
        }
    }
    if (damping != e->damp) {
        e->damp = damping;
        float pos = damping * (float)SID_FX_ECHO_DAMP_STEPS;
        int32_t i = (int32_t)pos;
        if (i >= (int32_t)SID_FX_ECHO_DAMP_STEPS) {
            i = (int32_t)SID_FX_ECHO_DAMP_STEPS - 1;
        }
        e->damp_a = sid_fx_lerp(e->damp_table[i], e->damp_table[i + 1], pos - (float)i);
    }
    float t1 = sid_fx_delay16_read(&e->line, buf, e->time);
    float back = t1;
    if (e->pingpong) {
        back = sid_fx_delay16_read(&e->line, buf, 2.0f * e->time);
        *l = t1;
        *r = back;
    } else {
        *l = t1;
        *r = t1;
    }
    e->lp = sid_fx_flush(e->lp + e->damp_a * (back - e->lp));
    float fb = SID_FX_ECHO_FEEDBACK * feedback;
    /* The whole write goes through the soft clip, so a hot input on top of the feedback
     * saturates smoothly instead of hitting the int16 line's rails. */
    sid_fx_delay16_push(&e->line, buf, sidou_soft_clip(m + fb * e->lp));
}

#ifdef __cplusplus
}
#endif

#endif
