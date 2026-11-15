// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_character.h"

#include "gen/expert.h"

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
