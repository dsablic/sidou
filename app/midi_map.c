// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_map.h"

#include <stddef.h>

#include "fx/fx_slots.h"

#define CC_DATA_MSB 6u
#define CC_DATA_LSB 38u
#define CC_INCREMENT 96u
#define CC_DECREMENT 97u
#define CC_NRPN_LSB 98u
#define CC_NRPN_MSB 99u
#define CC_RPN_LSB 100u
#define CC_RPN_MSB 101u
#define CC_RESET_ALL 121u

#define LINEAR MIDI_CURVE_LINEAR
#define STEPS MIDI_CURVE_STEPS
#define NRPN MIDI_MAP_NRPN

/* NRPN MSBs: the groups. */
#define NRPN_LIVE 0u
#define NRPN_SID 1u
#define NRPN_CHARACTER 2u
#define NRPN_SLOT 3u /* + slot: Distortion 3, Modulation 4, FX 5; the LSB is the field */

#define ROW(g, n, v, p, c, nr, mx, d, cv)                                                          \
    {.group = g, .name = n, .values = v, .param = p, .cc = c, .nrpn = nr, .max = mx, .def = d,    \
     .curve = cv}

/* One field of an effect slot: parameter, CC and NRPN all follow from slot and field. */
#define SLOT_ROW(slot, g, field, n, v, mx, cv)                                                     \
    ROW(g, n, v, LIVE_PARAM_SLOT + (slot) * LIVE_SLOT_FIELDS + (field),                            \
        LIVE_CC_SLOT_FIRST + (slot) * LIVE_SLOT_FIELDS + (field), NRPN(NRPN_SLOT + (slot), field), \
        mx, 0, cv)

#define SLOT_ROWS(slot, g, algorithms, count)                                                      \
    SLOT_ROW(slot, g, 0, "Algorithm", algorithms, (count) - 1, STEPS),                             \
        SLOT_ROW(slot, g, 1, "Amount", "0..127", LIVE_SLOT_MAX, LINEAR),                           \
        SLOT_ROW(slot, g, 2, "A", "0..127", LIVE_SLOT_MAX, LINEAR),                                \
        SLOT_ROW(slot, g, 3, "B", "0..127", LIVE_SLOT_MAX, LINEAR),                                \
        SLOT_ROW(slot, g, 4, "Mix", "0 dry .. 127 wet", LIVE_SLOT_MAX, LINEAR)

