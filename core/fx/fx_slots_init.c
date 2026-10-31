// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_slots.h"

#include <stddef.h>

static void tpl_keep(void* dst, const void* src, uint32_t n) {
    unsigned char* d = dst;
    const unsigned char* q = src;
    for (uint32_t i = 0; i < n; ++i) {
        d[i] = q[i];
    }
}

static uint32_t warm_up(uint32_t sample_rate, float ms) {
    return (uint32_t)(int32_t)(ms * 0.001f * (float)sample_rate) + 4u;
}

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
        ahead *= 1.0f - k;
    }
    s->smooth_ahead = ahead;
    float fade_samples = SID_SLOT_FADE_MS * 0.001f * (float)sample_rate;
    s->fade_step = 1.0f / fade_samples;
    s->clear_per_sample = SID_SLOT_POOL_LEN / (uint32_t)(int32_t)fade_samples + 1u;
    s->pool = pool;
    sid_slot_templates* t = &s->tpl;
    sid_fx_tape_init(&t->tape, sample_rate);
    sid_fx_overdrive_init(&t->overdrive, sample_rate);
    sid_fx_fuzz_init(&t->fuzz, sample_rate);
    sid_fx_phaser_init(&t->phaser, sample_rate);
    sid_fx_echo_init(&t->delay, sample_rate, SID_SLOT_POOL_LEN, 0);
    sid_fx_echo_init(&t->pingpong, sample_rate, SID_SLOT_POOL_LEN, 1);
    sid_fx_reverb_init(&t->reverb, sample_rate);
    sid_fx_plate_init(&t->plate, sample_rate);
    sid_fx_crush_init(&t->crush, sample_rate);
    sid_fx_crt_init(&t->lofi.crt, sample_rate);
    sid_fx_noise_init(&t->lofi.noise, seed);
    sid_fx_tremolo_init(&t->tremolo, sample_rate);
    sid_slot_mod_state* m = &s->mod;
    sid_fx_chorus_setup(&m->chorus, sample_rate);
    tpl_keep(t->chorus, &m->chorus.line, sizeof t->chorus);
    sid_fx_flanger_setup(&m->flanger, sample_rate);
    tpl_keep(t->flanger, &m->flanger.line, sizeof t->flanger);
    sid_fx_wow_setup(&m->wow, sample_rate, seed * 0x9e3779b9u + 1u);
    tpl_keep(t->wow, &m->wow.line, sizeof t->wow);
    for (uint32_t slot = 0; slot < SID_SLOT_COUNT; ++slot) {
        for (uint32_t alg = 0; alg < SID_SLOT_ALGS; ++alg) {
            float ms = 0.0f;
            if (slot == SID_SLOT_MOD && alg == SID_MOD_CHORUS) {
                ms = SID_FX_CHORUS_CENTRE_MS + SID_FX_CHORUS_SLOW_MS + SID_FX_CHORUS_FAST_MS;
            } else if (slot == SID_SLOT_MOD && alg == SID_MOD_FLANGER) {
                ms = SID_FX_FLANGER_MAX_MS;
            }
            t->warm[slot][alg] = (uint16_t)warm_up(sample_rate, ms);
        }
    }
}
