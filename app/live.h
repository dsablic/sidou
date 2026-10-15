// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The live instrument: one SID chip played from keys, knobs and MIDI.
 *
 * Portable C with no hardware access. The include roots are core/ and app/, so this
 * header is "live.h" and the SID core is "sid/sid.h".
 *
 * live never touches a sid directly. Every register change goes through a sink, a
 * function called once per write in the order the writes must happen:
 * - on the host, live_sid_sink applies them to a sid at once;
 * - on the device, the sink pushes them onto a ring that the audio ISR drains, so the
 *   chip is only ever changed between two rendered blocks.
 * Besides the SID registers 0x00..0x18 a sink receives two chip pseudo-registers:
 * - LIVE_REG_RESET (value: a sid_model) asks for the chip to be re-initialised with that
 *   model at its current clock and sample rate, before any write that follows it;
 * - LIVE_REG_GATE_RESET (value 0 or 1) sets the chip's gate reset (sid_set_gate_reset), the
 *   attack mode (LIVE_PARAM_ATTACK_MODE). sid_init turns it off, so it follows every reset.
 * live_init and a model change send a reset, the gate reset, then every register; a change
 * of the attack mode sends the gate reset alone. */
#ifndef SIDOU_LIVE_H
#define SIDOU_LIVE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LIVE_VOICES 3
#define LIVE_NO_NOTE 0xffu
/* Outside the SID's 5-bit address space; sid_write would alias them, so sinks catch them. */
#define LIVE_REG_RESET 0x80u
#define LIVE_REG_GATE_RESET 0x81u

#define LIVE_OCTAVE_MIN (-3)
#define LIVE_OCTAVE_MAX 3
#define LIVE_CUTOFF_MAX 2047u
#define LIVE_CUTOFF_DETENT 16
#define LIVE_RESONANCE_MAX 15u
#define LIVE_PULSE_WIDTH_MAX 4095u
#define LIVE_PULSE_WIDTH_DETENT 32
#define LIVE_MASTER_MAX 1023u
/* live_tick is called this often; the PWM and vibrato rates assume it. */
#define LIVE_TICK_HZ 100u

/* Pitch offsets are in 1/8192 semitone (Q13): a 14-bit pitch bend times the range in
 * semitones is exact at every range. */
#define LIVE_PITCH_SEMITONE 8192
#define LIVE_BEND_CENTRE 8192u
#define LIVE_BEND_MAX 16383u
#define LIVE_BEND_RANGE_MAX 24u
#define LIVE_BEND_RANGE_DEFAULT 2u
/* The expression controls: 0..127 each. */
#define LIVE_EXPRESSION_MAX 127u
/* The vibrato rate's default, 6.0 Hz (the rate is in 0.1 Hz). */
#define LIVE_VIBRATO_RATE_DEFAULT 60u

/* CC numbers of the published map (midi_map.h, docs/reference/midi.md) that M1a and M1c
 * fixed, and the channel-mode messages live_cc handles. CC 1 is the modulation wheel: by
 * default (LIVE_MOD_PULSE_WIDTH) it sets the pulse width, as in M1a and the map's row; with
 * another target (LIVE_PARAM_MOD_TARGET) it drives that target instead. */
#define LIVE_CC_PULSE_WIDTH 1u
#define LIVE_CC_MOD_WHEEL 1u
#define LIVE_CC_VOLUME 7u
#define LIVE_CC_RESONANCE 71u
#define LIVE_CC_CUTOFF 74u
/* General MIDI 2's sound controllers 7..9. */
#define LIVE_CC_VIBRATO_RATE 76u
#define LIVE_CC_VIBRATO_DEPTH 77u
#define LIVE_CC_VIBRATO_DELAY 78u
/* The effect slots (core/fx/fx_slots.h, app/fx_live.h): CC 102 + 5 slot + field, slot 0
 * Distortion, 1 Modulation, 2 FX; field 0 algorithm (the value scaled over the slot's
 * algorithms), 1 Amount, 2 A, 3 B, 4 Mix. In General MIDI's undefined block 102..119. */
