// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_slots.h"

#include <stddef.h>

#include "fx_limit.h"

#define SNAP_MASK 31u

#ifdef __PI32V2__
__attribute__((section(".data.sid_slots_algorithms")))
#endif
static const uint32_t ALGORITHMS[SID_SLOT_COUNT] = {SID_DIST_COUNT, SID_MOD_COUNT,
                                                    SID_FXSLOT_COUNT};

_Static_assert(SID_DIST_COUNT <= SID_SLOT_ALGS && SID_MOD_COUNT <= SID_SLOT_ALGS &&
                   SID_FXSLOT_COUNT <= SID_SLOT_ALGS,
               "every algorithm has a start");
_Static_assert(sizeof(sid_slots) <= 0xffffu, "a start's offsets fit 16 bits");
#define LINE_FIRST(type, len)                                                                \
    (offsetof(type, buf) == 0 && offsetof(type, line) == (len) * sizeof(float))
_Static_assert(LINE_FIRST(sid_fx_chorus, SID_FX_CHORUS_LEN) &&
                   LINE_FIRST(sid_fx_flanger, SID_FX_FLANGER_LEN) &&
                   LINE_FIRST(sid_fx_wow, SID_FX_WOW_LEN),
               "the delay lines' samples come just before their line");
#undef LINE_FIRST

#define START(state, tpl, clear)                                                              \
    {offsetof(sid_slots, state), offsetof(sid_slots, tpl), sizeof(((sid_slots*)0)->tpl), clear}
#ifdef __PI32V2__
__attribute__((section(".data.sid_slot_starts")))
#endif
static const sid_slot_start STARTS[SID_SLOT_COUNT][SID_SLOT_ALGS] = {
    [SID_SLOT_DIST] =
        {
            [SID_DIST_TAPE] = START(dist.tape, tpl.tape, 0),
            [SID_DIST_OVERDRIVE] = START(dist.overdrive, tpl.overdrive, 0),
            [SID_DIST_FUZZ] = START(dist.fuzz, tpl.fuzz, 0),
            [SID_DIST_CRUSH] = START(dist.crush, tpl.crush, 0),
            [SID_DIST_LOFI] = START(dist.lofi, tpl.lofi, 0),
        },
    [SID_SLOT_MOD] =
        {
            [SID_MOD_CHORUS] = START(mod.chorus.line, tpl.chorus, SID_FX_CHORUS_LEN),
            [SID_MOD_FLANGER] = START(mod.flanger.line, tpl.flanger, SID_FX_FLANGER_LEN),
            [SID_MOD_PHASER] = START(mod.phaser, tpl.phaser, 0),
            [SID_MOD_TREMOLO] = START(mod.tremolo, tpl.tremolo, 0),
            [SID_MOD_AUTOPAN] = START(mod.tremolo, tpl.tremolo, 0),
            [SID_MOD_WOW] = START(mod.wow.line, tpl.wow, SID_FX_WOW_LEN),
        },
    [SID_SLOT_FX] =
        {
            [SID_FXSLOT_DELAY] = START(fx.echo, tpl.delay, 0),
            [SID_FXSLOT_PINGPONG] = START(fx.echo, tpl.pingpong, 0),
            [SID_FXSLOT_REVERB_LIGHT] = START(fx.reverb, tpl.reverb, 0),
            [SID_FXSLOT_REVERB_LUSH] = START(fx.plate, tpl.plate, 0),
        },
};
#undef START

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

static int resting(const sid_slot* sl) {
    return sl->mix == 0.0f && sl->target.mix == 0.0f && sl->phase == SID_SLOT_RUN &&
           sl->algorithm == sl->target.algorithm;
}

static void slot_copy(void* dst, const void* src, uint32_t n) {
    unsigned char* d = dst;
    const unsigned char* q = src;
    for (uint32_t i = 0; i < n; ++i) {
        d[i] = q[i];
    }
}

static void slot_clear(float* buf, uint32_t n) {
    for (uint32_t i = 0; i < n; ++i) {
        buf[i] = 0.0f;
    }
}

static float dist_tick(sid_slots* s, uint32_t alg, float x, float amount, float a, float b) {
    sid_slot_dist_state* d = &s->dist;
    switch (alg) {
    case SID_DIST_TAPE: return sid_fx_tape_tick(&d->tape, x, amount);
    case SID_DIST_OVERDRIVE: return sid_fx_overdrive_tick(&d->overdrive, x, amount, a, b);
    case SID_DIST_FUZZ: return sid_fx_fuzz_tick(&d->fuzz, x, amount, a, b);
    case SID_DIST_CRUSH: return sid_fx_crush_tick(&d->crush, x, a, amount);
    case SID_DIST_LOFI: {
        float y = sid_fx_crt_tick(&d->lofi.crt, x, amount, a);
        return sid_fx_noise_tick(&d->lofi.noise, y, b);
    }
    default: return x;
    }
}

