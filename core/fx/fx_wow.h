// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_WOW_H
#define SIDOU_FX_WOW_H

/* Wow and flutter: a delay line read at a fixed centre delay plus a modulated excursion.
 * A delay changing at rate D' shifts the pitch by the ratio 1 - D', so an excursion
 * A sin(wt) deviates by at most A w; the depths below are set from cents that way.
 *
 * - wow: a ~0.5 Hz sine plus a random drift (a new random target in [-1, 1] every 0.25 s
 *   through two cascaded one-poles at ~0.5 Hz, so the pitch moves smoothly);
 * - flutter: an ~8 Hz sine.
 *
 * Only the excursion moves; the centre delay (half the line's worth of excursion) stays
 * put while the block is active, so the average pitch never shifts. Entering, the block
 * fills the line with CENTRE samples while passing the input dry, then crossfades from
 * dry to the delayed signal over SID_FX_WOW_FADE_MS (a smoothstep of a position whose
 * speed itself ramps over SID_FX_WOW_FADE_TURN samples, so the envelope has no corner,
 * not even when the direction reverses mid-fade); leaving (both depths at 0), it
 * crossfades back to dry. The depths given to the tick may jump: the block slews its own
 * copies, wow over SID_FX_WOW_DEPTH_S per full range (a deck settling: bounded pitch
 * disturbance, about 12 cents at most) and flutter over SID_FX_FLUTTER_DEPTH_MS.
 *
 * No stale reads: after a reset the line holds old samples, and the read must never
 * reach them. The fade starts once `centre` samples are in, and from then the depths
 * start at 0 and grow by at most their slew per sample, so m samples later the delay
 * is at most centre + m (wow_slew (wow_depth + drift_depth) + flutter_slew
 * flutter_depth) = centre + 0.0085 m at 44.1 kHz, always short of the centre + m
 * samples written. This holds while those slews times depths stay below 1 sample per
 * sample; faster slews would need the line cleared on reset. */

#include <stdint.h>

#include "fx_delay.h"
#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_WOW_LEN 1024u /* 23 ms at 44.1 kHz; a power of two */
#define SID_FX_WOW_HZ 0.5f
#define SID_FX_WOW_CENTS 20.0f
#define SID_FX_WOW_DRIFT_MS 3.0f /* drift excursion at depth 1 */
#define SID_FX_WOW_DRIFT_HZ 0.5f
#define SID_FX_FLUTTER_HZ 8.0f
#define SID_FX_FLUTTER_CENTS 8.0f
#define SID_FX_WOW_FADE_MS 10.0f
#define SID_FX_WOW_FADE_TURN 32.0f
#define SID_FX_WOW_DEPTH_S 1.0f
#define SID_FX_FLUTTER_DEPTH_MS 50.0f

typedef struct {
    float buf[SID_FX_WOW_LEN];
    sid_fx_delay line;
    uint32_t rng;
    uint32_t wow_phase, wow_inc;
    uint32_t flutter_phase, flutter_inc;
    uint32_t drift_count, drift_period;
    uint32_t filled, centre_samples; /* samples pushed since reset; needed before fading in */
    float wow_depth, drift_depth, flutter_depth; /* excursion amplitudes in samples */
    float centre;                                /* the fixed delay, in samples */
    float drift_coef, drift_target, drift1, drift2;
    float wow_slew, flutter_slew, fade_step, fade_accel;
    float wow, flutter; /* slewed depths */
    float fade;         /* crossfade position, 0 dry .. 1 delayed */
    float fade_v;       /* its speed per sample */
    float delay;        /* last read delay, for inspection */
} sid_fx_wow;

static inline void sid_fx_wow_reset(sid_fx_wow* w) {
    w->wow_phase = 0;
    w->flutter_phase = 0;
    w->drift1 = 0.0f;
    w->drift2 = 0.0f;
    w->drift_count = w->drift_period;
    w->drift_target = sid_fx_rng_bipolar(&w->rng);
    w->wow = 0.0f;
    w->flutter = 0.0f;
    w->fade = 0.0f;
    w->fade_v = 0.0f;
    w->filled = 0;
    w->delay = w->centre;
}

/* Excursion amplitude in samples of a sine at `hz` that deviates by `cents`. */
static inline float sid_fx_wow_depth(float cents, float hz, uint32_t sample_rate) {
    /* 2^(c/1200) - 1 to second order; under 0.1 % off below 50 cents. */
    float x = cents * (0.69314718f / 1200.0f);
    float ratio = x * (1.0f + 0.5f * x);
    return ratio * (float)sample_rate / (2.0f * SIDOU_PI * hz);
}

