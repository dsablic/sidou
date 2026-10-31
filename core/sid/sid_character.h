// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_CHARACTER_H
#define SIDOU_SID_CHARACTER_H

#include <stdint.h>

#include "sid.h"
#include "gen/grid.h"
#include "gen/profiles.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_CHARACTER_MAX 127u
#define SID_CHARACTER_KINK 0.3f
#define SID_CHARACTER_LEAK 6.0f
#define SID_CHARACTER_MISMATCH 0.5f
#define SID_CHARACTER_THUMP 1.0f
#define SID_CHARACTER_BLEED 0.5f
#define SID_CHARACTER_GRIT 1.0f
#define SID_CHARACTER_CURVE_LINEAR 0.15f
#define SID_CHARACTER_RAMP 4u
#define SID_CHARACTER_BLEED_RAMP 1u
#define SID_CHARACTER_BLEED_RAMP_MAX 4u
#define SID_CHARACTER_DAC_RAMP 2.75f
#define SID_CHARACTER_DAC_RAMP_MAX 6u

#define SID_EXPERT_DEFAULT 255u
enum {
    SID_EXPERT_FILTER_6581_CURVE,
    SID_EXPERT_FILTER_6581_RANGE,
    SID_EXPERT_FILTER_8580_CURVE,
    SID_EXPERT_NONLINEARITY,
    SID_EXPERT_FILTER_BIAS,
    SID_EXPERT_BLEED,
    SID_EXPERT_DAC_KINK,
    SID_EXPERT_DAC_LEAKAGE,
    SID_EXPERT_COMBINED,
    SID_EXPERT_NOISE_WRITEBACK,
    SID_EXPERT_THUMP,
    SID_EXPERT_EXTERNAL_LP,
    SID_EXPERT_EXTERNAL_HP,
    SID_EXPERT_VOICE_DC,
    SID_EXPERT_COUNT
};

#define SID_EXPERT_KINK_MIN 1.8f
#define SID_EXPERT_CURVE_6581_MIN 0.0f
#define SID_EXPERT_CURVE_6581_MAX 1.0f
#define SID_EXPERT_RANGE_6581_MIN 0.0f
#define SID_EXPERT_KINK_MAX 2.6f
#define SID_EXPERT_LEAKAGE_MAX 0.15f
#define SID_EXPERT_BIAS_MAX 2.3125f
#define SID_EXPERT_EXTERNAL_LP_MIN 1000.0f
#define SID_EXPERT_EXTERNAL_LP_MAX 20000.0f
#define SID_EXPERT_EXTERNAL_HP_MAX 100.0f
#define SID_EXPERT_VOICE_DC_MAX 0.6f

typedef struct {
    const char* name;
    uint8_t model;
    uint8_t curve;
    uint8_t hot_curve;
    uint8_t combined;
    float kink, leakage;
    float nonlinearity;
} sid_chip_profile;

const sid_chip_profile* sid_chip_profile_get(uint32_t id);
const char* sid_chip_profile_word(uint32_t id);
uint32_t sid_chip_default(sid_model model);

typedef struct {
    uint8_t profile;
    uint8_t age;
    uint8_t dac;
    uint8_t thump;
    uint8_t bleed;
    uint8_t expert[SID_EXPERT_COUNT];
    uint16_t expert_mask;
} sid_character;

uint32_t sid_character_expert_get(const sid_character* c, sid_model model, uint32_t index);
int sid_character_expert_active(sid_model model, uint32_t index);
float sid_character_expert_cutoff_hz(sid_model model, float curve, float range, uint32_t fc);
float sid_character_expert_value(const sid_character* c, sid_model model, uint32_t index);
void sid_character_expert_set(sid_character* c, sid_model model, uint32_t index, uint32_t value);
void sid_character_macro_changed(sid_character* c, uint32_t field);

void sid_character_init(sid_character* c);

typedef struct {
    sid_nonlin nl;
    const sid_dac* dac;
    float thump, bleed, output_gain;
    uint16_t grit;
    float filter_t;
    float filter_bias, out_c, dc_r;
    const uint16_t* const* pulldown;
    sid_filter_row filter_rows[2];
    uint8_t noise_writeback;
    uint32_t writeback_keep;
    uint32_t noise_keep;
    uint8_t filter_curve, filter_toward;
} sid_character_stage;

typedef struct {
    sid_filter_row rows[2];
    sid_character c;
    uint32_t sample_rate;
    uint16_t at;
    uint8_t model, phase;
    uint8_t transition;
    sid_filter_row source_rows[2];
    float source_t;
} sid_character_filter_job;

void sid_character_filter_job_start(sid_character_filter_job* job, const sid* s,
                                    const sid_character* c, const sid_filter_row* rows);
int sid_character_filter_job_step(sid_character_filter_job* job);