static void mod_tick(sid_slots* s, uint32_t alg, float x, float amount, float a, float b,
                     float* l, float* r) {
    sid_slot_mod_state* m = &s->mod;
    switch (alg) {
    case SID_MOD_CHORUS:
        sid_fx_chorus_tick(&m->chorus, x, amount, a, b, l, r);
        return;
    case SID_MOD_FLANGER:
        sid_fx_flanger_tick(&m->flanger, x, amount, a, b, l, r);
        return;
    case SID_MOD_PHASER:
        sid_fx_phaser_tick(&m->phaser, x, amount, a, b, l, r);
        return;
    case SID_MOD_TREMOLO:
        sid_fx_tremolo_tick(&m->tremolo, x, amount, a, b, l, r);
        return;
    case SID_MOD_AUTOPAN:
        sid_fx_autopan_tick(&m->tremolo, x, amount, a, b, l, r);
        return;
    case SID_MOD_WOW: x = sid_fx_wow_tick(&m->wow, x, amount, a); break;
    default: break;
    }
    *l = x;
    *r = x;
}

static int fx_uses_pool(uint32_t alg) {
    return alg != SID_FXSLOT_NONE;
}

static void fx_tick(sid_slots* s, uint32_t alg, float in, float amount, float a, float b,
                    float* l, float* r) {
    sid_slot_fx_state* f = &s->fx;
    float m = 0.5f * in * (*l + *r);
    switch (alg) {
    case SID_FXSLOT_DELAY:
    case SID_FXSLOT_PINGPONG:
        sid_fx_echo_tick(&f->echo, s->pool->s16, m, amount, a, b, l, r);
        break;
    case SID_FXSLOT_REVERB_LIGHT:
        sid_fx_reverb_tick(&f->reverb, s->pool->s16, m, amount, a, b, l, r);
        break;
    case SID_FXSLOT_REVERB_LUSH:
        sid_fx_plate_tick(&f->plate, s->pool->f32, m, amount, a, b, l, r);
        break;
    default: break;
    }
}

__attribute__((noinline)) static void slot_start(sid_slots* s, uint32_t k, uint32_t alg) {
    alg = alg < SID_SLOT_ALGS ? alg : 0u;
    const sid_slot_start* st = &STARTS[k][alg];
    unsigned char* base = (unsigned char*)s;
    s->slot[k].warm = s->tpl.warm[k][alg];
    slot_clear((float*)(void*)(base + st->dst) - st->clear, st->clear);
    slot_copy(base + st->dst, base + st->src, st->n);
}

static void enter(sid_slots* s, uint32_t k) {
    sid_slot* sl = &s->slot[k];
    sl->algorithm = sl->target.algorithm;
    sl->fade = 0.0f;
    if (k == SID_SLOT_FX && fx_uses_pool(sl->algorithm)) {
        sl->phase = SID_SLOT_CLEAR;
        sl->clear_pos = 0;
    } else {
        slot_start(s, k, sl->algorithm);
        sl->phase = SID_SLOT_FADE_IN;
    }
}

static float step_switch(sid_slots* s, uint32_t k) {
    sid_slot* sl = &s->slot[k];
    switch (sl->phase) {
    case SID_SLOT_RUN:
        if (sl->algorithm == sl->target.algorithm || !sl->may_switch) {
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
            s->pool->s16[i] = 0;
        }
        sl->clear_pos = end;
        if (end == SID_SLOT_POOL_LEN) {
            slot_start(s, k, sl->algorithm);
            sl->phase = SID_SLOT_FADE_IN;
        }
        return 0.0f;
    }
    default:
        if (sl->warm > 0) {
            --sl->warm;
            return 0.0f;
        }
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

#define CHUNK 16u

static void plan_controls(sid_slots* s, sid_slot* sl) {
    float* cur[4] = {&sl->amount, &sl->a, &sl->b, &sl->mix};
    const float tgt[4] = {sl->target.amount, sl->target.a, sl->target.b, sl->target.mix};
    for (int i = 0; i < 4; ++i) {
        if ((s->clock & SNAP_MASK) == 0) {
            sid_fx_smooth_snap(cur[i], tgt[i]);
        }
        float end = tgt[i] + (*cur[i] - tgt[i]) * s->smooth_ahead;
        sl->inc[i] = (end - *cur[i]) * (1.0f / (float)CHUNK);
    }
}

static int steady(const sid_slot* sl) {
    return sl->phase == SID_SLOT_RUN && (sl->algorithm == sl->target.algorithm || !sl->may_switch);
}

static void plan_turns(sid_slots* s, uint32_t live) {
    int busy = 0;
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        if ((live >> k & 1u) && s->slot[k].phase != SID_SLOT_RUN) {
            busy = 1;
        }
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot* sl = &s->slot[k];
        int pending = (live >> k & 1u) && sl->phase == SID_SLOT_RUN &&
                      sl->algorithm != sl->target.algorithm;
        sl->may_switch = pending && !busy;
        if (pending && !busy) {
            busy = 1;
        }
    }
}

