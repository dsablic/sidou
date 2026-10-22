// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_character.h"

#include <stddef.h>

#include "gen/expert.h"

static const sid_chip_profile profiles[SID_CHIPS] = SID_CHIP_PROFILE_ROWS;
#define EXPERT_ROW_G 128u
#define EXPERT_ROW_X 192u
_Static_assert(SID_EXPERT_COUNT <= 16u, "expert overrides fit their mask");
static int filter_overridden(const sid_character* c, sid_model model);
static void expert_copy(sid_character* to, const sid_character* from, uint32_t index);

_Static_assert(sizeof profiles / sizeof profiles[0] == SID_CHIPS, "a row per profile");
_Static_assert(SID_EXPERT_COUNT <= 16u, "expert flags fit their mask and register range");

const sid_chip_profile* sid_chip_profile_get(uint32_t id) {
    return id < SID_CHIPS ? &profiles[id] : NULL;
}

const char* sid_chip_profile_word(uint32_t id) {
    static const char* const WORDS[SID_CHIPS] = {"BRIGHT", "DARK", "CLASSIC", "CLEAN", "GRITTY"};
    return id < SID_CHIPS ? WORDS[id] : "?";
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
    c->expert_mask = 0;
    for (uint32_t i = 0; i < SID_EXPERT_COUNT; ++i) {
        c->expert[i] = SID_EXPERT_DEFAULT;
    }
}

/* The character's profile for a chip of the model: its own, or the model's default. */
static const sid_chip_profile* profile_for(sid_model model, const sid_character* c) {
    const sid_chip_profile* p = c->profile ? sid_chip_profile_get(c->profile - 1u) : NULL;
    if (!p || p->model != (uint8_t)model) {
        p = &profiles[sid_chip_default(model)];
    }
    return p;
}

static const sid_chip_profile* profile_of(const sid* s, const sid_character* c) {
    return profile_for(s->model, c);
}

/* A control as 0..1. */
static float amount(uint8_t v) {
    return (float)(v > SID_CHARACTER_MAX ? SID_CHARACTER_MAX : v) / (float)SID_CHARACTER_MAX;
}

/* A control's effect as 0..1 (sid_character.h, SID_CHARACTER_CURVE_LINEAR): a little linear
 * and the rest squared, so the first detents are subtle and the top is obvious. */
static float response(uint8_t v) {
    float a = amount(v);
    return a * (SID_CHARACTER_CURVE_LINEAR + (1.0f - SID_CHARACTER_CURVE_LINEAR) * a);
}

static int overridden(const sid_character* c, uint32_t index) {
    return index < SID_EXPERT_COUNT && (c->expert_mask & (1u << index)) &&
           c->expert[index] != SID_EXPERT_DEFAULT;
}

static float expert_default(const sid_character* c, sid_model model, uint32_t index) {
    const sid_chip_profile* p = profile_for(model, c);
    float age = amount(c->age), dac = response(c->dac);
    static const float curve[SID_CHIPS] = {0.58f, 0.42f, 0.5f, 0.5f, 0.35f};
    uint32_t id = (uint32_t)(p - profiles);
    switch (index) {
    case SID_EXPERT_FILTER_6581_CURVE:
        return model == SID_MODEL_6581 ? curve[id] + age * (0.25f - curve[id]) : 0.5f;
    case SID_EXPERT_FILTER_6581_RANGE: {
        float base = id == SID_CHIP_6581_WORN ? 0.4f : 19.0f / 39.0f;
        return base + age * (0.3f - base);
    }
    case SID_EXPERT_FILTER_8580_CURVE: return 0.5f + age * 0.25f;
    case SID_EXPERT_NONLINEARITY: {
        float nl = p->nonlinearity * (1.0f + age);
        return nl > 2.0f ? 2.0f : nl;
    }
    case SID_EXPERT_FILTER_BIAS: return 1.15625f;
    case SID_EXPERT_BLEED: return response(c->bleed) * SID_CHARACTER_BLEED;
    case SID_EXPERT_DAC_KINK: return p->kink + dac * SID_CHARACTER_KINK;
    case SID_EXPERT_DAC_LEAKAGE: return p->leakage * (1.0f + dac * (SID_CHARACTER_LEAK - 1.0f));
    case SID_EXPERT_COMBINED: return (float)p->combined;
    case SID_EXPERT_NOISE_WRITEBACK: return 1.0f;
    case SID_EXPERT_THUMP: return response(c->thump) * SID_CHARACTER_THUMP;
    case SID_EXPERT_EXTERNAL_LP: return 13154.0f;
    case SID_EXPERT_EXTERNAL_HP: return 16.0f;
    case SID_EXPERT_VOICE_DC: return model == SID_MODEL_6581 ? 2.0f * (0.2143f / 1.5f) : 0.0f;
    default: return 0.0f;
    }
}

