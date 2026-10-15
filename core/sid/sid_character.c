// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_character.h"

#include <stddef.h>

static const sid_chip_profile profiles[SID_CHIPS] = SID_CHIP_PROFILE_ROWS;

_Static_assert(sizeof profiles / sizeof profiles[0] == SID_CHIPS, "a row per profile");

const sid_chip_profile* sid_chip_profile_get(uint32_t id) {
    return id < SID_CHIPS ? &profiles[id] : NULL;
}

uint32_t sid_chip_default(sid_model model) {
    return model == SID_MODEL_8580 ? SID_CHIP_8580_R5 : SID_CHIP_6581_R4AR;
}

void sid_character_init(sid_character* c) {
    c->profile = 0;
    c->age = 0;
    c->dac = 0;
    c->thump = 0;
    c->bleed = 0;
}

/* The character's profile for the chip: its own, or the chip's model's default. */
static const sid_chip_profile* profile_of(const sid* s, const sid_character* c) {
    const sid_chip_profile* p = c->profile ? sid_chip_profile_get(c->profile - 1u) : NULL;
    if (!p || p->model != (uint8_t)s->model) {
        p = &profiles[sid_chip_default(s->model)];
    }
    return p;
}

/* A control as 0..1. */
static float amount(uint8_t v) {
    return (float)(v > SID_CHARACTER_MAX ? SID_CHARACTER_MAX : v) / (float)SID_CHARACTER_MAX;
}

static void dac_params(const sid* s, const sid_character* c, float* kink, float* leakage) {
    const sid_chip_profile* p = profile_of(s, c);
    float a = amount(c->dac);
    *kink = p->kink + a * SID_CHARACTER_KINK;
    *leakage = p->leakage * (1.0f + a * (SID_CHARACTER_LEAK - 1.0f));
}

int sid_character_dac_custom(const sid* s, const sid_character* c) {
    float kink, leakage;
    dac_params(s, c, &kink, &leakage);
    int is6581 = s->model == SID_MODEL_6581;
    return kink != (is6581 ? SID_DAC_KINK_6581 : SID_DAC_KINK_8580) ||
           leakage != (is6581 ? SID_DAC_LEAKAGE_6581 : SID_DAC_LEAKAGE_8580);
}

void sid_character_build_dac(const sid* s, const sid_character* c, sid_dac* d) {
    float kink, leakage;
    dac_params(s, c, &kink, &leakage);
    sid_dac_build(d, s->model == SID_MODEL_6581, kink, leakage);
}

void sid_character_dac_job(const sid* s, const sid_character* c, sid_dac* d, sid_dac_job* j) {
    float kink, leakage;
    dac_params(s, c, &kink, &leakage);
    sid_dac_job_start(j, d, s->model == SID_MODEL_6581, kink, leakage);
}

void sid_character_filter(sid* s, const sid_character* c) {
    const sid_chip_profile* p = profile_of(s, c);
    if (p->curve != s->filter_curve) {
        sid_set_filter_blend(s, p->curve, p->hot_curve, s->filter_t);
    } else {
        sid_set_filter_toward(s, p->hot_curve);
    }
}

void sid_character_stage_for(const sid* s, const sid_character* c, const sid_dac* dac,
                             sid_character_stage* out) {
    const sid_chip_profile* p = profile_of(s, c);
    float age = amount(c->age);
    float nl = p->nonlinearity * (1.0f + age);
    sid_nonlin_for(s, nl, age, &out->nl); /* clamped to 2 there */
    out->dac = dac ? dac : sid_dac_default(s->model == SID_MODEL_6581);
    out->thump = amount(c->thump) * SID_CHARACTER_THUMP;
    out->bleed = amount(c->bleed) * SID_CHARACTER_BLEED;
    out->filter_t = age;
    /* The thump's headroom: full scale moves out by the thump, which is the largest step a
     * volume write can add (from 0 to 15), so a signal that peaked at full scale and that
     * step together still do. At 0 the gain is sid_init's exactly. */
    float base = sid_base_output_gain(s);
    out->output_gain = out->thump > 0.0f ? 32767.0f / (32767.0f / base + out->thump) : base;
}

/* The filter tables, DACs and stage for the character, on a chip that is not rendering;
 * the DAC table it got (NULL: the model's default). */
static const sid_dac* change(sid* s, const sid_character* c, sid_dac* dac) {
    sid_character_filter(s, c);
    const sid_dac* table = NULL;
    if (dac && sid_character_dac_custom(s, c)) {
        sid_character_build_dac(s, c, dac);
        table = dac;
    }
    sid_character_stage st;
    sid_character_stage_for(s, c, table, &st);
    sid_character_commit(s, &st); /* the filter's coefficients too */
    return table;
}

const sid_dac* sid_character_apply(sid* s, const sid_character* c, sid_dac* dac) {
    const sid_chip_profile* p = profile_of(s, c);
    sid_set_combined_profile(s, p->combined);
    return change(s, c, dac);
}

void sid_character_change(sid* s, const sid_character* c, sid_dac* dac) {
    change(s, c, dac);
}

void sid_character_ramp_reset(sid_character_ramp* r, const sid_character* c,
                              const sid_dac* table) {
    r->target = *c;
    r->now = *c;
    r->table = table;
    r->building = 0;
    r->next_dac = c->dac;
}

/* v moved towards to by at most step. */
static uint8_t toward(uint8_t v, uint8_t to, uint32_t step) {
    if (v < to) {
        return (uint8_t)((uint32_t)(to - v) > step ? v + step : to);
    }
    return (uint8_t)((uint32_t)(v - to) > step ? v - step : to);
}

/* A control's setting as the ramp takes it: clamped, as amount() reads it. */
static uint8_t clamped(uint8_t v) {
    return v > SID_CHARACTER_MAX ? (uint8_t)SID_CHARACTER_MAX : v;
}

int sid_character_ramp_tick(sid_character_ramp* r, const sid* s, sid_dac* free_table,
                            sid_character_stage* out) {
    sid_character* now = &r->now;
    const sid_character* want = &r->target;
    sid_character was = *now;
    now->age = toward(now->age, clamped(want->age), SID_CHARACTER_RAMP);
    now->thump = toward(now->thump, clamped(want->thump), SID_CHARACTER_RAMP);
    now->bleed = toward(now->bleed, clamped(want->bleed), SID_CHARACTER_BLEED_RAMP);
    int changed = now->age != was.age || now->thump != was.thump || now->bleed != was.bleed;
    if (r->building) {
        if (sid_dac_job_step(&r->job)) {
            r->building = 0;
            now->dac = r->next_dac;
            r->table = r->job.d;
            changed = 1;
        }
    } else if (now->dac != clamped(want->dac)) {
        sid_character next = *now;
        next.dac = toward(now->dac, clamped(want->dac), SID_CHARACTER_DAC_RAMP);
        if (!sid_character_dac_custom(s, &next)) {
            now->dac = next.dac;
            r->table = NULL;
            changed = 1;
        } else if (free_table) {
            sid_character_dac_job(s, &next, free_table, &r->job);
            r->next_dac = next.dac;
            r->building = 1;
            sid_dac_job_step(&r->job); /* the first step now: the table in SID_DAC_BUILD_STEPS */
        }
    }
    if (changed) {
        sid_character_stage_for(s, now, r->table, out);
    }
    return changed;
}