__attribute__((noinline)) static float dist_tick_any(sid_slots* s, uint32_t alg, float x,
                                                     float amount, float a, float b) {
    return dist_tick(s, alg, x, amount, a, b);
}

__attribute__((noinline)) static void mod_tick_any(sid_slots* s, uint32_t alg, float x,
                                                   float amount, float a, float b, float* l,
                                                   float* r) {
    mod_tick(s, alg, x, amount, a, b, l, r);
}

__attribute__((noinline)) static void fx_tick_any(sid_slots* s, uint32_t alg, float in,
                                                  float amount, float a, float b, float* l,
                                                  float* r) {
    fx_tick(s, alg, in, amount, a, b, l, r);
}

#define LOAD(sl)                                                                      \
    float amount = (sl)->amount, a = (sl)->a, b = (sl)->b, mix = (sl)->mix;           \
    const float ia = (sl)->inc[0], ib = (sl)->inc[1], ic = (sl)->inc[2], im = (sl)->inc[3]
#define ADVANCE() (amount += ia, a += ib, b += ic, mix += im)
#define SAVE(sl) ((sl)->amount = amount, (sl)->a = a, (sl)->b = b, (sl)->mix = mix)

#define DIST_LOOP(expr)                                              \
    for (uint32_t i = 0; i < len; ++i) {                             \
        ADVANCE();                                                   \
        float xi = x[i];                                             \
        float xg = xi * SID_SLOT_DIST_GAIN;                          \
        float wet = (expr) * (1.0f / SID_SLOT_DIST_GAIN);            \
        x[i] = xi + mix * (wet - xi);                                \
    }

static void dist_run(sid_slots* s, float* x, uint32_t len) {
    sid_slot* sl = &s->slot[SID_SLOT_DIST];
    sid_slot_dist_state* d = &s->dist;
    LOAD(sl);
    if (!steady(sl)) {
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
            float g = step_switch(s, SID_SLOT_DIST);
            if (sl->phase != SID_SLOT_CLEAR) {
                float wet = dist_tick_any(s, sl->algorithm, x[i] * SID_SLOT_DIST_GAIN, amount,
                                          a, b) *
                            (1.0f / SID_SLOT_DIST_GAIN);
                x[i] += mix * g * (wet - x[i]);
            }
        }
        SAVE(sl);
        return;
    }
    switch (sl->algorithm) {
    case SID_DIST_TAPE: DIST_LOOP(sid_fx_tape_tick(&d->tape, xg, amount)) break;
    case SID_DIST_OVERDRIVE:
        DIST_LOOP(sid_fx_overdrive_tick(&d->overdrive, xg, amount, a, b))
        break;
    case SID_DIST_FUZZ: DIST_LOOP(sid_fx_fuzz_tick(&d->fuzz, xg, amount, a, b)) break;
    case SID_DIST_CRUSH: DIST_LOOP(sid_fx_crush_tick(&d->crush, xg, a, amount)) break;
    case SID_DIST_LOFI:
        DIST_LOOP(sid_fx_noise_tick(&d->lofi.noise, sid_fx_crt_tick(&d->lofi.crt, xg, amount, a),
                                    b))
        break;
    default:
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
        }
        break;
    }
    SAVE(sl);
}

#define MOD_LOOP(call)                                  \
    for (uint32_t i = 0; i < len; ++i) {                \
        ADVANCE();                                      \
        float xi = x[i], wl, wr;                        \
        call;                                           \
        l[i] = xi + mix * (wl - xi);                    \
        r[i] = xi + mix * (wr - xi);                    \
    }

