// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"

#include "fx/fx_slots.h"

static const uint16_t PWM_RATE[32] = {33,   39,   47,   56,   67,   80,   96,   115,
                                      137,  164,  196,  234,  280,  335,  400,  479,
                                      572,  684,  818,  978,  1170, 1399, 1672, 2000,
                                      2391, 2859, 3419, 4088, 4888, 5844, 6988, 8356};
static const uint16_t PWM_DEPTH[32] = {0,    2,    9,    19,   34,   53,   77,   104,
                                       136,  173,  213,  258,  307,  360,  418,  480,
                                       546,  616,  690,  769,  852,  940,  1031, 1127,
                                       1228, 1332, 1441, 1554, 1671, 1792, 1918, 2048};
static const uint8_t VIB_RATE[24] = {5,  6,  7,  8,  9,  10, 11, 13, 15, 17, 20,  23,
                                     27, 31, 36, 41, 47, 54, 62, 71, 82, 95, 110, 127};
static const uint8_t VIB_DEPTH[32] = {0,  6,  7,  8,  9,  10, 11, 12, 13, 14, 15,
                                      17, 19, 21, 23, 25, 28, 31, 34, 38, 42, 46,
                                      51, 56, 62, 69, 77, 85, 94, 104, 115, 127};
static const uint8_t VIB_DELAY[24] = {0,  4,  5,  6,  7,  8,  9,  10, 12, 14, 16,  19,
                                      22, 26, 30, 35, 41, 48, 56, 66, 78, 92, 108, 127};
const uint8_t live_walk_fx[3][32] = {
    {0,  4,  8,  12, 16, 20, 24, 28, 32, 36,  40,  45,  49,  53,  57,  61,
     66, 70, 74, 78, 82, 87, 91, 95, 99, 103, 107, 111, 115, 119, 123, 127},
    {0,  5,  6,  7,  8,  9,  10, 11, 12, 13, 14, 16, 18,  20,  22,  24,
     27, 30, 33, 37, 41, 45, 50, 55, 61, 68, 76, 84, 93, 103, 114, 127},
    {0,  6,  13, 19, 26,  32,  38,  44,  50,  56,  62,  67,  73,  78,  83,  87,
     92, 96, 100, 104, 108, 111, 114, 117, 119, 121, 122, 123, 124, 125, 126, 127},
};

typedef struct {
    const uint8_t* b;
    const uint16_t* w;
    uint8_t n;
    uint16_t pos, cc;
} walk_table;

#define N32 32, 1000, 1049
#define N24 24, 742, 1408
static const walk_table WALKS[LIVE_WALKS] = {
    [LIVE_WALK_PWM_RATE] = {0, PWM_RATE, N32},       [LIVE_WALK_PWM_DEPTH] = {0, PWM_DEPTH, N32},
    [LIVE_WALK_VIB_RATE] = {VIB_RATE, 0, N24},       [LIVE_WALK_VIB_DEPTH] = {VIB_DEPTH, 0, N32},
    [LIVE_WALK_VIB_DELAY] = {VIB_DELAY, 0, N24},     [LIVE_WALK_FX_LIN] = {live_walk_fx[0], 0, N32},
    [LIVE_WALK_FX_LOG] = {live_walk_fx[1], 0, N32},  [LIVE_WALK_FX_EQP] = {live_walk_fx[2], 0, N32},
};

static uint32_t at(const walk_table* k, uint32_t i) {
    return k->w ? k->w[i] : k->b[i];
}

static int32_t place(const walk_table* k, uint32_t value) {
    int32_t lo = -1, hi = k->n;
    while (hi - lo > 1) {
        int32_t mid = (lo + hi) >> 1;
        if (at(k, (uint32_t)mid) <= value) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    return lo;
}

#define SHAPES(amount, a, b, mix) ((amount) | (a) << 2 | (b) << 4 | (mix) << 6)
#define LIN 0u
#define LOG 1u
#define EQP 2u
const uint8_t live_walk_shapes[LIVE_SLOTS][8] = {
    [0] = {
        [SID_DIST_NONE] = SHAPES(LIN, LIN, LIN, LOG),
        [SID_DIST_TAPE] = SHAPES(LIN, LIN, LIN, LOG),
        [SID_DIST_OVERDRIVE] = SHAPES(LIN, LIN, LOG, LOG),
        [SID_DIST_FUZZ] = SHAPES(LIN, LOG, LOG, LOG),
        [SID_DIST_CRUSH] = SHAPES(LIN, EQP, LIN, LOG),
        [SID_DIST_LOFI] = SHAPES(LIN, LOG, LOG, LOG),
    },
    [1] = {
        [SID_MOD_NONE] = SHAPES(LIN, LIN, LIN, LOG),
        [SID_MOD_CHORUS] = SHAPES(LOG, LIN, LOG, LOG),
        [SID_MOD_FLANGER] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_MOD_PHASER] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_MOD_TREMOLO] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_MOD_AUTOPAN] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_MOD_WOW] = SHAPES(LOG, LOG, LIN, LOG),
    },
    [2] = {
        [SID_FXSLOT_NONE] = SHAPES(LIN, LIN, LIN, LOG),
        [SID_FXSLOT_DELAY] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_FXSLOT_PINGPONG] = SHAPES(LOG, LIN, LIN, LOG),
        [SID_FXSLOT_REVERB_LIGHT] = SHAPES(LIN, LIN, LIN, LOG),
        [SID_FXSLOT_REVERB_LUSH] = SHAPES(LIN, LIN, LIN, LOG),
    },
};
_Static_assert(SID_DIST_COUNT <= 8 && SID_MOD_COUNT <= 8 && SID_FXSLOT_COUNT <= 8,
               "a row of shapes a slot");
