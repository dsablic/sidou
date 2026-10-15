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
 * Besides the SID registers 0x00..0x18 a sink receives one pseudo-register,
 * LIVE_REG_RESET (value: a sid_model). It asks for the chip to be re-initialised with
 * that model at its current clock and sample rate, before any write that follows it.
 * live_init and a model change send it, each followed by a write of every register. */
#ifndef SIDOU_LIVE_H
#define SIDOU_LIVE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LIVE_VOICES 3
#define LIVE_NO_NOTE 0xffu
/* Outside the SID's 5-bit address space; sid_write would alias it, so sinks catch it. */
#define LIVE_REG_RESET 0x80u

#define LIVE_OCTAVE_MIN (-3)
#define LIVE_OCTAVE_MAX 3
#define LIVE_CUTOFF_MAX 2047u
#define LIVE_CUTOFF_DETENT 16
#define LIVE_RESONANCE_MAX 15u
#define LIVE_PULSE_WIDTH_MAX 4095u
#define LIVE_PULSE_WIDTH_DETENT 32
#define LIVE_MASTER_MAX 1023u
/* live_tick is called this often; the PWM rates in the presets assume it. */
#define LIVE_TICK_HZ 100u

/* CC numbers of the published map (midi_map.h, docs/reference/midi.md) that M1a and M1c
 * fixed, and the channel-mode messages live_cc handles. The pulse width is on CC 1, the
 * modulation wheel most keyboards have. */
#define LIVE_CC_PULSE_WIDTH 1u
#define LIVE_CC_VOLUME 7u
#define LIVE_CC_RESONANCE 71u
#define LIVE_CC_CUTOFF 74u
/* The effect slots (core/fx/fx_slots.h, app/fx_live.h): CC 102 + 5 slot + field, slot 0
 * Distortion, 1 Modulation, 2 FX; field 0 algorithm (the value scaled over the slot's
 * algorithms), 1 Amount, 2 A, 3 B, 4 Mix. In General MIDI's undefined block 102..119. */
#define LIVE_CC_SLOT_FIRST 102u
#define LIVE_CC_SLOT_LAST 116u
#define LIVE_CC_ALL_SOUND_OFF 120u
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

typedef void (*live_sink_fn)(void* ctx, uint8_t reg, uint8_t value);

/* The context of live_sid_sink: a chip and the clock and rate it was initialised with. */
typedef struct {
    sid* chip;
    uint32_t clock_hz;
    uint32_t sample_rate;
} live_sid_target;

/* Applies writes to ctx (a live_sid_target*) at once: sid_write for registers, sid_init
 * for LIVE_REG_RESET; other pseudo-registers (the slots') are ignored. For the host and for
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
} live_preset;

#define LIVE_PRESET_COUNT 4u
/* NULL past the last preset. */
const live_preset* live_preset_get(uint8_t index);

/* round(f * 2^24 / SID_CLOCK_PAL) for MIDI note n with f = 440 * 2^((n - 69) / 12).
 * Notes above 106 do not fit in 16 bits and are folded down by whole octaves.
 * Generated (live_freq.c) and checked by the host tests. */
extern const uint16_t live_freq_table[128];

typedef struct {
    uint8_t note;   /* the last note it played, LIVE_NO_NOTE if none */
    uint8_t gate;   /* 1 while the note is held */
    uint32_t stamp; /* when it was last gated or released, for the allocator */
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
    uint16_t pw_out;      /* the pulse width last written, after PWM */
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
    LIVE_PARAM_SLOT = 14,
    LIVE_PARAM_COUNT = 29
} live_param;

/* Preset 0, octave 0, full volume, every effect slot off. Sends LIVE_REG_RESET(model), then
 * every register, then every slot setting (fx_live.h). */
void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model);
/* Absolute MIDI notes; the octave is not applied here (see live_key_note). Velocity 0
 * is a note-off. A note that is already held is retriggered on its voice. */
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
 * curve. CC 120 and 123 release every note. Unmapped and reserved CCs, and values past 127,
 * do nothing. NRPNs need state per channel: midi_map_cc. */
void live_cc(live* l, uint8_t cc, uint8_t value);
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
/* Loads a preset: releases every voice and resets cutoff, resonance and pulse width, and
 * the five SID settings the parameters edit (waveform with ring and sync, attack/decay,
 * sustain/release, filter routing, filter mode), to the preset's. Loading the preset already
 * loaded does the same, so it discards those edits. Volume, octave, chip model and the
 * effect slots are kept. An index past the last is clamped. */
void live_set_preset(live* l, uint8_t index);
/* The MASTER pot, 0..1023 (clamped); the chip's volume is its top 4 bits, rounded. */
void live_set_master(live* l, uint16_t value);
/* Applies a chip model asked for since the last tick (live_param_set), then advances the
 * pulse-width LFO; call at LIVE_TICK_HZ. */
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