static void mod_run(sid_slots* s, const float* x, float* l, float* r, uint32_t len) {
    sid_slot* sl = &s->slot[SID_SLOT_MOD];
    sid_slot_mod_state* m = &s->mod;
    LOAD(sl);
    if (!steady(sl)) {
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
            float g = step_switch(s, SID_SLOT_MOD);
            float wl = x[i];
            float wr = x[i];
            mod_tick_any(s, sl->algorithm, x[i], amount, a, b, &wl, &wr);
            float k = mix * g;
            l[i] = x[i] + k * (wl - x[i]);
            r[i] = x[i] + k * (wr - x[i]);
        }
        SAVE(sl);
        return;
    }
    switch (sl->algorithm) {
    case SID_MOD_CHORUS:
        MOD_LOOP(sid_fx_chorus_tick(&m->chorus, xi, amount, a, b, &wl, &wr))
        break;
    case SID_MOD_FLANGER:
        MOD_LOOP(sid_fx_flanger_tick(&m->flanger, xi, amount, a, b, &wl, &wr))
        break;
    case SID_MOD_PHASER:
        MOD_LOOP(sid_fx_phaser_tick(&m->phaser, xi, amount, a, b, &wl, &wr))
        break;
    case SID_MOD_TREMOLO:
        MOD_LOOP(sid_fx_tremolo_tick(&m->tremolo, xi, amount, a, b, &wl, &wr))
        break;
    case SID_MOD_AUTOPAN:
        MOD_LOOP(sid_fx_autopan_tick(&m->tremolo, xi, amount, a, b, &wl, &wr))
        break;
    case SID_MOD_WOW: MOD_LOOP(wl = wr = sid_fx_wow_tick(&m->wow, xi, amount, a)) break;
    default:
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
            l[i] = x[i];
            r[i] = x[i];
        }
        break;
    }
    SAVE(sl);
}

#define FX_LOOP(call)                                   \
    for (uint32_t i = 0; i < len; ++i) {                \
        ADVANCE();                                      \
        float li = l[i], ri = r[i], wl, wr;             \
        float mono = 0.5f * (li + ri);                  \
        call;                                           \
        l[i] = li + mix * (wl - li);                    \
        r[i] = ri + mix * (wr - ri);                    \
    }

static void fx_run(sid_slots* s, float* l, float* r, uint32_t len) {
    sid_slot* sl = &s->slot[SID_SLOT_FX];
    sid_slot_fx_state* f = &s->fx;
    int16_t* pool = s->pool->s16;
    float* fpool = s->pool->f32;
    LOAD(sl);
    if (!steady(sl)) {
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
            float g = step_switch(s, SID_SLOT_FX);
            if (sl->phase != SID_SLOT_CLEAR) {
                float wl = l[i];
                float wr = r[i];
                fx_tick_any(s, sl->algorithm, sl->phase == SID_SLOT_FADE_IN ? g : 1.0f, amount,
                            a, b, &wl, &wr);
                float k = mix * g;
                l[i] += k * (wl - l[i]);
                r[i] += k * (wr - r[i]);
            }
        }
        SAVE(sl);
        return;
    }
    switch (sl->algorithm) {
    case SID_FXSLOT_DELAY:
    case SID_FXSLOT_PINGPONG:
        FX_LOOP(sid_fx_echo_tick(&f->echo, pool, mono, amount, a, b, &wl, &wr))
        break;
    case SID_FXSLOT_REVERB_LIGHT:
        FX_LOOP(sid_fx_reverb_tick(&f->reverb, pool, mono, amount, a, b, &wl, &wr))
        break;
    case SID_FXSLOT_REVERB_LUSH:
        FX_LOOP(sid_fx_plate_tick(&f->plate, fpool, mono, amount, a, b, &wl, &wr))
        break;
    default:
        for (uint32_t i = 0; i < len; ++i) {
            ADVANCE();
        }
        break;
    }
    SAVE(sl);
}

__attribute__((noinline, unused)) static void slots_bypass(const int16_t* in, const int16_t* over,
                                                           int16_t* out, float* fout, uint32_t n) {
    if (fout) {
        for (uint32_t i = 0; i < n; ++i) {
            int32_t v = (int32_t)in[i] + (over ? over[i] : 0);
            fout[2 * i] = fout[2 * i + 1] = (float)v * (1.0f / 32768.0f);
        }
        return;
    }
    if (over) {
        for (uint32_t i = 0; i < n; ++i) {
            out[2 * i] = out[2 * i + 1] = sid_fx_limit_i32((int32_t)in[i] + over[i]);
        }
        return;
    }
    for (uint32_t i = 0; i < n; ++i) {
        out[2 * i] = in[i];
        out[2 * i + 1] = in[i];
    }
}

