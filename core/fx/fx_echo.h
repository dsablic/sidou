// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_ECHO_H
#define SIDOU_FX_ECHO_H

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
#define SID_FX_ECHO_GLIDE_TURN 256.0f
#define SID_FX_ECHO_GLIDE_EASE 64.0f
#define SID_FX_ECHO_DAMP_MAX_HZ 18000.0f
#define SID_FX_ECHO_DAMP_LN_RANGE 2.8903718f
#define SID_FX_ECHO_DAMP_STEPS 16u

typedef struct {
    sid_fx_delay line;
    uint32_t pingpong;
    uint32_t fresh;
    float min_time;
    float octaves;
    float time;
    float speed;
    float time_amount;
    float target;
    float damp_table[SID_FX_ECHO_DAMP_STEPS + 1u];
    float damp, damp_a, lp;
} sid_fx_echo;

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
    float octaves = 0.0f;
    float r = max_time / e->min_time;
    while (r >= 2.0f) {
        r *= 0.5f;
        octaves += 1.0f;
    }
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
    sid_fx_delay16_push(&e->line, buf, sidou_soft_clip(m + fb * e->lp));
}

#ifdef __cplusplus
}
#endif

#endif
