// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_WOW_H
#define SIDOU_FX_WOW_H

/* Wow and flutter: a delay line read at a modulated fractional delay. A delay changing at
 * rate D' shifts the pitch by the ratio 1 - D', so a term A (1 - cos wt) / 2 deviates by
 * at most A w / 2; the depths below are set from cents that way. Every term is >= 0 and
 * starts at 0, so at rest the delay is 0 and the block is the identity.
 *
 * - wow: a ~0.5 Hz sine plus a random drift (a new random target every 0.25 s through two
 *   cascaded one-poles at ~0.5 Hz, so the pitch moves smoothly);
 * - flutter: an ~8 Hz sine.
 *
 * The delay is slew-limited to SID_FX_WOW_SLEW samples per sample (about 50 cents), so a
 * depth jump bends the pitch briefly instead of clicking, and the block stays active
 * after its depths reach 0 until the delay has glided back to 0. */

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_WOW_LEN 1024u /* 23 ms at 44.1 kHz; a power of two */
#define SID_FX_WOW_HZ 0.5f
#define SID_FX_WOW_CENTS 20.0f
#define SID_FX_WOW_DRIFT_MS 6.0f  /* drift range at depth 1, in delay */
#define SID_FX_WOW_DRIFT_HZ 0.5f
#define SID_FX_FLUTTER_HZ 8.0f
#define SID_FX_FLUTTER_CENTS 8.0f
#define SID_FX_WOW_SLEW 0.03f

typedef struct {
    float buf[SID_FX_WOW_LEN];
    sid_fx_delay line;
    uint32_t rng;
    uint32_t wow_phase, wow_inc;
    uint32_t flutter_phase, flutter_inc;
    uint32_t drift_count, drift_period;
    float wow_depth, drift_depth, flutter_depth; /* in samples, at amount 1 */
    float drift_coef, drift_target, drift1, drift2;
    float delay; /* current delay in samples */
} sid_fx_wow;

static inline void sid_fx_wow_reset(sid_fx_wow* w) {
    w->wow_phase = 0;
    w->flutter_phase = 0;
    w->drift1 = 0.0f;
    w->drift2 = 0.0f;
    w->drift_count = w->drift_period;
    w->drift_target = sid_fx_rng_unit(&w->rng);
    w->delay = 0.0f;
}

/* Depth in samples of a (1 - cos) / 2 term at `hz` that deviates by `cents`. */
static inline float sid_fx_wow_depth(float cents, float hz, uint32_t sample_rate) {
    /* 2^(c/1200) - 1 to second order; under 0.1 % off below 50 cents. */
    float x = cents * (0.69314718f / 1200.0f);
    float ratio = x * (1.0f + 0.5f * x);
    return 2.0f * ratio * (float)sample_rate / (2.0f * SIDOU_PI * hz);
}

static inline void sid_fx_wow_init(sid_fx_wow* w, uint32_t sample_rate, uint32_t seed) {
    sid_fx_delay_init(&w->line, SID_FX_WOW_LEN);
    for (uint32_t i = 0; i < SID_FX_WOW_LEN; ++i) {
        w->buf[i] = 0.0f;
    }
    w->rng = sid_fx_rng_seed(seed);
    w->wow_inc = sid_fx_phase_inc(SID_FX_WOW_HZ, sample_rate);
    w->flutter_inc = sid_fx_phase_inc(SID_FX_FLUTTER_HZ, sample_rate);
    w->drift_period = sample_rate / 4u;
    w->drift_coef = 2.0f * SIDOU_PI * SID_FX_WOW_DRIFT_HZ / (float)sample_rate;
    w->wow_depth = sid_fx_wow_depth(SID_FX_WOW_CENTS, SID_FX_WOW_HZ, sample_rate);
    w->drift_depth = SID_FX_WOW_DRIFT_MS * 0.001f * (float)sample_rate;
    w->flutter_depth = sid_fx_wow_depth(SID_FX_FLUTTER_CENTS, SID_FX_FLUTTER_HZ, sample_rate);
    /* High sample rates: shrink the depths to fit the line. */
    float total = w->wow_depth + w->drift_depth + w->flutter_depth;
    float room = (float)(SID_FX_WOW_LEN - 2u);
    if (total > room) {
        float scale = room / total;
        w->wow_depth *= scale;
        w->drift_depth *= scale;
        w->flutter_depth *= scale;
    }
    sid_fx_wow_reset(w);
}

/* True while the block still changes the signal with both depths at 0. */
static inline int sid_fx_wow_busy(const sid_fx_wow* w) {
    return w->delay != 0.0f;
}

/* x is normalised to [-1, 1); wow and flutter are depths in [0, 1]. */
static inline float sid_fx_wow_tick(sid_fx_wow* w, float x, float wow, float flutter) {
    sid_fx_delay_push(&w->line, w->buf, x);

    float slow = 0.5f - 0.5f * sid_fx_cos(w->wow_phase);
    float fast = 0.5f - 0.5f * sid_fx_cos(w->flutter_phase);
    w->wow_phase += w->wow_inc;
    w->flutter_phase += w->flutter_inc;
    if (--w->drift_count == 0) {
        w->drift_count = w->drift_period;
        w->drift_target = sid_fx_rng_unit(&w->rng);
    }
    w->drift1 += w->drift_coef * (w->drift_target - w->drift1);
    w->drift2 += w->drift_coef * (w->drift1 - w->drift2);

    float target = wow * (w->wow_depth * slow + w->drift_depth * w->drift2) +
                   flutter * (w->flutter_depth * fast);
    float d = target - w->delay;
    if (d > SID_FX_WOW_SLEW) {
        w->delay += SID_FX_WOW_SLEW;
    } else if (d < -SID_FX_WOW_SLEW) {
        w->delay -= SID_FX_WOW_SLEW;
    } else {
        w->delay = target;
    }
    return sid_fx_delay_read(&w->line, w->buf, w->delay);
}

#ifdef __cplusplus
}
#endif

#endif