#define LIVE_CC_SLOT_FIRST 102u
#define LIVE_CC_SLOT_LAST 116u
#define LIVE_CC_ALL_SOUND_OFF 120u
#define LIVE_CC_RESET_ALL 121u
#define LIVE_CC_ALL_NOTES_OFF 123u

/* Effect slots: three, each with an algorithm and four controls 0..LIVE_SLOT_MAX. */
#define LIVE_SLOTS 3u
#define LIVE_SLOT_FIELDS 5u
#define LIVE_SLOT_MAX 127u
/* A knob detent moves a slot control this many steps: 32 detents end to end. */
#define LIVE_SLOT_DETENT 4

typedef enum {
    LIVE_SLOT_ALGORITHM = 0,
    LIVE_SLOT_AMOUNT = 1,
    LIVE_SLOT_A = 2,
    LIVE_SLOT_B = 3,
    LIVE_SLOT_MIX = 4
} live_slot_field;

/* What velocity sets, per preset. The SID has no per-voice VCA: these stand in for one. A
 * velocity of 127 sounds as OFF does; lower ones lower the target. */
typedef enum {
    LIVE_VELOCITY_OFF = 0,
    LIVE_VELOCITY_SUSTAIN = 1,    /* the voice's sustain level, scaled by velocity / 127 */
    LIVE_VELOCITY_CUTOFF = 2,     /* the filter, 8 steps lower per velocity below 127 */
    LIVE_VELOCITY_PULSE_WIDTH = 3 /* the voice's pulse, narrower by velocity / 127 */
} live_velocity_target;

/* How a note's attack starts (LIVE_PARAM_ATTACK_MODE), for the whole instrument: not per
 * preset, and kept across preset and model changes. */
typedef enum {
    /* The rate counter restarts at the gate (sid_set_gate_reset): the attack starts within
     * 12 chip cycles, whatever the release before it. The default: an instrument played
     * live wants every note on time. */
    LIVE_ATTACK_INSTANT = 0,
    /* The chip's ADSR delay bug, as reSIDfp has it: after a release slower than the attack,
     * most notes wait up to 33 ms for the rate counter to come round. */
    LIVE_ATTACK_AUTHENTIC = 1
} live_attack_mode;

/* What the modulation wheel (CC 1) drives, per preset. */
typedef enum {
    LIVE_MOD_PULSE_WIDTH = 0, /* CC 1 sets the pulse width across its range (M1a) */
    LIVE_MOD_VIBRATO = 1,     /* adds to the vibrato depth */
    LIVE_MOD_CUTOFF = 2       /* opens the filter above its cutoff, 8 steps per value */
} live_mod_target;

/* What channel and polyphonic aftertouch drive, per preset: a voice's pressure is the larger
 * of the channel's and its note's. */
typedef enum {
    LIVE_PRESSURE_OFF = 0,
    LIVE_PRESSURE_VIBRATO = 1,     /* adds to the voice's vibrato depth */
    LIVE_PRESSURE_PULSE_WIDTH = 2, /* raises the voice's pulse width, 16 steps per value */
    LIVE_PRESSURE_CUTOFF = 3       /* opens the filter, 8 steps per value of the most pressed */
} live_pressure_target;

typedef void (*live_sink_fn)(void* ctx, uint8_t reg, uint8_t value);

/* The context of live_sid_sink: a chip and the clock and rate it was initialised with. */
typedef struct {
    sid* chip;
    uint32_t clock_hz;
    uint32_t sample_rate;
} live_sid_target;

/* Applies writes to ctx (a live_sid_target*) at once: sid_write for registers, sid_init
 * for LIVE_REG_RESET, sid_set_gate_reset for LIVE_REG_GATE_RESET; other pseudo-registers
 * (the slots') are ignored. For the host and for
 * code that already runs in the audio context. */
void live_sid_sink(void* ctx, uint8_t reg, uint8_t value);

