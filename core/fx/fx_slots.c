// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_slots.h"

/* Every 32 samples of the slots' own clock. */
#define SNAP_MASK 31u

static const uint32_t ALGORITHMS[SID_SLOT_COUNT] = {SID_DIST_COUNT, SID_MOD_COUNT,
                                                    SID_FXSLOT_COUNT};

void sid_slots_init(sid_slots* s, uint32_t sample_rate, uint32_t seed, sid_slot_pool* pool) {
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
        sl->warm = 0;
        sl->may_switch = 0;
        sl->fade = 1.0f;
        for (int i = 0; i < 4; ++i) {
            sl->inc[i] = 0.0f;
        }
    }
    s->sample_rate = sample_rate;
    s->seed = seed;
    s->clock = 0;
    float k = sid_fx_smooth_coef(SID_FX_SMOOTH_MS, sample_rate);
    float ahead = 1.0f;
    for (uint32_t i = 0; i < 16u; ++i) {
        ahead *= 1.0f - k; /* (1 - k)^CHUNK: the one-pole CHUNK samples on */
    }
    s->smooth_ahead = ahead;
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

static void mod_start(sid_slots* s, uint32_t alg) {
    sid_slot_mod_state* m = &s->mod;
    switch (alg) {
    case SID_MOD_CHORUS: sid_fx_chorus_init(&m->chorus, s->sample_rate); break;
    case SID_MOD_FLANGER: sid_fx_flanger_init(&m->flanger, s->sample_rate); break;
    case SID_MOD_PHASER: sid_fx_phaser_init(&m->phaser, s->sample_rate); break;
    case SID_MOD_TREMOLO:
    case SID_MOD_AUTOPAN: sid_fx_tremolo_init(&m->tremolo, s->sample_rate); break;
    case SID_MOD_WOW: sid_fx_wow_init(&m->wow, s->sample_rate, s->seed * 0x9e3779b9u + 1u); break;
    default: break;
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

/* Does the algorithm keep state in the pool (which must be cleared before it starts)? */
static int fx_uses_pool(uint32_t alg) {
    return alg != SID_FXSLOT_NONE;
}

static void fx_start(sid_slots* s, uint32_t alg) {
    sid_slot_fx_state* f = &s->fx;
    switch (alg) {
    case SID_FXSLOT_DELAY: sid_fx_echo_init(&f->echo, s->sample_rate, SID_SLOT_POOL_LEN, 0); break;
    case SID_FXSLOT_PINGPONG:
        sid_fx_echo_init(&f->echo, s->sample_rate, SID_SLOT_POOL_LEN, 1);
        break;
    case SID_FXSLOT_REVERB_LIGHT: sid_fx_reverb_init(&f->reverb, s->sample_rate); break;
    case SID_FXSLOT_REVERB_LUSH: sid_fx_plate_init(&f->plate, s->sample_rate); break;
    default: break;
    }
}

/* Wet output from the stereo input in *l, *r (which stays the wet when NONE). `in` scales
 * the wet path's input. */
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

/* Samples an algorithm must run before it is heard: its longest tap delay. */
static uint32_t warm_up(const sid_slots* s, uint32_t k, uint32_t alg) {
    float ms = 0.0f;
    if (k == SID_SLOT_MOD && alg == SID_MOD_CHORUS) {
        ms = SID_FX_CHORUS_CENTRE_MS + SID_FX_CHORUS_SLOW_MS + SID_FX_CHORUS_FAST_MS;
    } else if (k == SID_SLOT_MOD && alg == SID_MOD_FLANGER) {
        ms = SID_FX_FLANGER_MAX_MS;
    }
    return (uint32_t)(int32_t)(ms * 0.001f * (float)s->sample_rate) + 4u;
}

static void start(sid_slots* s, uint32_t k, uint32_t alg) {
    s->slot[k].warm = warm_up(s, k, alg);
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
        if (sl->algorithm == sl->target.algorithm || !sl->may_switch) {
            return 1.0f; /* switches take turns: see plan_turns */
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
            start(s, k, sl->algorithm);
            sl->phase = SID_SLOT_FADE_IN;
        }
        return 0.0f;
    }
    default: /* FADE_IN: completes before any new switch, so the fade never reverses */
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

/* ---- the frame ----
 *
 * Samples go through in chunks of up to CHUNK, cut on the slots' own clock (so the result
 * does not depend on how calls split the stream). At each chunk start every control's
 * one-pole is evaluated CHUNK samples ahead and the chunk follows it as a straight line:
 * one add per control per sample instead of a full one-pole step. */

#define CHUNK 16u

/* At a chunk start: snap (on the 32-sample grid) and set the controls' slopes. */
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

/* Is the slot running its algorithm undisturbed (no switch under way, none it may start
 * in this chunk)? */
static int steady(const sid_slot* sl) {
    return sl->phase == SID_SLOT_RUN && (sl->algorithm == sl->target.algorithm || !sl->may_switch);
}

/* At a chunk start: which slot may start a switch in this chunk. A preset change that
 * switches every slot would otherwise run three switches at once (three old algorithms
 * fading out, each on the per-sample switching path); taking turns bounds the extra cost
 * to one slot's, and a full preset change takes up to three switch times. Only a slot
 * with a switch pending may start one, only while no other is mid-switch, and the first
 * such slot in order goes first. Decided per chunk on the slots' clock, so where calls
 * end cannot change it. */
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

/* The per-sample ticks for the switching path, kept out of line so that the steady
 * path's per-algorithm loops below stay small (fm1-emu does not decode the long-range
 * conditional branches clang emits for one loop holding every algorithm). */
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

/* Loads a slot's controls and slopes into locals; SAVE writes them back. */
#define LOAD(sl)                                                                      \
    float amount = (sl)->amount, a = (sl)->a, b = (sl)->b, mix = (sl)->mix;           \
    const float ia = (sl)->inc[0], ib = (sl)->inc[1], ic = (sl)->inc[2], im = (sl)->inc[3]
#define ADVANCE() (amount += ia, a += ib, b += ic, mix += im)
#define SAVE(sl) ((sl)->amount = amount, (sl)->a = a, (sl)->b = b, (sl)->mix = mix)

/* Steady distortion: one loop per algorithm. */
#define DIST_LOOP(expr)                              \
    for (uint32_t i = 0; i < len; ++i) {             \
        ADVANCE();                                   \
        float xi = x[i];                             \
        float wet = (expr);                          \
        x[i] = xi + mix * (wet - xi);                \
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
                float wet = dist_tick_any(s, sl->algorithm, x[i], amount, a, b);
                x[i] += mix * g * (wet - x[i]);
            }
        }
        SAVE(sl);
        return;
    }
    switch (sl->algorithm) {
    case SID_DIST_TAPE: DIST_LOOP(sid_fx_tape_tick(&d->tape, xi, amount)) break;
    case SID_DIST_OVERDRIVE:
        DIST_LOOP(sid_fx_overdrive_tick(&d->overdrive, xi, amount, a, b))
        break;
    case SID_DIST_FUZZ: DIST_LOOP(sid_fx_fuzz_tick(&d->fuzz, xi, amount, a, b)) break;
    case SID_DIST_CRUSH: DIST_LOOP(sid_fx_crush_tick(&d->crush, xi, a, amount)) break;
    case SID_DIST_LOFI:
        DIST_LOOP(sid_fx_noise_tick(&d->lofi.noise, sid_fx_crt_tick(&d->lofi.crt, xi, amount, a),
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

/* Steady modulation: one loop per algorithm. */
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

/* Steady FX: one loop per algorithm; the wet path takes (L + R) / 2. */
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
                /* A fresh line records from the switch on: fading its input in with the
                 * wet path makes the first echo start smoothly, not mid-waveform. */
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

void sid_slots_process(sid_slots* s, const int16_t* in, int16_t* out, uint32_t n) {
    uint32_t live = 0;
    uint32_t awake = 0;
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        sid_slot* sl = &s->slot[k];
        /* A slot in the path leaves it only on the snap grid (where the chunk loop
         * checks too), so where calls end cannot move that point; one already out stays
         * out. */
        if (resting(sl) && (!sl->awake || (s->clock & SNAP_MASK) == 0)) {
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
        /* From rest: restart the clock, so where the chunks and the snap grid fall does
         * not depend on how earlier calls were split. */
        s->clock = 0;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        if ((live >> k & 1u) && !s->slot[k].awake) {
            enter(s, k); /* back from rest: start clean */
            for (int i = 0; i < 4; ++i) {
                s->slot[k].inc[i] = 0.0f; /* held until the next chunk start plans them */
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
                        live &= ~(1u << k); /* out of the path from here */
                    }
                }
            }
        }
        uint32_t len = CHUNK - pos;
        if (len > n - done) {
            len = n - done;
        }
        for (uint32_t i = 0; i < len; ++i) {
            x[i] = (float)in[done + i] * (1.0f / 32768.0f);
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
        for (uint32_t i = 0; i < len; ++i) {
            out[2 * (done + i)] = sid_fx_to_i16(l[i]);
            out[2 * (done + i) + 1] = sid_fx_to_i16(r[i]);
        }
        done += len;
        s->clock += len;
    }
    for (uint32_t k = 0; k < SID_SLOT_COUNT; ++k) {
        s->slot[k].awake = live >> k & 1u;
    }
}
