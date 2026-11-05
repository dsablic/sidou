// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_lfo.h"

#include <string.h>

#include "screen.h"

#define CENTI_HZ 10000u

static const char* const SHAPES[LAB_LFO_SHAPES] = {"LFO OFF", "TRIANGLE", "SAW UP",
                                                   "SAW DOWN", "SQUARE", "S&H"};
static const char DESTS[LAB_LFO_DESTS][4] = {"PCH", "PW", "CUT", "VOL"};

const char* lab_lfo_dest_code(uint32_t dest) {
    return DESTS[dest % LAB_LFO_DESTS];
}

uint32_t lab_lfo_shape_of(const live* l, uint32_t n) {
    return l->lfo[n % LAB_SOUND_LFOS][0] & LAB_LFO_SHAPE_MASK;
}

uint32_t lab_lfo_dest_of(const live* l, uint32_t n) {
    return (l->lfo[n % LAB_SOUND_LFOS][0] & LAB_LFO_DEST_MASK) >> LAB_LFO_DEST_SHIFT;
}

static void copy(char* out, const char* s) {
    while ((*out++ = *s++)) {
    }
}

static void rate_text(uint32_t step, char* out) {
    uint32_t c = (step * CENTI_HZ + 32768u) >> 16;
    uint32_t i = 0;
    if (c >= 100u) {
        uint32_t hz = (c + 50u) / 100u;
        if (hz < 10u) {
            out[i++] = (char)('0' + hz);
            out[i++] = '.';
            out[i++] = (char)('0' + (c % 100u + 5u) / 10u % 10u);
        } else {
            i += screen_decimal(out, hz);
        }
    } else {
        out[i++] = '.';
        out[i++] = (char)('0' + c / 10u);
        out[i++] = (char)('0' + c % 10u);
    }
    out[i] = 0;
}

int lab_lfo_card(const live* l, uint32_t k, char* name, char* value, uint32_t* fill, uint32_t* max) {
    static const char* const NAMES[4] = {"EDIT", "DEST", "RATE", "DEPTH"};
    const uint8_t* f = l->lfo[l->lfo_edit % LAB_SOUND_LFOS];
    copy(name, NAMES[k & 3u]);
    switch (k) {
    case 0u:
        value[0] = (char)('1' + l->lfo_edit);
        value[1] = 0;
        *fill = l->lfo_edit;
        *max = LAB_SOUND_LFOS - 1u;
        return 1;
    case 1u:
        copy(value, DESTS[lab_lfo_dest_of(l, l->lfo_edit)]);
        *fill = lab_lfo_dest_of(l, l->lfo_edit);
        *max = LAB_LFO_DESTS - 1u;
        break;
    case 2u:
        rate_text(live_walk_at(LIVE_WALK_PWM_RATE, f[1]), value);
        *fill = f[1];
        *max = LAB_LFO_RATES - 1u;
        break;
    default: {
        int32_t d = (int8_t)f[2];
        uint32_t i = 0;
        if (d) {
            value[i++] = d < 0 ? '-' : '+';
        }
        i += screen_decimal(value + i, (uint32_t)(d < 0 ? -d : d));
        value[i] = 0;
        *fill = (uint32_t)(d + LAB_LFO_DEPTH_MAX);
        *max = 2u * LAB_LFO_DEPTH_MAX;
        break;
    }
    }
    return lab_lfo_shape_of(l, l->lfo_edit) != LAB_LFO_OFF;
}

void lab_lfo_choice(const live* l, char* out) {
    copy(out, SHAPES[lab_lfo_shape_of(l, l->lfo_edit)]);
}

static int32_t clamp(int32_t v, int32_t lo, int32_t hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

static void put(live* l, uint32_t shape, uint32_t dest, int32_t rate, int32_t depth) {
    uint8_t f[LAB_SOUND_LFO_BYTES] = {(uint8_t)(shape | dest << LAB_LFO_DEST_SHIFT), (uint8_t)rate,
                                      (uint8_t)(int8_t)depth};
    live_lfo_set(l, l->lfo_edit, f);
}

void lab_lfo_turn(live* l, uint32_t k, int32_t d) {
    uint32_t n = l->lfo_edit;
    const uint8_t* f = l->lfo[n];
    uint32_t shape = lab_lfo_shape_of(l, n), dest = lab_lfo_dest_of(l, n);
    int32_t rate = f[1], depth = (int8_t)f[2];
    switch (k) {
    case 0u: l->lfo_edit = (uint8_t)clamp((int32_t)n + d, 0, LAB_SOUND_LFOS - 1); return;
    case 1u: dest = (uint32_t)clamp((int32_t)dest + d, 0, LAB_LFO_DESTS - 1); break;
    case 2u: rate = clamp(rate + d, 0, LAB_LFO_RATES - 1); break;
    default: depth = clamp(depth + d, -LAB_LFO_DEPTH_MAX, LAB_LFO_DEPTH_MAX); break;
    }
    put(l, shape, dest, rate, depth);
}

#define FIRST_RATE 20
#define FIRST_DEPTH 16

void lab_lfo_choice_turn(live* l, int32_t d) {
    uint32_t n = l->lfo_edit;
    const uint8_t* f = l->lfo[n];
    uint32_t was = lab_lfo_shape_of(l, n);
    uint32_t shape = (uint32_t)clamp((int32_t)was + d, 0, LAB_LFO_SHAPES - 1);
    int32_t rate = f[1], depth = (int8_t)f[2];
    if (!was && shape && !depth) {
        rate = FIRST_RATE;
        depth = FIRST_DEPTH;
    }
    put(l, shape, lab_lfo_dest_of(l, n), rate, depth);
}
