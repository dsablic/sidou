// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_sound.h"

#include "live.h"

#include <string.h>

_Static_assert(LAB_SOUND_SLOTS + LIVE_SLOTS * LIVE_SLOT_FIELDS == LAB_SOUND_BYTES_1_0,
               "the slots are 1.0's last bytes");
_Static_assert(LAB_SOUND_PWM_LINK + 1u == LAB_SOUND_BYTES_1_5, "1.1's after them");
_Static_assert(LAB_SOUND_LAYER_TUNING + LIVE_VOICES * LAB_SOUND_LAYER_TUNING_BYTES ==
                   LAB_SOUND_LAYER_VOICE,
               "each layer's tuning, then the voices of layers 2 and 3");
_Static_assert(LAB_SOUND_LAYER_VOICE + LIVE_LAYERS_MAX * LAB_SOUND_LAYER_VOICE_BYTES ==
                   LAB_SOUND_BYTES,
               "1.6's layers are the last bytes");
_Static_assert(LAB_SOUND_ARP + LIVE_ARP_SPAN + 1u == LAB_SOUND_SLOTS,
               "the arpeggio's four fields, then the slots");

static uint32_t u16_at(const uint8_t* s, uint32_t at) {
    return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
}

uint32_t lab_sound_equal(const uint8_t a[LAB_SOUND_BYTES], const uint8_t b[LAB_SOUND_BYTES]) {
    return memcmp(a, b, LAB_SOUND_BYTES) == 0;
}

static const struct {
    uint8_t at, max;
} BYTE_MAX[] = {
    {LAB_SOUND_RESONANCE, LIVE_RESONANCE_MAX},
    {LAB_SOUND_MONO_SYNC, 1u},
    {LAB_SOUND_BEND_RANGE, LIVE_BEND_RANGE_MAX},
    {LAB_SOUND_VELOCITY, LIVE_VELOCITY_PULSE_WIDTH},
    {LAB_SOUND_MOD, LIVE_MOD_CUTOFF},
    {LAB_SOUND_PRESSURE, LIVE_PRESSURE_CUTOFF},
    {LAB_SOUND_VIBRATO_RATE, LIVE_EXPRESSION_MAX},
    {LAB_SOUND_VIBRATO_DEPTH, LIVE_EXPRESSION_MAX},
    {LAB_SOUND_VIBRATO_DELAY, LIVE_EXPRESSION_MAX},
    {LAB_SOUND_ARP + LIVE_ARP_ON, 1u},
    {LAB_SOUND_ARP + LIVE_ARP_RATE, LIVE_ARP_RATES - 1u},
    {LAB_SOUND_ARP + LIVE_ARP_ORDER, LIVE_ARP_PLAYED},
    {LAB_SOUND_ARP + LIVE_ARP_SPAN, LIVE_ARP_SPAN_MAX},
    {LAB_SOUND_FILTER_LFO, LIVE_FILTER_LFO_MAX},
};

static int link_ok(uint8_t b) {
    return (uint8_t)(b + LIVE_LINK_MAX) <= 2u * LIVE_LINK_MAX;
}

static int within(uint8_t b, int32_t max) {
    int32_t v = (int8_t)b;
    return v >= -max && v <= max;
}

static int layers_ok(const uint8_t s[LAB_SOUND_BYTES]) {
    uint32_t layers = s[LAB_SOUND_LAYERS];
    if (layers > LIVE_LAYERS_MAX || (layers && s[LAB_SOUND_MONO_SYNC])) {
        return 0;
    }
    for (uint32_t n = 0; n < LIVE_VOICES; n++) {
        const uint8_t* t = &s[LAB_SOUND_LAYER_TUNING + n * LAB_SOUND_LAYER_TUNING_BYTES];
        if (!within(t[0], LIVE_TRANSPOSE_MAX) || !within(t[1], LIVE_DETUNE_CENTS)) {
            return 0;
        }
    }
    for (uint32_t n = 0; n < LIVE_LAYERS_MAX; n++) {
        const uint8_t* v = &s[LAB_SOUND_LAYER_VOICE + n * LAB_SOUND_LAYER_VOICE_BYTES];
        if ((v[0] & 0x09u) || u16_at(v, 3u) > LIVE_PULSE_WIDTH_MAX) {
            return 0;
        }
    }
    return 1;
}

LIVE_COLD uint32_t lab_sound_valid(const uint8_t s[LAB_SOUND_BYTES]) {
    for (uint32_t i = 0; i < sizeof BYTE_MAX / sizeof BYTE_MAX[0]; i++) {
        if (s[BYTE_MAX[i].at] > BYTE_MAX[i].max) {
            return 0;
        }
    }
    int32_t semis = (int8_t)s[LAB_SOUND_SYNC_SEMIS];
    int32_t tune = (int8_t)s[LAB_SOUND_TUNE];
    if ((s[LAB_SOUND_CONTROL] & 0x09u) || (s[LAB_SOUND_FILTER_VOICES] & ~0x07u) ||
        (s[LAB_SOUND_FILTER_MODE] & ~0x70u) || u16_at(s, LAB_SOUND_PULSE) > LIVE_PULSE_WIDTH_MAX ||
        u16_at(s, LAB_SOUND_CUTOFF) > LIVE_CUTOFF_MAX ||
        u16_at(s, LAB_SOUND_SYNC_BASE) > LAB_SOUND_SYNC_BASE_MAX || semis < -LIVE_RATIO_SEMIS ||
        semis > LIVE_RATIO_SEMIS || u16_at(s, LAB_SOUND_PWM_DEPTH) > LIVE_PWM_DEPTH_MAX ||
        u16_at(s, LAB_SOUND_PWM_RATE) > LIVE_PWM_STEP_MAX || tune < -LIVE_TUNE_CENTS ||
        tune > LIVE_TUNE_CENTS || s[LAB_SOUND_ARP + LIVE_ARP_SPAN] < 1u ||
        !link_ok(s[LAB_SOUND_RESO_LINK]) || !link_ok(s[LAB_SOUND_PWM_LINK]) || !layers_ok(s)) {
        return 0;
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        const uint8_t* f = &s[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS];
        if (f[LIVE_SLOT_ALGORITHM] >= live_slot_algorithms(k)) {
            return 0;
        }
        for (uint32_t c = LIVE_SLOT_AMOUNT; c < LIVE_SLOT_FIELDS; c++) {
            if (f[c] > LIVE_SLOT_MAX) {
                return 0;
            }
        }
    }
    return 1;
}