int sid_character_dac_custom(const sid* s, const sid_character* c);
void sid_character_build_dac(const sid* s, const sid_character* c, sid_dac* d);
void sid_character_dac_job(const sid* s, const sid_character* c, sid_dac* d, sid_dac_job* j);
void sid_character_filter(sid* s, const sid_character* c);
void sid_character_stage_for(const sid* s, const sid_character* c, const sid_dac* dac,
                             sid_character_stage* out);

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_character_copy(sid* s, const sid_character_stage* st) {
    s->nl.level = st->nl.level;
    s->nl.level_off = st->nl.level_off;
    s->nl.load = st->nl.load;
    s->nl.floor = st->nl.floor;
    s->dac = st->dac;
    s->thump = st->thump;
    s->bleed = st->bleed;
    s->output_gain = st->output_gain;
    s->grit = st->grit;
    s->filter_t = st->filter_t;
    s->filter_rows[0] = st->filter_rows[0];
    s->filter_rows[1] = st->filter_rows[1];
    s->filter_curve = st->filter_curve;
    s->filter_toward = st->filter_toward;
    s->filter_bias = st->filter_bias;
    uint32_t routed = (s->routing & 1u) + ((s->routing >> 1) & 1u) +
                      ((s->routing >> 2) & 1u) + ((s->routing >> 3) & 1u);
    s->dc_in = s->filter_bias * (float)routed;
    s->out_c = st->out_c;
    s->dc_r = st->dc_r;
    s->noise_writeback = st->noise_writeback;
    if (s->osc[0].writeback_keep != st->writeback_keep) {
        for (uint32_t i = 0; i < 16u; ++i) {
            s->noise_wb[0][i] = SID_NOISE_WB[0][i] | st->writeback_keep;
            s->noise_wb[1][i] = SID_NOISE_WB[1][i];
        }
        for (uint32_t v = 0; v < 3u; ++v) {
            s->osc[v].writeback = st->writeback_keep == 0u ?
                                     SID_NOISE_WB : (const uint32_t (*)[16])s->noise_wb;
            s->osc[v].writeback_keep = st->writeback_keep;
            s->osc[v].noise_keep = st->noise_keep;
            s->osc[v].quiet = 0;
        }
    }
    for (uint32_t v = 0; v < 3u; ++v) {
        if (s->osc[v].pulldown != st->pulldown) {
            s->osc[v].pulldown = st->pulldown;
            sid_osc_update_mode(&s->osc[v]);
        }
    }
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_character_commit(sid* s, const sid_character_stage* st) {
    sid_character_copy(s, st);
    sid_update_filter(s);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_character_commit_later(sid* s, const sid_character_stage* st) {
    sid_character_copy(s, st);
    s->filter_dirty = 1;
}

const sid_dac* sid_character_apply(sid* s, const sid_character* c, sid_dac* dac);
void sid_character_change(sid* s, const sid_character* c, sid_dac* dac);

#define SID_PREP_ROW_G 410u
#define SID_PREP_ROW_X 1024u
typedef struct {
    sid* s;
    sid_character c;
    const sid_dac* table;
    sid_dac_job job;
    sid_filter_row rows[2];
    sid_character_filter_job filter_job;
    uint32_t clock_hz, sample_rate;
    uint16_t at;
    uint8_t model;
    uint8_t phase;
    uint8_t default_left;
} sid_character_prep;

void sid_character_prep_update(sid_character_prep* p, const sid_character* c);

void sid_character_prep_start(sid_character_prep* p, sid* s, sid_model model,
                              uint32_t clock_hz, uint32_t sample_rate, const sid_character* c,
                              uint32_t default_steps);
void sid_character_prep_rows(sid_character_prep* p, const sid_filter_row* rows);
int sid_character_prep_step(sid_character_prep* p, sid_dac* dac);
int sid_character_prep_done(const sid_character_prep* p);
uint32_t sid_character_prep_slices(sid_model model, const sid_character* c);

typedef struct {
    sid_character target;
    sid_character now;
    const sid_dac* table;
    sid_dac_job job;
    uint8_t building;
    uint8_t next_dac;
    sid_character dac_next;
    sid_character filter_now;
    sid_character_filter_job filter_job;
    uint8_t filter_building;
    uint8_t filter_fading, filter_fade, filter_restore;
    sid_filter_row filter_rows[2];
    float filter_t;
    uint8_t filter_rows_valid, filter_managed;
    int8_t bleed_half;
    uint8_t grit_dac;
} sid_character_ramp;

void sid_character_ramp_reset(sid_character_ramp* r, const sid_character* c,
                              const sid_dac* table);
int sid_character_ramp_tick(sid_character_ramp* r, const sid* s, sid_dac* free_table,
                            sid_character_stage* out);
int sid_character_ramp_tick_tables(sid_character_ramp* r, const sid* s, sid_dac* free_table,
                                   const sid_filter_row* free_rows, sid_character_stage* out);
int sid_character_ramp_wants_table(const sid_character_ramp* r);

#ifdef __cplusplus
}
#endif

#endif
