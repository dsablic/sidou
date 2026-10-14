// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_slots.h"

/* Every 32 samples of the slots' own clock. */
#define SNAP_MASK 31u

static const uint32_t ALGORITHMS[SID_SLOT_COUNT] = {SID_DIST_COUNT, SID_MOD_COUNT,
                                                    SID_FXSLOT_COUNT};

void sid_slots_init(sid_slots* s, uint32_t sample_rate, uint32_t seed, int16_t* pool) {
    sid_slot_params off = {0, 0.0f, 0.0f, 0.0f, 0.0f};
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot* sl = &s->slot[k];
        sl->target = off;
        sl->amount = 0.0f;
        sl->a = 0.0f;
        sl->b = 0.0f;
        sl->mix = 0.0f;
        sl->algorithm = 0;
        sl->phase = SID_SLOT_RUN;
        sl->awake = 0;
        sl->clear_pos = 0;
        sl->fade = 1.0f;
    }
    s->sample_rate = sample_rate;
    s->seed = seed;
    s->clock = 0;
    s->smooth_coef = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    float fade_samples = SID_SLOT_FADE_MS * 0.001f * (float)sample_rate;
    s->fade_step = 1.0f / fade_samples;
    s->clear_per_sample = SID_SLOT_POOL_LEN / (uint32_t)(int32_t)fade_samples + 1u;
    s->pool = pool;
}

void sid_slots_set(sid_slots* s, uint32_t slot, const sid_slot_params* p) {
    if (slot >= SID_SLOT_COUNT) {
        return;
    }
    sid_slot* sl = &s->slot[slot];
    sl->target.algorithm = p->algorithm < ALGORITHMS[slot] ? p->algorithm : 0;
    sl->target.amount = sid_fx_unit(p->amount);
    sl->target.a = sid_fx_unit(p->a);
    sl->target.b = sid_fx_unit(p->b);
    sl->target.mix = sid_fx_unit(p->mix);
}

/* Out of the path: Mix 0 and settled, no switch under way or pending. */
static int resting(const sid_slot* sl) {
    return sl->mix == 0.0f && sl->target.mix == 0.0f && sl->phase == SID_SLOT_RUN &&
           sl->algorithm == sl->target.algorithm;
}

/* ---- per-slot algorithms ---- */

static void dist_start(sid_slots* s, uint32_t alg) {
    sid_slot_dist_state* d = &s->dist;
    switch (alg) {
    case SID_DIST_TAPE: sid_fx_tape_init(&d->tape, s->sample_rate); break;
    case SID_DIST_OVERDRIVE: sid_fx_overdrive_init(&d->overdrive, s->sample_rate); break;
    case SID_DIST_FUZZ: sid_fx_fuzz_init(&d->fuzz, s->sample_rate); break;
    case SID_DIST_CRUSH: sid_fx_crush_init(&d->crush, s->sample_rate); break;
    case SID_DIST_LOFI:
        sid_fx_crt_init(&d->lofi.crt, s->sample_rate);
        sid_fx_noise_init(&d->lofi.noise, s->seed);
        break;
    default: break;
    }
}

static float dist_tick(sid_slots* s, const sid_slot* sl, float x) {
    sid_slot_dist_state* d = &s->dist;
    switch (sl->algorithm) {
    case SID_DIST_TAPE: return sid_fx_tape_tick(&d->tape, x, sl->amount);
    case SID_DIST_OVERDRIVE:
        return sid_fx_overdrive_tick(&d->overdrive, x, sl->amount, sl->a, sl->b);
    case SID_DIST_FUZZ: return sid_fx_fuzz_tick(&d->fuzz, x, sl->amount, sl->a, sl->b);
    case SID_DIST_CRUSH: return sid_fx_crush_tick(&d->crush, x, sl->a, sl->amount);
    case SID_DIST_LOFI: {
        float y = sid_fx_crt_tick(&d->lofi.crt, x, sl->amount, sl->a);
        return sid_fx_noise_tick(&d->lofi.noise, y, sl->b);
    }
    default: return x;
    }
}

static void mod_start(sid_slots* s, uint32_t alg) {
    sid_slot_mod_state* m = &s->mod;
    switch (alg) {
    case SID_MOD_WOW: sid_fx_wow_init(&m->wow, s->sample_rate, s->seed * 0x9e3779b9u + 1u); break;
    default: break;
    }
}

static void mod_tick(sid_slots* s, const sid_slot* sl, float x, float* l, float* r) {
    sid_slot_mod_state* m = &s->mod;
    switch (sl->algorithm) {
    case SID_MOD_WOW: x = sid_fx_wow_tick(&m->wow, x, sl->amount, sl->a); break;
    default: break;
    }
    *l = x;
    *r = x;
}

/* Does the algorithm keep state in the pool (which must be cleared before it starts)? */
static int fx_uses_pool(uint32_t alg) {
    return alg != SID_FXSLOT_NONE;
}

static void fx_start(sid_slots* s, uint32_t alg) {
    (void)s;
    (void)alg;
}

/* Wet output from the stereo input. */
static void fx_tick(sid_slots* s, const sid_slot* sl, float* l, float* r) {
    (void)s;
    (void)sl;
    (void)l;
    (void)r;
}

static void start(sid_slots* s, uint32_t k, uint32_t alg) {
    switch (k) {
    case SID_SLOT_DIST: dist_start(s, alg); break;
    case SID_SLOT_MOD: mod_start(s, alg); break;
    default: fx_start(s, alg); break;
    }
}

/* ---- switching ---- */