typedef enum {
    LIVE_KNOB_CUTOFF = 0,      /* K1: one detent = LIVE_CUTOFF_DETENT steps of 0..2047 */
    LIVE_KNOB_RESONANCE = 1,   /* K2: 0..15 */
    LIVE_KNOB_PULSE_WIDTH = 2, /* K3: one detent = LIVE_PULSE_WIDTH_DETENT of 0..4095 */
    LIVE_KNOB_PRESET = 3       /* K4: one detent = one preset, clamped at both ends */
} live_knob_id;

typedef enum {
    LIVE_BUTTON_OCT_DOWN = 0,
    LIVE_BUTTON_OCT_UP = 1,
    LIVE_BUTTON_FX = 2 /* toggles the chip 6581/8580 at once, keeping every parameter */
} live_button_id;

/* A preset is data. The knobs start from its cutoff, resonance and pulse width, and the SID
 * parameters (live_param) from its control, envelope and filter settings. */
typedef struct {
    const char* name;
    uint8_t control;         /* waveform, ring and sync bits of $d404; never the gate */
    uint8_t attack_decay;    /* $d405 */
    uint8_t sustain_release; /* $d406 */
    uint16_t pulse_width;    /* 0..4095 */
    uint16_t cutoff;         /* 0..2047 */
    uint8_t resonance;       /* 0..15 */
    uint8_t filter_voices;   /* $d417 bits 0..2 */
    uint8_t filter_mode;     /* $d418 bits 4..6 */
    /* 1: one note at a time on voice 1, hard-synced to voice 3, which runs silently at
     * the note's pitch while voice 1 runs sync_ratio_q8 / 256 times higher. */
    uint8_t mono_sync;
    uint16_t sync_ratio_q8;
    uint16_t pwm_depth; /* pulse-width LFO depth in pulse-width steps; 0 = off */
    uint16_t pwm_rate;  /* LFO phase step per tick; 65536 = one cycle */
    /* Expression (live_param): */
    uint8_t bend_range;      /* semitones each way, 0..LIVE_BEND_RANGE_MAX */
    uint8_t velocity_target; /* a live_velocity_target */
    uint8_t mod_target;      /* a live_mod_target */
    uint8_t pressure_target; /* a live_pressure_target */
    uint8_t vibrato_rate;    /* 0..127, in 0.1 Hz */
    uint8_t vibrato_depth;   /* 0..127, 127 = +-8128/8192 semitone */
    uint8_t vibrato_delay;   /* 0..127 ticks after the note-on */
} live_preset;

#define LIVE_PRESET_COUNT 4u
/* NULL past the last preset. */
const live_preset* live_preset_get(uint8_t index);

/* round(f * 2^24 / SID_CLOCK_PAL) for MIDI note n with f = 440 * 2^((n - 69) / 12).
 * Notes above 106 do not fit in 16 bits and are folded down by whole octaves.
 * Generated (live_freq.c) and checked by the host tests. */
extern const uint16_t live_freq_table[128];
/* For pitches between the notes (live_freq_at), also generated: notes 95..106, the top
 * octave that fits 16 bits, as round(f * 2^24 / SID_CLOCK_PAL * 16); and
 * round(2^20 * 2^(j / 768)) - 2^20 for j = 0..64, 64 steps across one semitone. */
extern const uint32_t live_freq_top_q4[12];
extern const uint32_t live_freq_fine_q20[65];

/* The frequency register for note (0..127; above 106 folded down by octaves, as
 * live_freq_table) moved by offset (LIVE_PITCH_SEMITONE per semitone): the table's value at
 * offset 0, otherwise within 0.6 of the exact value, rising with the offset and by at most 1
 * for each 1/8192 semitone. Clamped to note 0 and 0xffff. Integers only (32-bit). */
uint16_t live_freq_at(uint8_t note, int32_t offset);

