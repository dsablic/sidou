// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_map.h"

#include <stddef.h>

#include "chip_spread.h"
#include "fx/fx_slots.h"
#include "midi_learn.h"

#define CC_BANK_MSB 0u
#define CC_BANK_LSB 32u
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
#define NRPN_EXPRESSION 6u
#define NO_CC MIDI_MAP_NO_CC

/* RPN 0/0: pitch-bend sensitivity, CC 6 the semitones and CC 38 the cents. */
#define RPN_BEND_RANGE 0u

#ifdef SIDOU_MIDI_MAP_DOC
#define DOC(g, n, v) .group = g, .name = n, .values = v,
#else
#define DOC(g, n, v) /* the device keeps no text (midi_map.h) */
#endif

#define ROW(g, n, v, p, c, nr, mx, d, cv)                                                          \
    {DOC(g, n, v).param = p, .cc = c, .nrpn = nr, .max = mx, .def = d, .curve = cv}

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

#define EXPERT_ROW(field, name, values, def) \
    ROW("Expert", name, values, LIVE_PARAM_EXPERT + (field), NO_CC, \
        NRPN(NRPN_CHARACTER, 5u + (field)), SID_CHARACTER_MAX, def, LINEAR)

/* clang-format off */
const midi_map_entry midi_map_rows[MIDI_MAP_ROWS] = {
    ROW("Live", "Cutoff", "0..2047, the SID's 11 bits", LIVE_PARAM_CUTOFF,
        LIVE_CC_CUTOFF, NRPN(NRPN_LIVE, 0), LIVE_CUTOFF_MAX, 1200, LINEAR),
    ROW("Live", "Resonance", "0..15", LIVE_PARAM_RESONANCE,
        LIVE_CC_RESONANCE, NRPN(NRPN_LIVE, 1), LIVE_RESONANCE_MAX, 6, STEPS),
    ROW("Live", "Pulse width",
        "0..4095, the SID's 12 bits; the PWM presets move around it; CC 1 while it is the mod "
        "wheel's target (the default)",
        LIVE_PARAM_PULSE_WIDTH, LIVE_CC_PULSE_WIDTH, NRPN(NRPN_LIVE, 2), LIVE_PULSE_WIDTH_MAX,
        2048, LINEAR),
    ROW("Live", "Volume", "0..1023 as the MASTER pot, the final output's digital gain",
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
    ROW("SID", "Envelope mode",
        "0 instant (the rate counter restarts at each gate edge), 1 authentic (the chip's "
        "ADSR delay: an attack or release up to 33 ms late after a slower rate)",
        LIVE_PARAM_ENVELOPE_MODE, 26, NRPN(NRPN_SID, 10), 1, LIVE_ENVELOPE_INSTANT, STEPS),

    ROW("Character", "Profile",
        "0 6581 R2, 1 6581 R3, 2 6581 R4AR (reSIDfp's default), 3 8580 R5, 4 worn 6581 "
        "(artistic), 5..12 USER 1..8 (an empty slot does nothing: it is skipped); sets the "
        "chip model too (a chip reset)",
        LIVE_PARAM_PROFILE, 85, NRPN(NRPN_CHARACTER, 0), SID_CHIPS - 1 + LIVE_USER_PROFILES,
        SID_CHIP_6581_R4AR, STEPS),
    ROW("Character", "Age/heat",
        "0 the profile .. 127: the filter's cutoff drifts darker, the 6581's non-linearity "
        "doubles", LIVE_PARAM_AGE, 86, NRPN(NRPN_CHARACTER, 1), SID_CHARACTER_MAX, 0, LINEAR),
    ROW("Character", "DAC sloppiness",
        "0 the profile .. 127: a worn chip's grit on every sound (the output through a worn "
        "converter: a noise riding on the signal and a bowed curve), and the voice DACs get bit "
        "errors, leak up to 6 times as much and kink more",
        LIVE_PARAM_DAC, 87, NRPN(NRPN_CHARACTER, 2), SID_CHARACTER_MAX, 0, LINEAR),
    ROW("Character", "Output thump",
        "0 none .. 127: a thump at each note's start and at a volume change (the 6581's voice "
        "DC and operating point)",
        LIVE_PARAM_THUMP, 89, NRPN(NRPN_CHARACTER, 3), SID_CHARACTER_MAX, 0, LINEAR),
    ROW("Character", "Filter bleed",
        "0 none .. 127: half the voices routed into the filter also leak past it, unfiltered",
        LIVE_PARAM_BLEED, 90, NRPN(NRPN_CHARACTER, 4), SID_CHARACTER_MAX, 0, LINEAR),

    SLOT_ROWS(0, "Distortion", "0 OFF, 1 TAPE, 2 OVERDRIVE, 3 FUZZ, 4 CRUSH, 5 LO-FI",
              SID_DIST_COUNT),
    SLOT_ROWS(1, "Modulation",
              "0 OFF, 1 CHORUS, 2 FLANGER, 3 PHASER, 4 TREMOLO, 5 AUTO-PAN, 6 WOW",
              SID_MOD_COUNT),
    SLOT_ROWS(2, "FX", "0 OFF, 1 DELAY, 2 PING-PONG, 3 REVERB, 4 LUSH", SID_FXSLOT_COUNT),

    ROW("Expression", "Bend range",
        "semitones each way; also RPN 0 (CC 101/100 = 0/0, then CC 6 the semitones)",
        LIVE_PARAM_BEND_RANGE, NO_CC, NRPN(NRPN_EXPRESSION, 0), LIVE_BEND_RANGE_MAX,
        LIVE_BEND_RANGE_DEFAULT, STEPS),
    ROW("Expression", "Velocity target",
        "0 off (full level), 1 sustain level, 2 cutoff (lower), 3 pulse width (narrower)",
        LIVE_PARAM_VELOCITY_TARGET, NO_CC, NRPN(NRPN_EXPRESSION, 1), LIVE_VELOCITY_PULSE_WIDTH,
        LIVE_VELOCITY_OFF, STEPS),
    ROW("Expression", "Mod wheel target",
        "0 pulse width (CC 1 sets it), 1 vibrato depth (adds), 2 cutoff (opens)",
        LIVE_PARAM_MOD_TARGET, NO_CC, NRPN(NRPN_EXPRESSION, 2), LIVE_MOD_CUTOFF,
        LIVE_MOD_PULSE_WIDTH, STEPS),
    ROW("Expression", "Aftertouch target",
        "0 off, 1 vibrato depth (adds), 2 pulse width (raises), 3 cutoff (opens)",
        LIVE_PARAM_PRESSURE_TARGET, NO_CC, NRPN(NRPN_EXPRESSION, 3), LIVE_PRESSURE_CUTOFF,
        LIVE_PRESSURE_OFF, STEPS),
    ROW("Expression", "Vibrato rate", "0..12.7 Hz in 0.1 Hz steps", LIVE_PARAM_VIBRATO_RATE,
        LIVE_CC_VIBRATO_RATE, NRPN(NRPN_EXPRESSION, 4), LIVE_EXPRESSION_MAX,
        LIVE_VIBRATO_RATE_DEFAULT, LINEAR),
    ROW("Expression", "Vibrato depth", "0 off .. 127 +-99 cents, a triangle",
        LIVE_PARAM_VIBRATO_DEPTH, LIVE_CC_VIBRATO_DEPTH, NRPN(NRPN_EXPRESSION, 5),
        LIVE_EXPRESSION_MAX, 0, LINEAR),
    ROW("Expression", "Vibrato delay", "0..1.27 s after each note-on, in 10 ms",
        LIVE_PARAM_VIBRATO_DELAY, LIVE_CC_VIBRATO_DELAY, NRPN(NRPN_EXPRESSION, 6),
        LIVE_EXPRESSION_MAX, 0, LINEAR),

    EXPERT_ROW(SID_EXPERT_FILTER_6581_CURVE, "6581 filter curve", "0..127 = reSIDfp curve 0..1", 64),
    EXPERT_ROW(SID_EXPERT_FILTER_6581_RANGE, "6581 filter range", "0..127 = reSIDfp range 0..1",
        (uint16_t)((19.0f / 39.0f - SID_EXPERT_RANGE_6581_MIN) *
                   127.0f / (1.0f - SID_EXPERT_RANGE_6581_MIN) + 0.5f)),
    EXPERT_ROW(SID_EXPERT_FILTER_8580_CURVE, "8580 filter curve", "0..127 = reSIDfp curve 0..1", 64),
    EXPERT_ROW(SID_EXPERT_NONLINEARITY, "6581 non-linearity", "0..127 = amount 0..2", 64),
    EXPERT_ROW(SID_EXPERT_FILTER_BIAS, "6581 filter input DC bias", "0..127 = 0..2.3125 voice-DAC units", 64),
    EXPERT_ROW(SID_EXPERT_BLEED, "Filter bleed", "0..127 = 0..50 percent of routed voices past the filter", 0),
    EXPERT_ROW(SID_EXPERT_DAC_KINK, "DAC ladder kink", "0..127 = 2R/R ratio 1.8..2.6", 64),
    EXPERT_ROW(SID_EXPERT_DAC_LEAKAGE, "DAC leakage", "0..127 = leakage 0..0.15", 6),
    EXPERT_ROW(SID_EXPERT_COMBINED, "Combined-waveform strength",
        "0..31 WEAK, 32..95 AVERAGE, 96..127 STRONG", 64),
    EXPERT_ROW(SID_EXPERT_NOISE_WRITEBACK, "Noise write-back",
        "0 off; 1..127 progressively connect 1..8 write-back taps; 127 authentic", 127),
    EXPERT_ROW(SID_EXPERT_THUMP, "Output thump",
        "0..127 = 0..1 voice-DAC units: a volume write's step at volume 15, a note's twice it",
        0),
    EXPERT_ROW(SID_EXPERT_EXTERNAL_LP, "6581 external low-pass", "0..127 = 1000..20000 Hz", 81),
    EXPERT_ROW(SID_EXPERT_EXTERNAL_HP, "External high-pass", "0..127 = 0..100 Hz", 20),
    EXPERT_ROW(SID_EXPERT_VOICE_DC, "Voice DC offset", "0..127 = 0..0.6 voice-DAC units", 60),

    ROW("Live", "PWM depth",
        "0 off .. 2048 pulse-width steps each way: the pulse width's LFO, a triangle (narrowed "
        "near the ends so the pulse never closes); a preset's",
        LIVE_PARAM_PWM_DEPTH, NO_CC, NRPN(NRPN_LIVE, 5), LIVE_PWM_DEPTH_MAX, 0, LINEAR),
    ROW("Live", "PWM rate", "0..12.75 Hz in 0.05 Hz steps (PWM BASS's 10 is 0.5 Hz); a preset's",
        LIVE_PARAM_PWM_RATE, NO_CC, NRPN(NRPN_LIVE, 6), LIVE_PWM_RATE_MAX, 0, LINEAR),
    ROW("Live", "Fine tune", "0..100 = -50..+50 cents, 50 in tune; every voice; a preset's",
        LIVE_PARAM_TUNE, NO_CC, NRPN(NRPN_LIVE, 7), 2 * LIVE_TUNE_CENTS, LIVE_TUNE_CENTS, STEPS),
    ROW("Live", "Filter LFO",
        "0 off .. 32 of K1's cutoff detents each way: the cutoff's LFO, a triangle at the PWM "
        "rate; a preset's (off)",
        LIVE_PARAM_FILTER_LFO, LIVE_CC_FILTER_LFO, NRPN(NRPN_LIVE, 8), LIVE_FILTER_LFO_MAX, 0,
        STEPS),

    ROW("Output", "Spread",
        "SETTINGS' SPREAD, 0..100 %: 0 mono, 100 chip A left and chip B right, equal power; "
        "saved with the settings (not a preset's, not learned, not sent)",
        MIDI_MAP_SPREAD, MIDI_MAP_CC_SPREAD, NRPN(NRPN_LIVE, 4), CHIP_SPREAD_MAX, 0, STEPS),
};
/* clang-format on */

_Static_assert(SID_DIST_COUNT == 6 && SID_MOD_COUNT == 7 && SID_FXSLOT_COUNT == 5,
               "the algorithm lists above name every algorithm");

const midi_map_entry* midi_map_row(uint32_t i) {
    return i < MIDI_MAP_ROWS ? &midi_map_rows[i] : NULL;
}

const midi_map_entry* midi_map_for_param(uint32_t param) {
    for (uint32_t i = 0; i < MIDI_MAP_ROWS; ++i) {
        if (midi_map_rows[i].param == param && param != MIDI_MAP_RESERVED) {
            return &midi_map_rows[i];
        }
    }
    return NULL;
}

/* Each CC's row + 1, 0 for none: midi_map_for_cc in one load, for every CC of a stream (the
 * rows' order, which the host tests check against the table). */
#define SLOT_CC(k) [LIVE_CC_SLOT_FIRST + (k)] = 21u + (k) /* the slots' rows from 20 */
/* A parameter past the experts: its row + 1 (42 rows before the experts, then the experts, then
 * one row each in the parameters' order). */
#define TAIL_ROW(p) (43u + SID_EXPERT_COUNT + (p) - LIVE_PARAM_EXPERT_END)
/* clang-format off */
static const uint8_t cc_rows[128] = {
    [LIVE_CC_CUTOFF] = 1u,
    [LIVE_CC_RESONANCE] = 2u,
    [LIVE_CC_PULSE_WIDTH] = 3u,
    [LIVE_CC_VOLUME] = 4u,
    [20] = 5u, /* waveform */
    [21] = 6u, /* ring */
    [22] = 7u, /* sync */
    [23] = 8u, /* filter mode */
    [24] = 9u, /* filter routing */
    [25] = 10u, /* chip model */
    [73] = 11u, /* attack */
    [75] = 12u, /* decay */
    [79] = 13u, /* sustain */
    [72] = 14u, /* release */
    [26] = 15u, /* envelope mode */
    [85] = 16u, /* the Character rows */
    [86] = 17u,
    [87] = 18u,
    [89] = 19u,
    [90] = 20u,
    [LIVE_CC_VIBRATO_RATE] = 40u, /* after the Expression rows without a CC */
    [LIVE_CC_VIBRATO_DEPTH] = 41u,
    [LIVE_CC_VIBRATO_DELAY] = 42u,
    [LIVE_CC_FILTER_LFO] = TAIL_ROW(LIVE_PARAM_FILTER_LFO), /* the sound's, after the experts */
    [MIDI_MAP_CC_SPREAD] = MIDI_MAP_ROWS, /* the last row */
    SLOT_CC(0),  SLOT_CC(1),  SLOT_CC(2),  SLOT_CC(3),  SLOT_CC(4),
    SLOT_CC(5),  SLOT_CC(6),  SLOT_CC(7),  SLOT_CC(8),  SLOT_CC(9),
    SLOT_CC(10), SLOT_CC(11), SLOT_CC(12), SLOT_CC(13), SLOT_CC(14),
};
/* clang-format on */
#undef SLOT_CC

_Static_assert(LIVE_CC_SLOT_LAST - LIVE_CC_SLOT_FIRST == 14u, "SLOT_CC covers every slot CC");

const midi_map_entry* midi_map_for_cc(uint8_t cc) {
    uint8_t r = cc < 128u ? cc_rows[cc] : 0u;
    return r ? &midi_map_rows[r - 1u] : NULL;
}

const midi_map_entry* midi_map_for_nrpn(uint16_t nrpn) {
    for (uint32_t i = 0; i < MIDI_MAP_ROWS; ++i) {
        if (midi_map_rows[i].nrpn == nrpn && nrpn != MIDI_MAP_NO_NRPN) {
            return &midi_map_rows[i];
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
    uint32_t max = e->max;
    if (max == 0 || value == 0) {
        return 0;
    }
    if (value >= max) {
        return 127;
    }
    if (max == 127u) {
        return (uint8_t)value;
    }
    return (uint8_t)((value * 127u + max / 2u) / max);
}

uint32_t midi_map_param_get(const live* l, uint32_t param) {
    return param == MIDI_MAP_SPREAD ? l->spread : live_param_get(l, param);
}

void midi_map_param_set(live* l, uint32_t param, uint32_t value) {
    if (param == MIDI_MAP_SPREAD) {
        chip_spread_set(l, value);
    } else {
        live_param_set(l, param, value);
    }
}

uint32_t midi_map_param_max(uint32_t param) {
    return param == MIDI_MAP_SPREAD ? CHIP_SPREAD_MAX : live_param_max(param);
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
    p->learn = NULL;
    p->banks = NULL;
    for (uint32_t i = 0; i < 16u; ++i) {
        deselect(&p->ch[i]);
    }
}

/* The parameter data entry goes to: none for an RPN (RPN 0 is rpn_data's), the null NRPN,
 * an unmapped or a reserved one. */
static const midi_map_entry* selected(const midi_map_channel* c) {
    if (c->rpn) {
        return NULL;
    }
    const midi_map_entry* e = midi_map_for_nrpn(MIDI_MAP_NRPN(c->nrpn_msb, c->nrpn_lsb));
    return e && e->param != MIDI_MAP_RESERVED ? e : NULL;
}

/* RPN 0, the bend range in semitones: CC 6 sets it at once (the standard's MSB), CC 38 (the
 * cents) is ignored, CC 96 and 97 step a semitone. Program changes keep it
 * (live_bend_range_rpn). Other RPNs do nothing. */
static void rpn_data(midi_map_port* p, const midi_map_channel* c, uint8_t cc, uint8_t value) {
    if (c->rpn_msb != 0u || c->rpn_lsb != RPN_BEND_RANGE) {
        return;
    }
    uint32_t now = live_param_get(p->l, LIVE_PARAM_BEND_RANGE);
    switch (cc) {
    case CC_DATA_MSB: live_bend_range_rpn(p->l, value); break;
    case CC_INCREMENT: live_bend_range_rpn(p->l, now + 1u); break;
    case CC_DECREMENT: live_bend_range_rpn(p->l, now ? now - 1u : 0u); break;
    default: break; /* CC_DATA_LSB */
    }
}

static void data(midi_map_port* p, midi_map_channel* c, uint8_t cc, uint8_t value) {
    if (c->rpn) {
        rpn_data(p, c, cc, value);
        return;
    }
    const midi_map_entry* e = selected(c);
    if (!e) {
        return;
    }
    uint32_t now = midi_map_param_get(p->l, e->param);
    uint32_t v;
    switch (cc) {
    case CC_DATA_MSB:
        c->data_msb = value;
        if (e->max < 128u) {
            return; /* a small parameter's value is in the LSB: CC 38 applies it */
        }
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
    midi_map_param_set(p->l, e->param, v > e->max ? e->max : v);
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
    case CC_BANK_MSB:
    case CC_BANK_LSB:
        if (p->banks) {
            p->banks[channel][cc == CC_BANK_LSB] = value;
        }
        break;
    case CC_RESET_ALL:
        deselect(c);
        live_cc(p->l, cc, value); /* the controllers */
        break;
    default: {
        /* A learned CC wins over the map's (midi_learn.h), CC 1's wheel included. */
        const midi_map_entry* e = p->learn ? midi_learn_route(p->learn, cc) : NULL;
        if (!e && cc == MIDI_MAP_CC_SPREAD) {
            e = midi_map_for_cc(cc); /* a setting's CC: live_cc does not know it */
        }
        if (e) {
            midi_map_param_set(p->l, e->param, midi_map_from_cc(e, value));
        } else {
            live_cc(p->l, cc, value);
        }
        break;
    }
    }
}