/* clang-format off */
static const midi_map_entry rows[MIDI_MAP_ROWS] = {
    ROW("Live", "Cutoff", "0..2047, the SID's 11 bits", LIVE_PARAM_CUTOFF,
        LIVE_CC_CUTOFF, NRPN(NRPN_LIVE, 0), LIVE_CUTOFF_MAX, 1200, LINEAR),
    ROW("Live", "Resonance", "0..15", LIVE_PARAM_RESONANCE,
        LIVE_CC_RESONANCE, NRPN(NRPN_LIVE, 1), LIVE_RESONANCE_MAX, 6, STEPS),
    ROW("Live", "Pulse width", "0..4095, the SID's 12 bits; the PWM presets move around it",
        LIVE_PARAM_PULSE_WIDTH, LIVE_CC_PULSE_WIDTH, NRPN(NRPN_LIVE, 2), LIVE_PULSE_WIDTH_MAX,
        2048, LINEAR),
    ROW("Live", "Volume", "0..1023 as the MASTER pot; the chip's volume is its top 4 bits",
        LIVE_PARAM_VOLUME, LIVE_CC_VOLUME, NRPN(NRPN_LIVE, 3), LIVE_MASTER_MAX,
        LIVE_MASTER_MAX, LINEAR),

    ROW("SID", "Waveform", "sum of 1 triangle, 2 saw, 4 pulse, 8 noise; 0 silent",
        LIVE_PARAM_WAVEFORM, 20, NRPN(NRPN_SID, 0), 15, 2, STEPS),
    ROW("SID", "Ring modulation", "0 off, 1 on (by the voice before)", LIVE_PARAM_RING,
        21, NRPN(NRPN_SID, 1), 1, 0, STEPS),
    ROW("SID", "Sync", "0 off, 1 on (to the voice before)", LIVE_PARAM_SYNC,
        22, NRPN(NRPN_SID, 2), 1, 0, STEPS),
    ROW("SID", "Filter mode", "sum of 1 low-pass, 2 band-pass, 4 high-pass",
        LIVE_PARAM_FILTER_MODE, 23, NRPN(NRPN_SID, 3), 7, 1, STEPS),
    ROW("SID", "Filter routing", "sum of 1 voice 1, 2 voice 2, 4 voice 3",
        LIVE_PARAM_FILTER_VOICES, 24, NRPN(NRPN_SID, 4), 7, 7, STEPS),
    ROW("SID", "Chip model", "0 6581, 1 8580", LIVE_PARAM_MODEL,
        25, NRPN(NRPN_SID, 5), 1, 0, STEPS),
    ROW("SID", "Attack", "0..15, the SID's rates", LIVE_PARAM_ATTACK,
        73, NRPN(NRPN_SID, 6), 15, 0, STEPS),
    ROW("SID", "Decay", "0..15, the SID's rates", LIVE_PARAM_DECAY,
        75, NRPN(NRPN_SID, 7), 15, 9, STEPS),
    ROW("SID", "Sustain", "0..15, the level", LIVE_PARAM_SUSTAIN,
        79, NRPN(NRPN_SID, 8), 15, 10, STEPS),
    ROW("SID", "Release", "0..15, the SID's rates", LIVE_PARAM_RELEASE,
        72, NRPN(NRPN_SID, 9), 15, 6, STEPS),

    ROW("Character", "Profile", "6581 R2, 6581 R3, 6581 R4AR, 8580 R5, worn 6581",
        MIDI_MAP_RESERVED, 85, NRPN(NRPN_CHARACTER, 0), 4, 0, STEPS),
    ROW("Character", "Age/heat", "0..127", MIDI_MAP_RESERVED,
        86, NRPN(NRPN_CHARACTER, 1), 127, 0, LINEAR),
    ROW("Character", "DAC sloppiness", "0..127", MIDI_MAP_RESERVED,
        87, NRPN(NRPN_CHARACTER, 2), 127, 0, LINEAR),
    ROW("Character", "Output thump", "0..127", MIDI_MAP_RESERVED,
        89, NRPN(NRPN_CHARACTER, 3), 127, 0, LINEAR),
    ROW("Character", "Filter bleed", "0..127", MIDI_MAP_RESERVED,
        90, NRPN(NRPN_CHARACTER, 4), 127, 0, LINEAR),

    SLOT_ROWS(0, "Distortion", "0 OFF, 1 TAPE, 2 OVERDRIVE, 3 FUZZ, 4 CRUSH, 5 LO-FI",
              SID_DIST_COUNT),
    SLOT_ROWS(1, "Modulation",
              "0 OFF, 1 CHORUS, 2 FLANGER, 3 PHASER, 4 TREMOLO, 5 AUTO-PAN, 6 WOW",
              SID_MOD_COUNT),
    SLOT_ROWS(2, "FX", "0 OFF, 1 DELAY, 2 PING-PONG, 3 REVERB, 4 LUSH", SID_FXSLOT_COUNT),
};
/* clang-format on */

_Static_assert(SID_DIST_COUNT == 6 && SID_MOD_COUNT == 7 && SID_FXSLOT_COUNT == 5,
               "the algorithm lists above name every algorithm");

const midi_map_entry* midi_map_row(uint32_t i) {
    return i < MIDI_MAP_ROWS ? &rows[i] : NULL;
}

const midi_map_entry* midi_map_for_param(uint32_t param) {
    for (uint32_t i = 0; i < MIDI_MAP_ROWS; ++i) {
        if (rows[i].param == param && param != MIDI_MAP_RESERVED) {
            return &rows[i];
        }
    }
    return NULL;
}

const midi_map_entry* midi_map_for_cc(uint8_t cc) {
    for (uint32_t i = 0; i < MIDI_MAP_ROWS; ++i) {
        if (rows[i].cc == cc && cc != MIDI_MAP_NO_CC) {
            return &rows[i];
        }
    }
    return NULL;
}