/* Enter the new algorithm: clear the pool first if it needs it, else start fading in. */
static void enter(sid_slots* s, uint32_t k) {
    sid_slot* sl = &s->slot[k];
    sl->algorithm = sl->target.algorithm;
    sl->fade = 0.0f;
    if (k == SID_SLOT_FX && fx_uses_pool(sl->algorithm)) {
        sl->phase = SID_SLOT_CLEAR;
        sl->clear_pos = 0;
    } else {
        start(s, k, sl->algorithm);
        sl->phase = SID_SLOT_FADE_IN;
    }
}

/* One sample of the switch state machine; returns the wet path's gain. */
static float step_switch(sid_slots* s, uint32_t k) {
    sid_slot* sl = &s->slot[k];
    switch (sl->phase) {
    case SID_SLOT_RUN:
        if (sl->algorithm == sl->target.algorithm) {
            return 1.0f;
        }
        sl->phase = SID_SLOT_FADE_OUT;
        /* fall through */
    case SID_SLOT_FADE_OUT:
        sl->fade -= s->fade_step;
        if (sl->fade <= 0.0f) {
            enter(s, k);
            return 0.0f;
        }
        break;
    case SID_SLOT_CLEAR: {
        uint32_t end = sl->clear_pos + s->clear_per_sample;
        if (end > SID_SLOT_POOL_LEN) {
            end = SID_SLOT_POOL_LEN;
        }
        for (uint32_t i = sl->clear_pos; i < end; ++i) {
            s->pool[i] = 0;
        }
        sl->clear_pos = end;
        if (end == SID_SLOT_POOL_LEN) {
            start(s, k, sl->algorithm);
            sl->phase = SID_SLOT_FADE_IN;
        }
        return 0.0f;
    }
    default: /* FADE_IN: completes before any new switch, so the fade never reverses */
        sl->fade += s->fade_step;
        if (sl->fade >= 1.0f) {
            sl->fade = 1.0f;
            sl->phase = SID_SLOT_RUN;
            return 1.0f;
        }
        break;
    }
    return sid_fx_smoothstep(sl->fade);
}

/* ---- the frame ---- */

static void smooth_controls(sid_slot* sl, float k) {
    sid_fx_smooth_step(&sl->amount, sl->target.amount, k);
    sid_fx_smooth_step(&sl->a, sl->target.a, k);
    sid_fx_smooth_step(&sl->b, sl->target.b, k);
    sid_fx_smooth_step(&sl->mix, sl->target.mix, k);
}

static void snap(sid_slot* sl) {
    sid_fx_smooth_snap(&sl->amount, sl->target.amount);
    sid_fx_smooth_snap(&sl->a, sl->target.a);
    sid_fx_smooth_snap(&sl->b, sl->target.b);
    sid_fx_smooth_snap(&sl->mix, sl->target.mix);
}

void sid_slots_process(sid_slots* s, const int16_t* in, int16_t* out, uint32_t n) {
    uint32_t live = 0;
    uint32_t awake = 0;
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot* sl = &s->slot[k];
        if (resting(sl)) {
            /* At rest from here on, however the last call ended: waking up restarts it. */
            sl->awake = 0;
        } else {
            live |= 1u << k;
        }
        awake |= sl->awake << k;
    }
    if (!live) {
        for (uint32_t i = 0; i < n; ++i) {
            out[2 * i] = in[i];
            out[2 * i + 1] = in[i];
        }
        return;
    }
    if (!awake) {
        /* From rest: restart the snap grid, so where it falls does not depend on how
         * earlier calls were split. */
        s->clock = 0;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        if ((live >> k & 1u) && !s->slot[k].awake) {
            enter(s, k); /* back from rest: start clean */
        }
    }

    sid_slot* dist = &s->slot[SID_SLOT_DIST];
    sid_slot* mod = &s->slot[SID_SLOT_MOD];
    sid_slot* fx = &s->slot[SID_SLOT_FX];
    for (uint32_t i = 0; i < n; ++i) {
        float x = (float)in[i] * (1.0f / 32768.0f);
        float l = x;
        float r = x;

        if (live & 1u) {
            smooth_controls(dist, s->smooth_coef);
            float g = step_switch(s, SID_SLOT_DIST);
            float wet = g > 0.0f ? dist_tick(s, dist, x) : x;
            x += dist->mix * g * (wet - x);
            l = x;
            r = x;
        }
        if (live & 2u) {
            smooth_controls(mod, s->smooth_coef);
            float g = step_switch(s, SID_SLOT_MOD);
            float wl = x;
            float wr = x;
            if (g > 0.0f) {
                mod_tick(s, mod, x, &wl, &wr);
            }
            float m = mod->mix * g;
            l = x + m * (wl - x);
            r = x + m * (wr - x);
        }
        if (live & 4u) {
            smooth_controls(fx, s->smooth_coef);
            float g = step_switch(s, SID_SLOT_FX);
            float wl = l;
            float wr = r;
            if (g > 0.0f) {
                fx_tick(s, fx, &wl, &wr);
            }
            float m = fx->mix * g;
            l += m * (wl - l);
            r += m * (wr - r);
        }
        out[2 * i] = sid_fx_to_i16(l);
        out[2 * i + 1] = sid_fx_to_i16(r);

        if ((s->clock & SNAP_MASK) == 0) {
            for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
                if (live >> k & 1u) {
                    snap(&s->slot[k]);
                    if (resting(&s->slot[k])) {
                        live &= ~(1u << k); /* out of the path from the next sample */
                    }
                }
            }
        }
        ++s->clock;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        s->slot[k].awake = live >> k & 1u;
    }
}