typedef struct {
    uint8_t note;           /* the last note it played, LIVE_NO_NOTE if none */
    uint8_t gate;           /* 1 while the note is held */
    uint8_t velocity;       /* the note's, 1..127 */
    uint8_t pressure;       /* its polyphonic aftertouch, 0..127; 0 at the note-on */
    uint32_t stamp;         /* when it was last gated or released, for the allocator */
    uint16_t vibrato_phase; /* its vibrato LFO: 65536 = one cycle, from 0 at the note-on */
    uint8_t vibrato_wait;   /* ticks until its vibrato starts (the delay) */
    uint16_t freq_out;      /* the frequency register last written */
    uint16_t pw_out;        /* the pulse width last written */
} live_voice;

/* The whole instrument; fields are read-only for callers (the UI shows them). */
typedef struct {
    live_sink_fn sink;
    void* sink_ctx;
    sid_model model;     /* the chip's, as last reset */
    sid_model model_req; /* asked for by LIVE_PARAM_MODEL; live_tick applies it */
    uint8_t preset;
    int8_t octave;
    uint16_t cutoff;      /* 0..2047 */
    uint8_t resonance;    /* 0..15 */
    uint16_t pulse_width; /* 0..4095, before PWM */
    uint16_t master;      /* 0..1023 */
    uint8_t volume;       /* 0..15, from master */
    uint16_t cutoff_out;  /* the cutoff last written, after expression */
    uint16_t lfo_phase;
    uint32_t clock;
    live_voice voice[LIVE_VOICES];
    uint8_t slot[LIVE_SLOTS][LIVE_SLOT_FIELDS]; /* each slot's algorithm and controls */
    uint8_t fx_select;                          /* the slot the panel edits */
    /* The preset's SID settings as edited (live_param); loading a preset resets them. */
    uint8_t control;         /* waveform, ring and sync bits of $d404; never the gate */
    uint8_t attack_decay;    /* $d405 */
    uint8_t sustain_release; /* $d406 */
    uint8_t filter_voices;   /* $d417 bits 0..2 */
    uint8_t filter_mode;     /* $d418 bits 4..6 */
    /* The preset's expression settings as edited (live_param); loading a preset resets them,
     * except a bend range set by RPN 0 (bend_range_rpn). */
    uint8_t bend_range;
    /* 1 while the bend range is RPN 0's: the channel's setting (RP-015), kept across program
     * changes until power-off or an NRPN 6/0 edit (live_bend_range_rpn). */
    uint8_t bend_range_rpn;
    uint8_t velocity_target;
    uint8_t mod_target;
    uint8_t pressure_target;
    uint8_t vibrato_rate;
    uint8_t vibrato_depth;
    uint8_t vibrato_delay;
    /* The controllers (omni: one set for every channel); live_tick applies them. */
    uint16_t bend;            /* 0..16383, LIVE_BEND_CENTRE at rest */
    uint8_t mod_wheel;        /* CC 1, when the wheel has a target other than the pulse width */
    uint8_t channel_pressure; /* 0..127 */
    uint8_t last_velocity;    /* the last note-on's: LIVE_VELOCITY_CUTOFF's (one filter) */
    uint32_t note_ons; /* note-ons played (wraps): a note shorter than a look still shows */
    uint8_t attack_mode; /* a live_attack_mode */
} live;

/* The instrument's parameters, each an integer 0..live_param_max: what MIDI reaches through
 * the published map (midi_map.h). The SID ones apply to every voice. */
