// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_character.h"
#include "sid_character_cold.h"


void sid_character_macro_changed(sid_character* c, uint32_t field) {
    uint32_t mask = 0;
    switch (field) {
    case 0: mask = (1u << SID_EXPERT_COUNT) - 1u; break;
    case 1:
        mask = (1u << SID_EXPERT_FILTER_6581_CURVE) | (1u << SID_EXPERT_FILTER_6581_RANGE) |
               (1u << SID_EXPERT_FILTER_8580_CURVE) | (1u << SID_EXPERT_NONLINEARITY);
        break;
    case 2: mask = (1u << SID_EXPERT_DAC_KINK) | (1u << SID_EXPERT_DAC_LEAKAGE); break;
    case 3: mask = 1u << SID_EXPERT_THUMP; break;
    case 4: mask = 1u << SID_EXPERT_BLEED; break;
    default: return;
    }
    c->expert_mask &= (uint16_t)~mask;
    for (uint32_t i = 0; i < SID_EXPERT_COUNT; ++i) {
        if (mask & (1u << i)) {
            c->expert[i] = SID_EXPERT_DEFAULT;
        }
    }
}

static void dac_params_for(sid_model model, const sid_character* c, float* kink,
                           float* leakage) {
    *kink = sid_character_expert_value(c, model, SID_EXPERT_DAC_KINK);
    *leakage = sid_character_expert_value(c, model, SID_EXPERT_DAC_LEAKAGE);
}

static void dac_params(const sid* s, const sid_character* c, float* kink, float* leakage) {
    dac_params_for(s->model, c, kink, leakage);
}

static float dac_mismatch(const sid_character* c) {
    return sid_character_response(c->dac) * SID_CHARACTER_MISMATCH;
}

int sid_character_dac_custom_for(sid_model model, const sid_character* c) {
    float kink, leakage;
    dac_params_for(model, c, &kink, &leakage);
    int is6581 = model == SID_MODEL_6581;
    return dac_mismatch(c) != 0.0f || kink != (is6581 ? SID_DAC_KINK_6581 : SID_DAC_KINK_8580) ||
           leakage != (is6581 ? SID_DAC_LEAKAGE_6581 : SID_DAC_LEAKAGE_8580) ||
           sid_character_expert_value(c, model, SID_EXPERT_VOICE_DC) !=
               (is6581 ? 2.0f * (0.2143f / 1.5f) : 0.0f);
}

int sid_character_dac_custom(const sid* s, const sid_character* c) {
    return sid_character_dac_custom_for(s->model, c);
}

void sid_character_build_dac(const sid* s, const sid_character* c, sid_dac* d) {
    sid_dac_job job;
    sid_character_dac_job(s, c, d, &job);
    while (!sid_dac_job_step(&job)) {
    }
}

void sid_character_dac_job(const sid* s, const sid_character* c, sid_dac* d, sid_dac_job* j) {
    float kink, leakage;
    dac_params(s, c, &kink, &leakage);
    sid_dac_job_start(j, d, s->model == SID_MODEL_6581, kink, leakage);
    j->mismatch = dac_mismatch(c);
    j->voice_dc = sid_character_expert_value(c, s->model, SID_EXPERT_VOICE_DC) * 0.5f;
}