static void expert_range(uint32_t index, float* lo, float* hi) {
    *lo = 0.0f;
    *hi = 1.0f;
    switch (index) {
    case SID_EXPERT_FILTER_6581_CURVE:
        *lo = SID_EXPERT_CURVE_6581_MIN;
        *hi = SID_EXPERT_CURVE_6581_MAX;
        break;
    case SID_EXPERT_FILTER_6581_RANGE: *lo = SID_EXPERT_RANGE_6581_MIN; break;
    case SID_EXPERT_NONLINEARITY: *hi = 2.0f; break;
    case SID_EXPERT_FILTER_BIAS: *hi = SID_EXPERT_BIAS_MAX; break;
    case SID_EXPERT_BLEED: *hi = SID_CHARACTER_BLEED; break;
    case SID_EXPERT_DAC_KINK: *lo = SID_EXPERT_KINK_MIN; *hi = SID_EXPERT_KINK_MAX; break;
    case SID_EXPERT_DAC_LEAKAGE: *hi = SID_EXPERT_LEAKAGE_MAX; break;
    case SID_EXPERT_COMBINED: *hi = 2.0f; break;
    case SID_EXPERT_EXTERNAL_LP:
        *lo = SID_EXPERT_EXTERNAL_LP_MIN;
        *hi = SID_EXPERT_EXTERNAL_LP_MAX;
        break;
    case SID_EXPERT_EXTERNAL_HP: *hi = SID_EXPERT_EXTERNAL_HP_MAX; break;
    case SID_EXPERT_VOICE_DC: *hi = SID_EXPERT_VOICE_DC_MAX; break;
    default: break;
    }
}

float sid_character_expert_value(const sid_character* c, sid_model model, uint32_t index) {
    if (!overridden(c, index)) {
        return expert_default(c, model, index);
    }
    float lo, hi;
    expert_range(index, &lo, &hi);
    if (index == SID_EXPERT_COMBINED) {
        uint32_t raw = c->expert[index] > SID_CHARACTER_MAX ? SID_CHARACTER_MAX : c->expert[index];
        return (float)((raw * 2u + 63u) / 127u);
    }
    return lo + amount(c->expert[index]) * (hi - lo);
}

uint32_t sid_character_expert_get(const sid_character* c, sid_model model, uint32_t index) {
    if (index >= SID_EXPERT_COUNT) {
        return SID_EXPERT_DEFAULT;
    }
    if (overridden(c, index)) {
        return c->expert[index] > SID_CHARACTER_MAX ? SID_CHARACTER_MAX : c->expert[index];
    }
    float lo, hi;
    expert_range(index, &lo, &hi);
    float raw = (expert_default(c, model, index) - lo) * 127.0f / (hi - lo);
    return raw < 0.0f ? 0u : raw > 127.0f ? 127u : (uint32_t)(raw + 0.5f);
}

int sid_character_expert_active(sid_model model, uint32_t index) {
    switch (index) {
    case SID_EXPERT_FILTER_6581_CURVE:
    case SID_EXPERT_FILTER_6581_RANGE:
    case SID_EXPERT_NONLINEARITY:
    case SID_EXPERT_FILTER_BIAS:
    case SID_EXPERT_EXTERNAL_LP:
        return model == SID_MODEL_6581;
    case SID_EXPERT_FILTER_8580_CURVE:
        return model == SID_MODEL_8580;
    case SID_EXPERT_COMBINED:
#ifndef SIDOU_ALL_COMBINED_PROFILES
        if (model == SID_MODEL_6581) {
            return 0;
        }
#endif
#ifndef SIDOU_ALL_COMBINED_PROFILES_8580
        if (model == SID_MODEL_8580) {
            return 0;
        }
#endif
        return 1;
    default: return index < SID_EXPERT_COUNT;
    }
}

void sid_character_expert_set(sid_character* c, sid_model model, uint32_t index, uint32_t value) {
    (void)model;
    if (index >= SID_EXPERT_COUNT) {
        return;
    }
    c->expert[index] = (uint8_t)(value == SID_EXPERT_DEFAULT ? SID_EXPERT_DEFAULT :
                               value > SID_CHARACTER_MAX ? SID_CHARACTER_MAX : value);
    if (value == SID_EXPERT_DEFAULT) {
        c->expert_mask &= (uint16_t)~(1u << index);
    } else {
        c->expert_mask |= (1u << index);
    }
}

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

/* The DAC macro's bit errors (sid_dac_mismatch): no expert parameter of their own, they
 * follow the macro whatever the kink and leakage are set to. */
static float dac_mismatch(const sid_character* c) {
    return response(c->dac) * SID_CHARACTER_MISMATCH;
}

static int dac_custom_for(sid_model model, const sid_character* c) {
    float kink, leakage;
    dac_params_for(model, c, &kink, &leakage);
    int is6581 = model == SID_MODEL_6581;
    return dac_mismatch(c) != 0.0f || kink != (is6581 ? SID_DAC_KINK_6581 : SID_DAC_KINK_8580) ||
           leakage != (is6581 ? SID_DAC_LEAKAGE_6581 : SID_DAC_LEAKAGE_8580) ||
           sid_character_expert_value(c, model, SID_EXPERT_VOICE_DC) !=
               (is6581 ? 2.0f * (0.2143f / 1.5f) : 0.0f);
}