typedef enum {
    LIVE_PARAM_CUTOFF = 0,        /* 0..2047 */
    LIVE_PARAM_RESONANCE = 1,     /* 0..15 */
    LIVE_PARAM_PULSE_WIDTH = 2,   /* 0..4095, before PWM */
    LIVE_PARAM_VOLUME = 3,        /* the master, 0..1023 (live_set_master) */
    LIVE_PARAM_WAVEFORM = 4,      /* 0..15, $d404 bits 4..7: 1 triangle, 2 saw, 4 pulse, 8 noise */
    LIVE_PARAM_RING = 5,          /* 0..1, $d404 bit 2 */
    LIVE_PARAM_SYNC = 6,          /* 0..1, $d404 bit 1 */
    LIVE_PARAM_ATTACK = 7,        /* 0..15 */
    LIVE_PARAM_DECAY = 8,         /* 0..15 */
    LIVE_PARAM_SUSTAIN = 9,       /* 0..15 */
    LIVE_PARAM_RELEASE = 10,      /* 0..15 */
    LIVE_PARAM_FILTER_MODE = 11,  /* 0..7, $d418 bits 4..6: 1 low-, 2 band-, 4 high-pass */
    LIVE_PARAM_FILTER_VOICES = 12, /* 0..7, $d417 bits 0..2: voices 1, 2, 3 into the filter */
    LIVE_PARAM_MODEL = 13,        /* 0 6581, 1 8580: a reset, at the next live_tick */
    /* The effect slots: LIVE_PARAM_SLOT + slot * LIVE_SLOT_FIELDS + field (live_slot_set). */
    LIVE_PARAM_SLOT = 14, /* .. LIVE_PARAM_SLOT_END - 1 */
    /* Expression, per preset: recorded, and live_tick writes what they change; the velocity
     * target rewrites the envelopes at once. */
    LIVE_PARAM_BEND_RANGE = 29,      /* 0..24 semitones each way: a preset edit (RPN 0's is
                                      * live_bend_range_rpn) */
    LIVE_PARAM_VELOCITY_TARGET = 30, /* a live_velocity_target, 0..3 */
    LIVE_PARAM_MOD_TARGET = 31,      /* a live_mod_target, 0..2; a change restarts the wheel at 0 */
    LIVE_PARAM_PRESSURE_TARGET = 32, /* a live_pressure_target, 0..3 */
    LIVE_PARAM_VIBRATO_RATE = 33,    /* 0..127, in 0.1 Hz */
    LIVE_PARAM_VIBRATO_DEPTH = 34,   /* 0..127 */
    LIVE_PARAM_VIBRATO_DELAY = 35,   /* 0..127 ticks */
    /* A live_attack_mode, 0..1: one write (LIVE_REG_GATE_RESET), for the gates after it. */
    LIVE_PARAM_ATTACK_MODE = 36,
    LIVE_PARAM_COUNT = 37
} live_param;

/* One past the last slot parameter, i.e. LIVE_PARAM_BEND_RANGE (live.c asserts it). A macro,
 * not an enumerator: the enum gives each value one name, so a switch over live_param cannot
 * meet the same value under two case labels. */
#define LIVE_PARAM_SLOT_END (LIVE_PARAM_SLOT + LIVE_SLOTS * LIVE_SLOT_FIELDS)

/* Preset 0, octave 0, full volume, every effect slot off, the attack instant. Sends
 * LIVE_REG_RESET(model), LIVE_REG_GATE_RESET, then every register, then every slot setting
 * (fx_live.h). */
void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model);
/* Absolute MIDI notes; the octave is not applied here (see live_key_note). Velocity 0
 * is a note-off. A note that is already held is retriggered on its voice. The note starts at
 * the current bend, with its pressure at 0 and its vibrato at the start of the delay; its
 * velocity applies at once (the sustain level before the gate). */
void live_note_on(live* l, uint8_t note, uint8_t velocity);
void live_note_off(live* l, uint8_t note);
void live_all_notes_off(live* l);
/* The MIDI note for a key whose note at octave 0 is base_note, or LIVE_NO_NOTE.
 * The device must keep the note it got at key-down and send that same note at key-up:
 * the octave may change while the key is held. */
uint8_t live_key_note(const live* l, uint8_t base_note);
void live_knob(live* l, live_knob_id knob, int32_t delta);
void live_button(live* l, live_button_id button);
/* A CC through the published map (midi_map.h): the parameter's value from the CC's by its
 * curve. CC 1 is the modulation wheel: the pulse width's row while that is its target, else
 * the wheel's position for its target. CC 120 and 123 release every note; CC 121 resets the
 * controllers (live_reset_controllers). Unmapped and reserved CCs, and values past 127, do
 * nothing. NRPNs need state per channel: midi_map_cc. */
void live_cc(live* l, uint8_t cc, uint8_t value);