__attribute__((noinline, unused)) static void slots_run(sid_slots* s, const int16_t* in,
                                                        const int16_t* over, int16_t* out,
                                                        float* fout, uint32_t n) {
    uint32_t live = 0;
    uint32_t awake = 0;
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot* sl = &s->slot[k];
        if (resting(sl) && (!sl->awake || (s->clock & SNAP_MASK) == 0)) {
            sl->awake = 0;
        } else {
            live |= 1u << k;
        }
        awake |= sl->awake << k;
    }
    if (!live) {
        slots_bypass(in, over, out, fout, n);
        return;
    }
    if (!awake) {
        s->clock = 0;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        if ((live >> k & 1u) && !s->slot[k].awake) {
            sid_slot* sl = &s->slot[k];
            sl->algorithm = SID_SLOT_WAKING;
            sl->phase = SID_SLOT_RUN;
            sl->fade = 0.0f;
            sl->may_switch = 0;
            for (int i = 0; i < 4; ++i) {
                sl->inc[i] = 0.0f;
            }
        }
    }

    float x[CHUNK], l[CHUNK], r[CHUNK];
    uint32_t done = 0;
    while (done < n) {
        uint32_t pos = s->clock & (CHUNK - 1u);
        if (pos == 0) {
            plan_turns(s, live);
            for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
                if (live >> k & 1u) {
                    plan_controls(s, &s->slot[k]);
                    if ((s->clock & SNAP_MASK) == 0 && resting(&s->slot[k])) {
                        live &= ~(1u << k);
                    }
                }
            }
        }
        uint32_t len = CHUNK - pos;
        if (len > n - done) {
            len = n - done;
        }
        if (over) {
            for (uint32_t i = 0; i < len; ++i) {
                x[i] = (float)((int32_t)in[done + i] + over[done + i]) * (1.0f / 32768.0f);
            }
        } else {
            for (uint32_t i = 0; i < len; ++i) {
                x[i] = (float)in[done + i] * (1.0f / 32768.0f);
            }
        }
        if (live & 1u) {
            dist_run(s, x, len);
        }
        if (live & 2u) {
            mod_run(s, x, l, r, len);
        } else {
            for (uint32_t i = 0; i < len; ++i) {
                l[i] = x[i];
                r[i] = x[i];
            }
        }
        if (live & 4u) {
            fx_run(s, l, r, len);
        }
        if (fout) {
            for (uint32_t i = 0; i < len; ++i) {
                fout[2 * (done + i)] = l[i];
                fout[2 * (done + i) + 1] = r[i];
            }
        } else {
            for (uint32_t i = 0; i < len; ++i) {
                out[2 * (done + i)] = sid_fx_to_i16_limited(l[i]);
                out[2 * (done + i) + 1] = sid_fx_to_i16_limited(r[i]);
            }
        }
        done += len;
        s->clock += len;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        s->slot[k].awake = live >> k & 1u;
    }
}

void sid_slots_process(sid_slots* s, const int16_t* in, int16_t* out, uint32_t n) {
    slots_run(s, in, NULL, out, NULL, n);
}

void sid_slots_process_wide(sid_slots* s, const int16_t* in, const int16_t* over, int16_t* out,
                            uint32_t n) {
    slots_run(s, in, over, out, NULL, n);
}

void sid_slots_process_float(sid_slots* s, const int16_t* in, const int16_t* over, float* out,
                             uint32_t n) {
    slots_run(s, in, over, NULL, out, n);
}

__attribute__((noinline, unused)) static float slot_dry(const sid_slot* sl) {
    if (!sl->awake || sl->algorithm == 0u || sl->algorithm == SID_SLOT_WAKING) {
        return 1.0f;
    }
    float g = 1.0f;
    if (sl->phase == SID_SLOT_CLEAR || (sl->phase == SID_SLOT_FADE_IN && sl->warm > 0u)) {
        g = 0.0f;
    } else if (sl->phase != SID_SLOT_RUN) {
        g = sid_fx_smoothstep(sl->fade);
    }
    return 1.0f - sl->mix * g;
}

float sid_slots_dry(const sid_slots* s) {
    _Static_assert(SID_SLOT_COUNT == 3, "three slots");
    return slot_dry(&s->slot[0]) * slot_dry(&s->slot[1]) * slot_dry(&s->slot[2]);
}