int sid_character_dac_custom(const sid* s, const sid_character* c) {
    return dac_custom_for(s->model, c);
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

void sid_character_filter(sid* s, const sid_character* c) {
    if (filter_overridden(c, s->model)) {
        sid_character_filter_job job;
        sid_filter_row rows[2] = {s->filter_rows[0], s->filter_rows[1]};
        sid_character_filter_job_start(&job, s, c, rows);
        while (!sid_character_filter_job_step(&job)) {
        }
        s->filter_rows[0] = rows[0];
        s->filter_rows[1] = rows[1];
        s->filter_curve = s->filter_toward = SID_EXPERT_DEFAULT;
        return;
    }
    const sid_chip_profile* p = profile_of(s, c);
    if (p->curve != s->filter_curve) {
        sid_set_filter_blend(s, p->curve, p->hot_curve, s->filter_t);
    } else {
        sid_set_filter_toward(s, p->hot_curve);
    }
}

void sid_character_stage_for(const sid* s, const sid_character* c, const sid_dac* dac,
                             sid_character_stage* out) {
    float age = amount(c->age);
    float nl = sid_character_expert_value(c, s->model, SID_EXPERT_NONLINEARITY);
    float filter_t = filter_overridden(c, s->model) ? 0.0f : age;
    sid_nonlin_for(s, nl, filter_t, &out->nl); /* clamped to 2 there */
    out->dac = dac ? dac : sid_dac_default(s->model == SID_MODEL_6581);
    out->thump = sid_character_expert_value(c, s->model, SID_EXPERT_THUMP);
    out->bleed = sid_character_expert_value(c, s->model, SID_EXPERT_BLEED);
    out->filter_t = filter_t;
    out->filter_rows[0] = s->filter_rows[0];
    out->filter_rows[1] = s->filter_rows[1];
    out->filter_curve = s->filter_curve;
    out->filter_toward = s->filter_toward;
    out->filter_bias = sid_character_expert_value(c, s->model, SID_EXPERT_FILTER_BIAS);
    sid_external_coefficients(s->sample_rate,
                              sid_character_expert_value(c, s->model, SID_EXPERT_EXTERNAL_LP),
                              sid_character_expert_value(c, s->model, SID_EXPERT_EXTERNAL_HP),
                              &out->out_c, &out->dc_r);
    out->pulldown = sid_combined_table(s->model, (uint32_t)sid_character_expert_value(c, s->model,
                                                                                SID_EXPERT_COMBINED));
    out->noise_writeback = (uint8_t)sid_character_expert_get(c, s->model,
                                                          SID_EXPERT_NOISE_WRITEBACK);
    uint32_t taps = (out->noise_writeback * 8u + 126u) / 127u;
    uint32_t kept = sid_noise_writeback((0xffu >> taps) << 4);
    out->writeback_keep = kept;
    out->noise_keep = sid_noise_output(kept);
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

/* The preparation's pieces (sid_character_prep_step), in order. */
enum { PREP_DEFAULT_DAC, PREP_ROW_G, PREP_ROW_X, PREP_EXPERT_FILTER, PREP_DAC, PREP_DONE };

#define ROW_SLICES(n) ((SID_FC_STEPS + (n) - 1u) / (n))

void sid_character_prep_start(sid_character_prep* p, sid* s, sid_model model,
                              uint32_t clock_hz, uint32_t sample_rate, const sid_character* c,
                              uint32_t default_steps) {
    p->default_left = (uint8_t)(default_steps < SID_DAC_BUILD_STEPS ? default_steps
                                                                     : SID_DAC_BUILD_STEPS);
    p->s = s;
    p->c = *c;
    p->table = NULL;
    p->job.d = NULL;
    p->clock_hz = clock_hz;
    p->sample_rate = sample_rate;
    p->at = 0;
    p->model = (uint8_t)model;
    p->phase = PREP_DEFAULT_DAC;
    p->rows[0] = sid_filter_backing(s, 0);
    p->rows[1] = sid_filter_backing(s, 1);
}

void sid_character_prep_rows(sid_character_prep* p, const sid_filter_row* rows) {
    p->rows[0] = rows[0];
    p->rows[1] = rows[1];
}

void sid_character_prep_update(sid_character_prep* p, const sid_character* c) {
    if (p->phase == PREP_DONE) {
        return;
    }
    if (!filter_overridden(&p->c, (sid_model)p->model) || p->phase < PREP_EXPERT_FILTER) {
        p->c.age = c->age;
    }
    p->c.thump = c->thump;
    p->c.bleed = c->bleed;
    if (p->phase < PREP_DAC) {
        p->c.dac = c->dac;
    }
    for (uint32_t i = SID_EXPERT_NONLINEARITY; i < SID_EXPERT_COUNT; ++i) {
        int is_dac = i == SID_EXPERT_DAC_KINK || i == SID_EXPERT_DAC_LEAKAGE || i == SID_EXPERT_VOICE_DC;
        if (!is_dac || p->phase < PREP_DAC) {
            expert_copy(&p->c, c, i);
        }
    }
}

int sid_character_prep_done(const sid_character_prep* p) {
    return p->phase == PREP_DONE;
}

/* sid_init_end and sid_character_apply's stage: what is left once the tables are built. */
static void prep_finish(sid_character_prep* p) {
    sid* s = p->s;
    sid_init_end(s);
    sid_character_stage st;
    sid_character_stage_for(s, &p->c, p->table, &st);
    sid_character_commit(s, &st); /* the filter's coefficients too */
    p->phase = PREP_DONE;
}

/* The pieces follow sid_init and sid_character_apply (sid_character_filter, then the DACs,
 * then the stage), building each table once: the first row is the profile's curve setting
 * and the second its hot row, as sid_character_filter leaves them whichever way it gets
 * there (both rows anew, or sid_init's first and the second anew). */
int sid_character_prep_step(sid_character_prep* p, sid_dac* dac) {
    sid* s = p->s;
    const sid_chip_profile* prof = profile_for((sid_model)p->model, &p->c);
    switch (p->phase) {
    case PREP_DEFAULT_DAC:
        /* sid_init_begin points the chip at the model's default DACs: on the device the
         * 8580's are built on first use, a step a slice, not all in one. The caller says how
         * many slices to spend here, whether or not a step is left to do (on hosts the first
         * builds the whole table, and another preparer may have built it), so the count is
         * the same everywhere. */
        if (p->default_left) {
            p->default_left--;
            sid_dac_default_step(p->model == SID_MODEL_6581);
            return 0;
        }
        p->phase = PREP_ROW_G;
        /* fall through */
    case PREP_ROW_G:
        if (p->at == 0u) {
            sid_init_begin(s, (sid_model)p->model, p->clock_hz, p->sample_rate);
            s->filter_rows[0] = p->rows[0];
            s->filter_rows[1] = p->rows[1];
            sid_set_combined_profile(s, prof->combined);
            if (filter_overridden(&p->c, (sid_model)p->model)) {
                sid_character_filter_job_start(&p->filter_job, s, &p->c, p->rows);
                p->phase = PREP_EXPERT_FILTER;
                sid_character_filter_job_step(&p->filter_job);
                return 0;
            }
        }
        sid_filter_row_part(s, 0, prof->curve, p->at, p->at + SID_PREP_ROW_G);
        p->at = (uint16_t)(p->at + SID_PREP_ROW_G);
        if (p->at >= SID_FC_STEPS) {
            p->at = 0;
            p->phase = PREP_ROW_X;
        }
        return 0;
    case PREP_EXPERT_FILTER:
        if (!sid_character_filter_job_step(&p->filter_job)) {
            return 0;
        }
        s->filter_curve = s->filter_toward = SID_EXPERT_DEFAULT;
        p->phase = PREP_DAC;
        if (!sid_character_dac_custom(s, &p->c)) {
            prep_finish(p);
            return 1;
        }
        return 0;
    case PREP_ROW_X:
        sid_filter_row_part(s, 1, prof->hot_curve, p->at, p->at + SID_PREP_ROW_X);
        p->at = (uint16_t)(p->at + SID_PREP_ROW_X);
        if (p->at < SID_FC_STEPS) {
            return 0;
        }
        p->phase = PREP_DAC;
        if (!sid_character_dac_custom(s, &p->c)) {
            prep_finish(p);
            return 1;
        }
        return 0;
    case PREP_DAC:
        if (!p->job.d) {
            if (!dac) {
                return 0;
            }
            sid_character_dac_job(s, &p->c, dac, &p->job);
        }
        if (!sid_dac_job_step(&p->job)) {
            return 0;
        }
        p->table = p->job.d;
        prep_finish(p);
        return 1;
    default: return 1;
    }
}

uint32_t sid_character_prep_slices(sid_model model, const sid_character* c) {
    return (filter_overridden(c, model) ? ROW_SLICES(EXPERT_ROW_G) + ROW_SLICES(EXPERT_ROW_X) :
                                        ROW_SLICES(SID_PREP_ROW_G) + ROW_SLICES(SID_PREP_ROW_X)) +
           (dac_custom_for(model, c) ? SID_DAC_BUILD_STEPS : 0u);
}

static float anchor_hz(const float* row, uint32_t fc) {
    uint32_t at = fc >> 6;
    uint32_t span = at == 31u ? 63u : 64u;
    float t = (float)(fc - at * 64u) / (float)span;
    return row[at] + t * (row[at + 1u] - row[at]);
}

#define GRID_COUNT(a) (sizeof(a) / sizeof((a)[0]))
_Static_assert(GRID_COUNT(SID_EXPERT_HZ_6581) == GRID_COUNT(SID_EXPERT_RANGE_6581),
               "expert range coordinates match generated rows");
_Static_assert(GRID_COUNT(SID_EXPERT_HZ_6581[0]) == GRID_COUNT(SID_EXPERT_CURVE_6581),
               "expert curve coordinates match generated rows");
_Static_assert(GRID_COUNT(SID_EXPERT_HZ_8580) == GRID_COUNT(SID_EXPERT_RANGE_8580) &&
               GRID_COUNT(SID_EXPERT_HZ_8580[0]) == GRID_COUNT(SID_EXPERT_CURVE_8580),
               "8580 coordinates match generated rows");

static uint32_t coordinate_interval(const float* coordinates, uint32_t count, float value) {
    uint32_t at = 0;
    while (at + 2u < count && value >= coordinates[at + 1u]) {
        at++;
    }
    return at;
}

static float curve_hz(const float (*rows)[33], const float* coordinates, uint32_t count,
                       float curve, uint32_t fc) {
    uint32_t at = coordinate_interval(coordinates, count, curve);
    float t = (curve - coordinates[at]) / (coordinates[at + 1u] - coordinates[at]);
    float a = anchor_hz(rows[at], fc), b = anchor_hz(rows[at + 1u], fc);
    return a + t * (b - a);
}

float sid_character_expert_cutoff_hz(sid_model model, float curve, float range, uint32_t fc) {
    curve = !(curve >= 0.0f) ? 0.0f : curve > 1.0f ? 1.0f : curve;
    range = !(range >= 0.0f) ? 0.0f : range > 1.0f ? 1.0f : range;
    fc &= 2047u;
    if (model == SID_MODEL_8580) {
        return curve_hz(SID_EXPERT_HZ_8580[0], SID_EXPERT_CURVE_8580,
                         GRID_COUNT(SID_EXPERT_CURVE_8580), curve, fc);
    }
    uint32_t at = coordinate_interval(SID_EXPERT_RANGE_6581, GRID_COUNT(SID_EXPERT_RANGE_6581), range);
    float t = (range - SID_EXPERT_RANGE_6581[at]) /
              (SID_EXPERT_RANGE_6581[at + 1u] - SID_EXPERT_RANGE_6581[at]);
    float a = curve_hz(SID_EXPERT_HZ_6581[at], SID_EXPERT_CURVE_6581,
                       GRID_COUNT(SID_EXPERT_CURVE_6581), curve, fc);
    float b = curve_hz(SID_EXPERT_HZ_6581[at + 1u], SID_EXPERT_CURVE_6581,
                       GRID_COUNT(SID_EXPERT_CURVE_6581), curve, fc);
    return a + t * (b - a);
}

void sid_character_filter_job_start(sid_character_filter_job* job, const sid* s,
                                    const sid_character* c, const sid_filter_row* rows) {
    job->rows[0] = rows[0];
    job->rows[1] = rows[1];
    job->c = *c;
    job->sample_rate = s->sample_rate;
    job->model = (uint8_t)s->model;
    job->at = 0;
    job->phase = 0;
    job->transition = 0;
    job->source_rows[0] = s->filter_rows[0];
    job->source_rows[1] = s->filter_rows[1];
    job->source_t = s->filter_t;
}

/* Bilinear curve/range interpolation adds work to each cutoff; use smaller pieces
 * than the profile-only preparer. No new table storage, only caller-leased rows. */

int sid_character_filter_job_step(sid_character_filter_job* job) {
    if (job->phase >= 2u) {
        return 1;
    }
    sid_model model = (sid_model)job->model;
    const sid_character* c = &job->c;
    const sid_chip_profile* p = profile_for(model, c);
    uint32_t curve_id = job->phase ? p->hot_curve : p->curve;
    float curve = sid_character_expert_value(c, model, model == SID_MODEL_6581 ?
                                            SID_EXPERT_FILTER_6581_CURVE : SID_EXPERT_FILTER_8580_CURVE);
    float range = sid_character_expert_value(c, model, SID_EXPERT_FILTER_6581_RANGE);
    int edited = filter_overridden(c, model);
    uint32_t count = job->phase ? EXPERT_ROW_X : EXPERT_ROW_G;
    uint32_t end = job->at + count;
    if (end > SID_FC_STEPS) {
        end = SID_FC_STEPS;
    }
    float fs = (float)job->sample_rate;
    sid_filter_row row = job->rows[job->phase];
    for (uint32_t fc = job->at; fc < end; ++fc) {
        if (job->transition && job->phase == 0u) {
            row.cutoff[fc] = sid_filter_cutoff(job->source_rows, fc, job->source_t);
            continue;
        }
        float hz = edited ? sid_character_expert_cutoff_hz(model, curve, range, fc) :
                            sid_cutoff_hz(model, curve_id, fc);
        if (hz < 30.0f) {
            hz = 30.0f;
        }
        if (hz > 0.45f * fs) {
            hz = 0.45f * fs;
        }
        float x = SIDOU_PI * hz / fs;
        if (job->transition && !edited) {
            float base = sid_cutoff_hz(model, p->curve, fc);
            if (base < 30.0f) {
                base = 30.0f;
            }
            if (base > 0.45f * fs) {
                base = 0.45f * fs;
            }
            float from = SIDOU_PI * base / fs;
            x = from + amount(c->age) * (x - from);
        }
        row.cutoff[fc] = job->phase ? x : sidou_tanf(x);
    }
    job->at = (uint16_t)end;
    if (end == SID_FC_STEPS) {
        for (uint32_t res = 0; res < 16u; ++res) {
            if (job->transition && job->phase == 0u) {
                float a = job->source_rows[0].res[res];
                row.res[res] = a + job->source_t * (job->source_rows[1].res[res] - a);
            } else if (job->transition && !edited) {
                float a = sid_resonance_k(model, p->curve, res);
                row.res[res] = a + amount(c->age) * (sid_resonance_k(model, p->hot_curve, res) - a);
            } else {
                row.res[res] = sid_resonance_k(model, edited ? p->curve : curve_id, res);
            }
        }
        job->phase++;
        job->at = 0;
    }
    return job->phase >= 2u;
}

void sid_character_ramp_reset(sid_character_ramp* r, const sid_character* c,
                              const sid_dac* table) {
    r->target = *c;
    r->now = *c;
    r->table = table;
    r->building = 0;
    r->next_dac = c->dac;
    r->dac_next = *c;
    r->filter_now = *c;
    r->filter_building = 0;
    r->filter_fading = r->filter_fade = r->filter_restore = 0;
    r->filter_rows_valid = r->filter_managed = 0;
    r->bleed_half = 0;
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

static void expert_copy(sid_character* to, const sid_character* from, uint32_t index) {
    uint16_t mask = (uint16_t)(1u << index);
    to->expert[index] = from->expert[index];
    to->expert_mask = (uint16_t)((to->expert_mask & ~mask) | (from->expert_mask & mask));
}

static int expert_differs(const sid_character* a, const sid_character* b, uint32_t index) {
    int aa = overridden(a, index), bb = overridden(b, index);
    return aa != bb || (aa && a->expert[index] != b->expert[index]);
}

static int expert_step(sid_character* now, const sid_character* want, sid_model model,
                        uint32_t index) {
    if (!expert_differs(now, want, index)) {
        return 0;
    }
    uint8_t v = (uint8_t)sid_character_expert_get(now, model, index);
    uint8_t to = (uint8_t)sid_character_expert_get(want, model, index);
    uint8_t next = toward(v, to, 1u);
    if (next == to) {
        expert_copy(now, want, index);
    } else {
        sid_character_expert_set(now, model, index, next);
    }
    return 1;
}

/* The most detents (at most max) a ramp moves a control on the perceptual curve from now
 * towards to in one step, keeping the effect's change within lim 127ths of its amount (what a
 * detent of a linear control moves it); 0 if even one detent changes it more. */
static uint32_t ramp_step(uint8_t now, uint8_t to, float lim, uint32_t max) {
    uint32_t gap = now < to ? (uint32_t)(to - now) : (uint32_t)(now - to);
    float from = response(now);
    for (uint32_t step = max < gap ? max : gap; step > 0u; --step) {
        float d = response((uint8_t)(now < to ? now + step : now - step)) - from;
        if ((d < 0.0f ? -d : d) * (float)SID_CHARACTER_MAX <= lim) {
            return step;
        }
    }
    return 0;
}

/* ramp_step, but at least a detent. */
static uint8_t ramp_toward(uint8_t now, uint8_t to, float lim, uint32_t max) {
    uint32_t step = ramp_step(now, to, lim, max);
    return toward(now, to, step ? step : 1u);
}

static int dac_differs(const sid_character* a, const sid_character* b) {
    if (a->dac != clamped(b->dac)) {
        return 1;
    }
    uint32_t mask = (1u << SID_EXPERT_DAC_KINK) | (1u << SID_EXPERT_DAC_LEAKAGE) |
                    (1u << SID_EXPERT_VOICE_DC);
    return ((a->expert_mask | b->expert_mask) & mask) &&
           (expert_differs(a, b, SID_EXPERT_DAC_KINK) ||
            expert_differs(a, b, SID_EXPERT_DAC_LEAKAGE) ||
            expert_differs(a, b, SID_EXPERT_VOICE_DC));
}

static void dac_copy(sid_character* to, const sid_character* from) {
    to->dac = from->dac;
    expert_copy(to, from, SID_EXPERT_DAC_KINK);
    expert_copy(to, from, SID_EXPERT_DAC_LEAKAGE);
    expert_copy(to, from, SID_EXPERT_VOICE_DC);
}

/* The bleed's tick: up to SID_CHARACTER_BLEED_RAMP_MAX detents where its curve is shallow,
 * one where it is not, and where one detent moves it more than SID_CHARACTER_BLEED_RAMP 127ths
 * half a detent a tick (bleed_half: the stage half way to the next detent, sid_character.h).
 * 1 if the stage's bleed changed. */
static int bleed_tick(sid_character_ramp* r) {
    sid_character* now = &r->now;
    uint8_t to = clamped(r->target.bleed);
    if (r->bleed_half) {
        /* The rest of the half detent's way, or back if the target turned. */
        int ahead = r->bleed_half > 0 ? to > now->bleed : to < now->bleed;
        if (ahead) {
            now->bleed = (uint8_t)(now->bleed + r->bleed_half);
        }
        r->bleed_half = 0;
        return 1;
    }
    if (now->bleed == to) {
        return 0;
    }
    uint32_t step = ramp_step(now->bleed, to, (float)SID_CHARACTER_BLEED_RAMP,
                              SID_CHARACTER_BLEED_RAMP_MAX);
    if (step) {
        now->bleed = toward(now->bleed, to, step);
    } else {
        r->bleed_half = (int8_t)(to > now->bleed ? 1 : -1);
    }
    return 1;
}

int sid_character_ramp_tick(sid_character_ramp* r, const sid* s, sid_dac* free_table,
                            sid_character_stage* out) {
    return sid_character_ramp_tick_tables(r, s, free_table, NULL, out);
}

static int filter_overridden(const sid_character* c, sid_model model) {
    return model == SID_MODEL_6581 ? overridden(c, SID_EXPERT_FILTER_6581_CURVE) ||
                                        overridden(c, SID_EXPERT_FILTER_6581_RANGE)
                                  : overridden(c, SID_EXPERT_FILTER_8580_CURVE);
}

static int filter_differs(const sid_character* a, const sid_character* b, sid_model model) {
    if (!filter_overridden(a, model) && !filter_overridden(b, model)) {
        return 0;
    }
    uint32_t first = model == SID_MODEL_6581 ? SID_EXPERT_FILTER_6581_CURVE : SID_EXPERT_FILTER_8580_CURVE;
    uint32_t end = model == SID_MODEL_6581 ? SID_EXPERT_FILTER_8580_CURVE : SID_EXPERT_NONLINEARITY;
    for (uint32_t i = first; i < end; ++i) {
        if (expert_differs(a, b, i) || sid_character_expert_value(a, model, i) !=
                                      sid_character_expert_value(b, model, i)) {
            return 1;
        }
    }
    return 0;
}

static void filter_copy(sid_character* to, const sid_character* from) {
    for (uint32_t i = 0; i < 3u; ++i) {
        expert_copy(to, from, i);
    }
}

static int filter_target_changed(const sid_character_ramp* r, sid_model model) {
    return filter_differs(&r->filter_now, &r->target, model) ||
           (r->filter_managed && !filter_overridden(&r->target, model) &&
            clamped(r->filter_now.age) != clamped(r->target.age));
}

static void filter_transition_start(sid_character_ramp* r, const sid* s,
                                     const sid_filter_row* rows, float source_t) {
    sid_character_filter_job_start(&r->filter_job, s, &r->target, rows);
    r->filter_job.transition = 1;
    r->filter_job.source_rows[0] = r->filter_rows[0];
    r->filter_job.source_rows[1] = r->filter_rows[1];
    r->filter_job.source_t = source_t;
    r->filter_t = source_t;
    r->filter_managed = 1;
    r->filter_building = 1;
}

static void stage_rows(const sid* s, const sid_character* c, const sid_filter_row* rows,
                        sid_character_stage* out) {
    out->filter_rows[0] = rows[0];
    out->filter_rows[1] = rows[1];
    out->filter_t = filter_overridden(c, s->model) ? 0.0f : amount(c->age);
    const sid_chip_profile* p = profile_of(s, c);
    out->filter_curve = filter_overridden(c, s->model) ? SID_EXPERT_DEFAULT : p->curve;
    out->filter_toward = filter_overridden(c, s->model) ? SID_EXPERT_DEFAULT : p->hot_curve;
    sid_nonlin_for_rows(s, sid_character_expert_value(c, s->model, SID_EXPERT_NONLINEARITY),
                        out->filter_t, rows, &out->nl);
}

int sid_character_ramp_tick_tables(sid_character_ramp* r, const sid* s, sid_dac* free_table,
                                   const sid_filter_row* free_rows, sid_character_stage* out) {
    sid_character* now = &r->now;
    const sid_character* want = &r->target;
    if (!(now->expert_mask | want->expert_mask) &&
        !(r->building | r->filter_building | r->filter_fading | r->filter_restore | r->filter_managed) &&
        now->age == clamped(want->age) && now->dac == clamped(want->dac) &&
        now->thump == clamped(want->thump) && now->bleed == clamped(want->bleed) &&
        !r->bleed_half) {
        return 0;
    }
    sid_character was = *now;
    if (!r->filter_rows_valid) {
        r->filter_rows[0] = s->filter_rows[0];
        r->filter_rows[1] = s->filter_rows[1];
        r->filter_t = s->filter_t;
        r->filter_rows_valid = 1;
        r->filter_managed = (uint8_t)filter_overridden(now, s->model);
    }
    now->age = toward(now->age, clamped(want->age), SID_CHARACTER_RAMP);
    now->thump = ramp_toward(now->thump, clamped(want->thump), (float)SID_CHARACTER_RAMP,
                             SID_CHARACTER_RAMP);
    int changed = now->age != was.age || now->thump != was.thump;
    changed |= bleed_tick(r);
    if (now->expert_mask | want->expert_mask) {
        if (s->model == SID_MODEL_6581) {
            expert_copy(now, want, SID_EXPERT_FILTER_8580_CURVE);
        } else {
            expert_copy(now, want, SID_EXPERT_FILTER_6581_CURVE);
            expert_copy(now, want, SID_EXPERT_FILTER_6581_RANGE);
        }
        for (uint32_t i = SID_EXPERT_NONLINEARITY; i < SID_EXPERT_COUNT; ++i) {
            if (i != SID_EXPERT_DAC_KINK && i != SID_EXPERT_DAC_LEAKAGE && i != SID_EXPERT_VOICE_DC) {
                changed |= expert_step(now, want, s->model, i);
            }
        }
    }
    int filter_ready = 0;
    int filter_work = r->filter_building != 0;
    if (r->filter_fading) {
        r->filter_fade++;
        r->filter_t = amount(r->filter_fade);
        changed = 1;
        if (r->filter_fade == SID_CHARACTER_MAX) {
            r->filter_fading = 0;
            r->filter_restore = (uint8_t)!filter_overridden(&r->filter_now, s->model);
        }
    } else if (r->filter_building) {
        if (sid_character_filter_job_step(&r->filter_job)) {
            if (!r->filter_job.transition && filter_target_changed(r, s->model)) {
                sid_filter_row rows[2] = {r->filter_job.rows[0], r->filter_job.rows[1]};
                filter_transition_start(r, s, rows, r->filter_t);
            } else {
                r->filter_building = 0;
                r->filter_now = r->filter_job.c;
                filter_copy(now, &r->filter_now);
                r->filter_rows[0] = r->filter_job.rows[0];
                r->filter_rows[1] = r->filter_job.rows[1];
                if (r->filter_job.transition) {
                    r->filter_fading = 1;
                    r->filter_fade = 0;
                    r->filter_t = 0.0f;
                    filter_ready = 1;
                } else {
                    r->filter_managed = 0;
                    r->filter_restore = 0;
                    filter_ready = 2;
                }
                changed = 1;
            }
        }
    } else if (!r->building && free_rows &&
               (r->filter_restore || filter_target_changed(r, s->model))) {
        sid_character next = *want;
        if (filter_target_changed(r, s->model)) {
            filter_transition_start(r, s, free_rows,
                                     r->filter_managed ? r->filter_t : amount(was.age));
        } else {
            sid_character_filter_job_start(&r->filter_job, s, &next, free_rows);
        }
        r->filter_building = 1;
        sid_character_filter_job_step(&r->filter_job);
        filter_work = 1;
    }
    if (r->building && !filter_work) {
        if (sid_dac_job_step(&r->job)) {
            r->building = 0;
            dac_copy(now, &r->dac_next);
            r->table = r->job.d;
            changed = 1;
        }
    } else if (!filter_work && dac_differs(now, want)) {
        sid_character next = *now;
        next.dac = ramp_toward(now->dac, clamped(want->dac), SID_CHARACTER_DAC_RAMP,
                               SID_CHARACTER_DAC_RAMP_MAX);
        expert_step(&next, want, s->model, SID_EXPERT_DAC_KINK);
        expert_step(&next, want, s->model, SID_EXPERT_DAC_LEAKAGE);
        expert_step(&next, want, s->model, SID_EXPERT_VOICE_DC);
        if (!sid_character_dac_custom(s, &next)) {
            dac_copy(now, &next);
            r->table = NULL;
            changed = 1;
        } else if (free_table) {
            sid_character_dac_job(s, &next, free_table, &r->job);
            r->next_dac = next.dac;
            r->dac_next = next;
            r->building = 1;
            sid_dac_job_step(&r->job); /* the first step now: the table in SID_DAC_BUILD_STEPS */
        }
    }
    if (changed) {
        sid_character_stage_for(s, now, r->table, out);
        if (r->bleed_half && !overridden(now, SID_EXPERT_BLEED)) {
            uint8_t next = (uint8_t)(now->bleed + r->bleed_half);
            out->bleed = 0.5f * (out->bleed + response(next) * SID_CHARACTER_BLEED);
        }
        stage_rows(s, now, r->filter_rows, out);
        if (r->filter_managed && filter_ready != 2) {
            out->filter_t = r->filter_t;
            out->filter_curve = out->filter_toward = SID_EXPERT_DEFAULT;
            sid_nonlin_for_rows(s, sid_character_expert_value(now, s->model,
                                                            SID_EXPERT_NONLINEARITY),
                                out->filter_t, out->filter_rows, &out->nl);
        }
    }
    return changed;
}

int sid_character_ramp_wants_table(const sid_character_ramp* r) {
    return !r->building && !r->filter_building && dac_differs(&r->now, &r->target);
}