/* The controllers are only recorded: live_tick applies them, so a flood of 14-bit bends or
 * pressure costs no register writes beyond one tick's. */
/* Pitch bend, 0..16383 (clamped), LIVE_BEND_CENTRE at rest: +-bend_range semitones. */
void live_bend(live* l, uint16_t bend);
/* Channel aftertouch, 0..127 (clamped). */
void live_pressure(live* l, uint8_t pressure);
/* Polyphonic aftertouch, 0..127 (clamped), for the voices holding note. */
void live_poly_pressure(live* l, uint8_t note, uint8_t pressure);
/* Bend to the centre, the wheel and every pressure to 0 (MIDI's reset all controllers). The
 * pulse width is a parameter, not the wheel's position: it is kept. */
void live_reset_controllers(live* l);
/* The largest value of a parameter; 0 for an unknown one. */
uint32_t live_param_max(uint32_t param);
/* Sets a parameter, clamped to 0..live_param_max; writes only on a change. Unknown
 * parameters are ignored. The chip model is only recorded here: re-initialising the chip
 * waits for the audio interrupt to empty the register ring and runs sid_init in the main
 * loop, too slow for every CC of a stream. live_tick applies the last model asked for, at
 * most once a tick and only when it differs from the chip's; the FX button applies at once. */
void live_param_set(live* l, uint32_t param, uint32_t value);
/* A parameter's value; 0 for an unknown one. For the chip model, the one last asked for
 * (model_req), so a set reads back at once; the chip's is the model field. */
uint32_t live_param_get(const live* l, uint32_t param);
/* RPN 0, pitch-bend sensitivity: sets the bend range (clamped to 0..LIVE_BEND_RANGE_MAX) as
 * the channel's setting, which program changes keep (RP-015 makes RPN values channel state)
 * until power-off. Setting LIVE_PARAM_BEND_RANGE (NRPN 6/0) is a preset edit and ends that:
 * of RPN 0 and NRPN 6/0 the last one wins, and only RPN 0's survives a program change. */
void live_bend_range_rpn(live* l, uint32_t semitones);
/* Loads a preset: releases every voice and resets cutoff, resonance and pulse width, the
 * five SID settings the parameters edit (waveform with ring and sync, attack/decay,
 * sustain/release, filter routing, filter mode) and the expression settings (bend range,
 * targets, vibrato), to the preset's. Loading the preset already loaded does the same, so it
 * discards those edits. Volume, octave, chip model, attack mode, the effect slots, the
 * controllers (bend, wheel, pressure) and a bend range set by RPN 0 are kept. An index past the last is
 * clamped. */
void live_set_preset(live* l, uint8_t index);
/* The MASTER pot, 0..1023 (clamped); the chip's volume is its top 4 bits, rounded. */
void live_set_master(live* l, uint16_t value);
/* Applies a chip model asked for since the last tick (live_param_set), advances the
 * pulse-width LFO and each voice's vibrato, then writes every frequency, pulse width and the
 * cutoff that the controllers, the vibrato or the LFO changed (nothing that did not change);
 * call at LIVE_TICK_HZ. At most 14 writes besides a model change. */
void live_tick(live* l);

/* Effect slots. The number of algorithms slot has (its NONE included). */
uint32_t live_slot_algorithms(uint32_t slot);
/* Sets a slot's field: an algorithm clamped to the slot's, a control to 0..LIVE_SLOT_MAX.
 * Sends it when it changed. */
void live_slot_set(live* l, uint32_t slot, uint32_t field, int32_t value);
/* Turns a slot control by detents (LIVE_SLOT_DETENT steps each), clamped at both ends. */
void live_slot_turn(live* l, uint32_t slot, uint32_t field, int32_t detents);
/* The slot's next algorithm, wrapping from the last to NONE. */
void live_slot_next_algorithm(live* l, uint32_t slot);
/* The panel's slot: the next one, wrapping. */
void live_fx_select_next(live* l);

#ifdef __cplusplus
}
#endif

#endif