static inline void sid_fx_wow_init(sid_fx_wow* w, uint32_t sample_rate, uint32_t seed) {
    sid_fx_delay_init(&w->line, SID_FX_WOW_LEN);
    for (uint32_t i = 0; i < SID_FX_WOW_LEN; ++i) {
        w->buf[i] = 0.0f;
    }
    float fs = (float)sample_rate;
    w->rng = sid_fx_rng_seed(seed);
    w->wow_inc = sid_fx_phase_inc(SID_FX_WOW_HZ, sample_rate);
    w->flutter_inc = sid_fx_phase_inc(SID_FX_FLUTTER_HZ, sample_rate);
    w->drift_period = sample_rate / 4u;
    w->drift_coef = 2.0f * SIDOU_PI * SID_FX_WOW_DRIFT_HZ / fs;
    w->wow_depth = sid_fx_wow_depth(SID_FX_WOW_CENTS, SID_FX_WOW_HZ, sample_rate);
    w->drift_depth = SID_FX_WOW_DRIFT_MS * 0.001f * fs;
    w->flutter_depth = sid_fx_wow_depth(SID_FX_FLUTTER_CENTS, SID_FX_FLUTTER_HZ, sample_rate);
    /* The excursion swings both ways around the centre; high sample rates shrink the
     * depths so centre + excursion fits the line. */
    float swing = w->wow_depth + w->drift_depth + w->flutter_depth;
    float room = 0.5f * (float)(SID_FX_WOW_LEN - 4u);
    if (swing > room) {
        float scale = room / swing;
        w->wow_depth *= scale;
        w->drift_depth *= scale;
        w->flutter_depth *= scale;
        swing = room;
    }
    w->centre_samples = (uint32_t)(int32_t)swing + 2u;
    w->centre = (float)(int32_t)w->centre_samples;
    w->wow_slew = 1.0f / (SID_FX_WOW_DEPTH_S * fs);
    w->flutter_slew = 1000.0f / (SID_FX_FLUTTER_DEPTH_MS * fs);
    w->fade_step = 1000.0f / (SID_FX_WOW_FADE_MS * fs);
    w->fade_accel = w->fade_step / SID_FX_WOW_FADE_TURN;
    sid_fx_wow_reset(w);
}

/* True while the block still changes the signal with both depths at 0. */
static inline int sid_fx_wow_busy(const sid_fx_wow* w) {
    return w->fade != 0.0f;
}

static inline float sid_fx_wow_slew(float cur, float target, float step) {
    float d = target - cur;
    if (d > step) {
        return cur + step;
    }
    if (d < -step) {
        return cur - step;
    }
    return target;
}

/* x is normalised to [-1, 1); wow and flutter are depths in [0, 1]. */
static inline float sid_fx_wow_tick(sid_fx_wow* w, float x, float wow, float flutter) {
    sid_fx_delay_push(&w->line, w->buf, x);
    if (w->filled < w->centre_samples) {
        ++w->filled;
    }
    w->wow = sid_fx_wow_slew(w->wow, wow, w->wow_slew);
    w->flutter = sid_fx_wow_slew(w->flutter, flutter, w->flutter_slew);

    float slow = sid_fx_sin(w->wow_phase);
    float fast = sid_fx_sin(w->flutter_phase);
    w->wow_phase += w->wow_inc;
    w->flutter_phase += w->flutter_inc;
    if (--w->drift_count == 0) {
        w->drift_count = w->drift_period;
        w->drift_target = sid_fx_rng_bipolar(&w->rng);
    }
    w->drift1 += w->drift_coef * (w->drift_target - w->drift1);
    w->drift2 += w->drift_coef * (w->drift1 - w->drift2);

    /* Fade in once the line holds a centre's worth of input; out once both are off. */
    int on = wow > 0.0f || flutter > 0.0f;
    if (!(on && w->fade == 1.0f)) {
        float v = on && w->filled >= w->centre_samples ? w->fade_step : -w->fade_step;
        w->fade_v = sid_fx_wow_slew(w->fade_v, v, w->fade_accel);
        w->fade += w->fade_v;
        if (w->fade >= 1.0f) {
            w->fade = 1.0f;
            w->fade_v = 0.0f;
        } else if (w->fade <= 0.0f) {
            w->fade = 0.0f;
            w->fade_v = 0.0f;
        }
    }

    w->delay = w->centre + w->wow * (w->wow_depth * slow + w->drift_depth * w->drift2) +
               w->flutter * (w->flutter_depth * fast);
    /* Linear: a Hermite read costs ~34 instructions per sample more (measured). */
    float wet = sid_fx_delay_read(&w->line, w->buf, w->delay);
    if (w->fade == 1.0f) {
        return wet;
    }
    return x + sid_fx_smoothstep(w->fade) * (wet - x);
}

#ifdef __cplusplus
}
#endif

#endif