const midi_map_entry* midi_map_for_nrpn(uint16_t nrpn) {
    for (uint32_t i = 0; i < MIDI_MAP_ROWS; ++i) {
        if (rows[i].nrpn == nrpn && nrpn != MIDI_MAP_NO_NRPN) {
            return &rows[i];
        }
    }
    return NULL;
}

uint32_t midi_map_from_cc(const midi_map_entry* e, uint8_t value) {
    uint32_t v = value > 127u ? 127u : value;
    if (e->curve == MIDI_CURVE_STEPS) {
        return v * (e->max + 1u) / 128u;
    }
    return (v * e->max + 63u) / 127u;
}

uint8_t midi_map_to_cc(const midi_map_entry* e, uint32_t value) {
    if (e->max == 0) {
        return 0;
    }
    if (value > e->max) {
        value = e->max;
    }
    return (uint8_t)((value * 127u + e->max / 2u) / e->max);
}

static void deselect(midi_map_channel* c) {
    c->nrpn_msb = MIDI_MAP_NULL;
    c->nrpn_lsb = MIDI_MAP_NULL;
    c->rpn_msb = MIDI_MAP_NULL;
    c->rpn_lsb = MIDI_MAP_NULL;
    c->rpn = 0;
    c->data_msb = MIDI_MAP_NO_DATA;
}

void midi_map_port_init(midi_map_port* p, live* l) {
    p->l = l;
    for (uint32_t i = 0; i < 16u; ++i) {
        deselect(&p->ch[i]);
    }
}

/* The parameter data entry goes to: none for an RPN (none is implemented yet), the null
 * NRPN, an unmapped or a reserved one. */
static const midi_map_entry* selected(const midi_map_channel* c) {
    if (c->rpn) {
        return NULL;
    }
    const midi_map_entry* e = midi_map_for_nrpn(MIDI_MAP_NRPN(c->nrpn_msb, c->nrpn_lsb));
    return e && e->param != MIDI_MAP_RESERVED ? e : NULL;
}

static void data(midi_map_port* p, midi_map_channel* c, uint8_t cc, uint8_t value) {
    const midi_map_entry* e = selected(c);
    if (!e) {
        return;
    }
    uint32_t now = live_param_get(p->l, e->param);
    uint32_t v;
    switch (cc) {
    case CC_DATA_MSB:
        c->data_msb = value;
        v = (uint32_t)value << 7;
        break;
    case CC_DATA_LSB: {
        uint32_t msb = c->data_msb != MIDI_MAP_NO_DATA ? c->data_msb : now >> 7;
        v = msb << 7 | value;
        break;
    }
    case CC_INCREMENT:
        c->data_msb = MIDI_MAP_NO_DATA;
        v = now + 1u;
        break;
    default: /* CC_DECREMENT */
        c->data_msb = MIDI_MAP_NO_DATA;
        v = now ? now - 1u : 0u;
        break;
    }
    live_param_set(p->l, e->param, v > e->max ? e->max : v);
}

void midi_map_cc(midi_map_port* p, uint8_t channel, uint8_t cc, uint8_t value) {
    if (channel > 15u || value > 127u) {
        return;
    }
    midi_map_channel* c = &p->ch[channel];
    switch (cc) {
    case CC_NRPN_MSB:
    case CC_NRPN_LSB:
        if (cc == CC_NRPN_MSB) {
            c->nrpn_msb = value;
        } else {
            c->nrpn_lsb = value;
        }
        c->rpn = 0;
        c->data_msb = MIDI_MAP_NO_DATA;
        break;
    case CC_RPN_MSB:
    case CC_RPN_LSB:
        if (cc == CC_RPN_MSB) {
            c->rpn_msb = value;
        } else {
            c->rpn_lsb = value;
        }
        c->rpn = 1;
        c->data_msb = MIDI_MAP_NO_DATA;
        break;
    case CC_DATA_MSB:
    case CC_DATA_LSB:
    case CC_INCREMENT:
    case CC_DECREMENT: data(p, c, cc, value); break;
    case CC_RESET_ALL: deselect(c); break;
    default: live_cc(p->l, cc, value); break;
    }
}