_Static_assert(LIVE_WALK_FX_LOG == LIVE_WALK_FX_LIN + LOG &&
                   LIVE_WALK_FX_EQP == LIVE_WALK_FX_LIN + EQP,
               "the shapes' walks in order");

uint32_t live_walk_count(uint32_t walk) {
    return walk < LIVE_WALKS ? WALKS[walk].n : 0u;
}

uint32_t live_walk_at(uint32_t walk, uint32_t i) {
    return walk < LIVE_WALKS && i < WALKS[walk].n ? at(&WALKS[walk], i) : 0u;
}

uint32_t live_walk_slot(uint32_t slot, uint32_t algorithm, uint32_t field) {
    if (slot >= LIVE_SLOTS || algorithm >= 8u || field < LIVE_SLOT_AMOUNT ||
        field > LIVE_SLOT_MIX) {
        return LIVE_WALK_FX_LIN;
    }
    uint32_t shift = 2u * (field - LIVE_SLOT_AMOUNT);
    return LIVE_WALK_FX_LIN + (live_walk_shapes[slot][algorithm] >> shift & 3u);
}

uint32_t live_walk_of(const live* l, uint32_t param) {
    switch (param) {
    case LIVE_PARAM_PWM_RATE: return LIVE_WALK_PWM_RATE;
    case LIVE_PARAM_PWM_DEPTH: return LIVE_WALK_PWM_DEPTH;
    case LIVE_PARAM_VIBRATO_RATE: return LIVE_WALK_VIB_RATE;
    case LIVE_PARAM_VIBRATO_DEPTH: return LIVE_WALK_VIB_DEPTH;
    case LIVE_PARAM_VIBRATO_DELAY: return LIVE_WALK_VIB_DELAY;
    default: break;
    }
    if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t field = param - LIVE_PARAM_SLOT, slot = 0;
        while (field >= LIVE_SLOT_FIELDS) {
            field -= LIVE_SLOT_FIELDS;
            slot++;
        }
        if (field != LIVE_SLOT_ALGORITHM) {
            return live_walk_slot(slot, l ? l->slot[slot][LIVE_SLOT_ALGORITHM] : 0u, field);
        }
    }
    return LIVE_WALK_NONE;
}

uint32_t live_walk_step(uint32_t walk, uint32_t value, int32_t detents) {
    if (walk >= LIVE_WALKS || detents == 0) {
        return value;
    }
    const walk_table* k = &WALKS[walk];
    int32_t n = k->n;
    int32_t pos = place(k, value);
    if (pos < 0 || at(k, (uint32_t)pos) != value) {
        if (detents > 0 ? pos + 1 >= n : pos < 0) {
            return value;
        }
        pos += detents > 0 ? 0 : 1;
    }
    pos += detents;
    pos = pos < 0 ? 0 : pos >= n ? n - 1 : pos;
    return at(k, (uint32_t)pos);
}

static uint32_t walked(const live* l, uint32_t param) {
    return param == LIVE_PARAM_PWM_RATE ? l->pwm_rate : live_param_get(l, param);
}

static void walk_set(live* l, uint32_t param, uint32_t value) {
    if (param == LIVE_PARAM_PWM_RATE) {
        l->settings_dirty = 1;
        l->pwm_rate = (uint16_t)value;
    } else {
        live_param_set(l, param, value);
    }
}

void live_walk_turn(live* l, uint32_t param, int32_t detents) {
    walk_set(l, param, live_walk_step(live_walk_of(l, param), walked(l, param), detents));
}

static uint32_t cc_position(uint32_t walk, uint32_t cc) {
    cc = cc > 127u ? 127u : cc;
    return walk < LIVE_WALKS ? (cc * WALKS[walk].pos + 2048u) >> 12 : 0u;
}

uint32_t live_walk_from_cc(const live* l, uint32_t param, uint32_t cc) {
    uint32_t walk = live_walk_of(l, param);
    uint32_t v = live_walk_at(walk, cc_position(walk, cc));
    return param == LIVE_PARAM_PWM_RATE ? live_pwm_rate(v) : v;
}

uint8_t live_walk_to_cc(const live* l, uint32_t param, uint32_t value) {
    uint32_t walk = live_walk_of(l, param);
    if (walk >= LIVE_WALKS) {
        return 0;
    }
    const walk_table* k = &WALKS[walk];
    if (param == LIVE_PARAM_PWM_RATE) {
        value = live_pwm_step(value);
    }
    int32_t i = place(k, value);
    if (i < 0 || (i + 1 < k->n && at(k, (uint32_t)i + 1u) - value < value - at(k, (uint32_t)i))) {
        i++;
    }
    return (uint8_t)(((uint32_t)i * k->cc + 128u) >> 8);
}

void live_walk_cc(live* l, uint32_t param, uint32_t cc) {
    uint32_t walk = live_walk_of(l, param);
    walk_set(l, param, live_walk_at(walk, cc_position(walk, cc)));
}
