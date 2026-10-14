// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx.h"

#define ON_CRUSH 1u
#define ON_TAPE 2u
#define ON_WOW 4u
#define ON_CRT 8u
#define ON_NOISE 16u

void sid_fx_init(sid_fx* fx, uint32_t sample_rate, uint32_t seed) {
    sid_fx_params zero = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f};
    fx->target = zero;
    fx->cur = zero;
    /* CRT's amount is smoothed on every sample; the others, whose blocks respond to it
     * smoothly enough (the click tests check), on every other sample, with the
     * coefficient for two steps: 1 - (1 - k)^2. That halves the smoothing and the
     * coefficient updates while parameters move. */
    float k = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    fx->smooth_coef = k;
    fx->smooth_coef2 = k * (2.0f - k);
    fx->active = 0;
    fx->clock = 0;
    sid_fx_crush_init(&fx->crush, sample_rate);
    sid_fx_tape_init(&fx->tape, sample_rate);
    /* Different streams for the drift and the hiss. */
    sid_fx_wow_init(&fx->wow, sample_rate, seed * 0x9e3779b9u + 1u);
    sid_fx_crt_init(&fx->crt, sample_rate);
    sid_fx_noise_init(&fx->noise, seed);
}

void sid_fx_set(sid_fx* fx, const sid_fx_params* p) {
    fx->target.tape_drive = sid_fx_unit(p->tape_drive);
    fx->target.wow = sid_fx_unit(p->wow);
    fx->target.flutter = sid_fx_unit(p->flutter);
    fx->target.noise = sid_fx_unit(p->noise);
    fx->target.crt = sid_fx_unit(p->crt);
    fx->target.crush = sid_fx_unit(p->crush);
}

static int idle(const sid_fx* fx) {
    const sid_fx_params* t = &fx->target;
    const sid_fx_params* c = &fx->cur;
    return t->tape_drive == 0.0f && t->wow == 0.0f && t->flutter == 0.0f && t->noise == 0.0f &&
           t->crt == 0.0f && t->crush == 0.0f && c->tape_drive == 0.0f && c->wow == 0.0f &&
           c->flutter == 0.0f && c->noise == 0.0f && c->crt == 0.0f && c->crush == 0.0f &&
           !sid_fx_wow_busy(&fx->wow);
}

/* Every 32 samples of the chain's own clock. */
#define SNAP_MASK 31u

static void smooth(sid_fx* fx) {
    const sid_fx_params* t = &fx->target;
    sid_fx_params* c = &fx->cur;
    sid_fx_smooth_step(&c->crt, t->crt, fx->smooth_coef);
    if ((fx->clock & 1u) == 0) {
        float k = fx->smooth_coef2;
        sid_fx_smooth_step(&c->tape_drive, t->tape_drive, k);
        sid_fx_smooth_step(&c->wow, t->wow, k);
        sid_fx_smooth_step(&c->flutter, t->flutter, k);
        sid_fx_smooth_step(&c->noise, t->noise, k);
        sid_fx_smooth_step(&c->crush, t->crush, k);
    }
    if ((fx->clock & SNAP_MASK) == 0) {
        sid_fx_smooth_snap(&c->tape_drive, t->tape_drive);
        sid_fx_smooth_snap(&c->wow, t->wow);
        sid_fx_smooth_snap(&c->flutter, t->flutter);
        sid_fx_smooth_snap(&c->noise, t->noise);
        sid_fx_smooth_snap(&c->crt, t->crt);
        sid_fx_smooth_snap(&c->crush, t->crush);
    }
}

void sid_fx_process(sid_fx* fx, int16_t* io, uint32_t n) {
    if (idle(fx)) {
        return;
    }
    const sid_fx_params* c = &fx->cur;
    const sid_fx_params* t = &fx->target;
    uint32_t active = fx->active;
    if (active == 0) {
        /* Starting from rest (just initialised, or after the bypass): restart the snap
         * grid here, so where it falls does not depend on how earlier calls were split. */
        fx->clock = 0;
    }
    int settled = c->tape_drive == t->tape_drive && c->wow == t->wow &&
                  c->flutter == t->flutter && c->noise == t->noise && c->crt == t->crt &&
                  c->crush == t->crush;
    for (uint32_t i = 0; i < n; ++i) {
        if (!settled) {
            smooth(fx);
        }
        float x = (float)io[i] * (1.0f / 32768.0f);
        /* A block entering the path starts from its rest state. */
        if (c->crush > 0.0f) {
            if (!(active & ON_CRUSH)) {
                sid_fx_crush_reset(&fx->crush);
                active |= ON_CRUSH;
            }
            x = sid_fx_crush_tick(&fx->crush, x, c->crush, c->crush);
        } else {
            active &= ~ON_CRUSH;
        }
        if (c->tape_drive > 0.0f) {
            if (!(active & ON_TAPE)) {
                sid_fx_tape_reset(&fx->tape);
                active |= ON_TAPE;
            }
            x = sid_fx_tape_tick(&fx->tape, x, c->tape_drive);
        } else {
            active &= ~ON_TAPE;
        }
        if (c->wow > 0.0f || c->flutter > 0.0f || sid_fx_wow_busy(&fx->wow)) {
            if (!(active & ON_WOW)) {
                sid_fx_wow_reset(&fx->wow);
                active |= ON_WOW;
            }
            x = sid_fx_wow_tick(&fx->wow, x, c->wow, c->flutter);
            /* It can finish fading out on this very sample, maybe the last before the
             * bypass: leave the path now, so the next start resets it. */
            if (c->wow == 0.0f && c->flutter == 0.0f && !sid_fx_wow_busy(&fx->wow)) {
                active &= ~ON_WOW;
            }
        } else {
            active &= ~ON_WOW;
        }
        if (c->crt > 0.0f) {
            if (!(active & ON_CRT)) {
                sid_fx_crt_reset(&fx->crt, x);
                active |= ON_CRT;
            }
            x = sid_fx_crt_tick(&fx->crt, x, c->crt, c->crt);
        } else {
            active &= ~ON_CRT;
        }
        if (c->noise > 0.0f) {
            if (!(active & ON_NOISE)) {
                sid_fx_noise_reset(&fx->noise);
                active |= ON_NOISE;
            }
            x = sid_fx_noise_tick(&fx->noise, x, c->noise);
        } else {
            active &= ~ON_NOISE;
        }
        io[i] = sid_fx_to_i16(x);
        ++fx->clock;
    }
    fx->active = active;
}
