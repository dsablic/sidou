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
 * live_init and the model toggle send it, each followed by a write of every register. */
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

/* MIDI CCs live_cc understands. */
#define LIVE_CC_VOLUME 7u
#define LIVE_CC_PULSE_WIDTH 70u
#define LIVE_CC_RESONANCE 71u
#define LIVE_CC_CUTOFF 74u
#define LIVE_CC_ALL_SOUND_OFF 120u
#define LIVE_CC_ALL_NOTES_OFF 123u

typedef void (*live_sink_fn)(void* ctx, uint8_t reg, uint8_t value);

/* The context of live_sid_sink: a chip and the clock and rate it was initialised with. */
typedef struct {
    sid* chip;
    uint32_t clock_hz;
    uint32_t sample_rate;
} live_sid_target;

/* Applies writes to ctx (a live_sid_target*) at once: sid_write for registers, sid_init
 * for LIVE_REG_RESET. For the host and for code that already runs in the audio context. */
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
    LIVE_BUTTON_FX = 2 /* toggles 6581/8580 and keeps every parameter */
} live_button_id;

/* A preset is data. The knobs start from its cutoff, resonance and pulse width. */
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
    sid_model model;
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
} live;

/* Preset 0, octave 0, full volume. Sends LIVE_REG_RESET(model) and then every register. */
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
void live_cc(live* l, uint8_t cc, uint8_t value);
/* Loads a preset: releases every voice and resets cutoff, resonance and pulse width to
 * the preset's. An index past the last is clamped. */
void live_set_preset(live* l, uint8_t index);
/* The MASTER pot, 0..1023 (clamped); the chip's volume is its top 4 bits, rounded. */
void live_set_master(live* l, uint16_t value);
/* Advances the pulse-width LFO; call at LIVE_TICK_HZ. */
void live_tick(live* l);

#ifdef __cplusplus
}
#endif

#endif
