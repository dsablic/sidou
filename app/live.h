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
 *   chip is only ever changed between two rendered blocks. live is chip-agnostic: it writes
 *   chip A's registers, and a sink bound to chip B names B in each entry (app/reg_ring.h).
 * Besides the SID registers 0x00..0x18 a sink receives two chip pseudo-registers:
 * - LIVE_REG_RESET (value: a sid_model) asks for the chip to be re-initialised with that
 *   model at its current clock and sample rate, before any write that follows it;
 * - LIVE_REG_GATE_RESET (value 0 or 1) sets the chip's gate reset (sid_set_gate_reset), the
 *   envelope mode (LIVE_PARAM_ENVELOPE_MODE). sid_init turns it off, so it follows every
 *   reset.
 * - LIVE_REG_CHAR(field) (value: the setting) gives the chip's character (sid_character.h):
 *   field 0 the profile (a SID_CHIP_*), 1 age/heat, 2 DAC sloppiness, 3 output thump, 4
 *   filter bleed (0..127 each). A sink keeps them and applies them to every chip a later
 *   reset makes (sid_init resets the character), at once, and ramps the playing chip
 *   towards the controls at each LIVE_REG_CHAR_TICK (sid_character_ramp_tick). A profile
 *   only ever changes with a reset after it.
 * - LIVE_REG_CHAR_TICK (value 0), once every live_tick, after the settings that changed.
 * live_init and a model or profile change send the character's settings, a reset, the gate
 * reset, then every register; a change of the envelope mode sends the gate reset alone; a
 * tick sends the character's controls that changed and LIVE_REG_CHAR_TICK.
 *
 * A model or profile change with a preparer (live_set_preparer) waits for the chip the reset
 * will swap in: live_tick calls the preparer once a tick, a slice of the work each (the
 * sink's spare chip, app/reg_ring.h), and sends the reset, in that tick, once it says the
 * chip is whole; the chip that plays meanwhile keeps every register and setting. Without
 * one, the reset goes at the next tick (live_button's toggle at once) and the sink prepares the
 * chip in that call. */
#ifndef SIDOU_LIVE_H
#define SIDOU_LIVE_H

#include <stdint.h>

#include "sid/sid.h"
#include "sid/sid_character.h"

#include "lab_sound.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LIVE_VOICES 3
#define LIVE_NO_NOTE 0xffu
/* Outside the SID's 5-bit address space; sid_write would alias them, so sinks catch them. */
#define LIVE_REG_RESET 0x80u
#define LIVE_REG_GATE_RESET 0x81u
/* The character (above): LIVE_REG_CHAR(0..LIVE_CHAR_FIELDS - 1) is 0x82..0x86, then the
 * tick. */
#define LIVE_CHAR_FIELDS 5u
/* The USER chip profiles: slots USER 1..8, after the built-in profiles in LIVE_PARAM_PROFILE. */
#define LIVE_USER_PROFILES 8u
#define LIVE_REG_CHAR_FIRST 0x82u
#define LIVE_REG_CHAR(field) (LIVE_REG_CHAR_FIRST + (field))
#define LIVE_REG_CHAR_TICK 0x87u
#define LIVE_REG_EXPERT_FIRST 0x90u
#define LIVE_REG_EXPERT(field) (LIVE_REG_EXPERT_FIRST + (field))

#define LIVE_OCTAVE_MIN (-3)
#define LIVE_OCTAVE_MAX 3
#define LIVE_CUTOFF_MAX 2047u
#define LIVE_RESONANCE_MAX 15u
#define LIVE_PULSE_WIDTH_MAX 4095u
/* K3 on a pulse sound: positions 0..LIVE_PULSE_POSITIONS - 1, the width LIVE_PULSE_FIRST +
 * LIVE_PULSE_STEP * position (3 % to 50 %, 1.6 % a detent), clockwise wider. */
#define LIVE_PULSE_FIRST 128
#define LIVE_PULSE_STEP 64
#define LIVE_PULSE_POSITIONS 31
/* K3 on a sync sound: the sync ratio's offset in semitones, a detent each way. */
#define LIVE_RATIO_SEMIS 12
#define LIVE_MASTER_MAX 1023u
/* The volume the live instrument leaves in the chip's $D418 bits 0-3, always: MASTER is a
 * digital gain after the chips (live_set_master). */
#define LIVE_CHIP_VOLUME 15u
/* live_tick is called this often; the PWM and vibrato rates assume it. */
#define LIVE_TICK_HZ 100u
/* The pulse-width LFO (live.pwm_depth, live.pwm_rate): its depth at most, in pulse-width steps,
 * and its phase step a tick at most (255 x 0.05 Hz: round(255 * 65536 * 0.05 / LIVE_TICK_HZ)). */
#define LIVE_PWM_DEPTH_MAX 2048u
#define LIVE_PWM_STEP_MAX 8356u
/* The fine tune (live.tune), cents each way. */
#define LIVE_TUNE_CENTS 50
/* LIVE_PARAM_PWM_RATE's largest value: 12.75 Hz. */
#define LIVE_PWM_RATE_MAX 255u
/* The filter LFO (live.filter_lfo): its depth at most, K1's cutoff walk's detents each way
 * (live_cutoff_steps), at the PWM's rate. */
#define LIVE_FILTER_LFO_MAX 32u
/* The macro links (live.reso_link, live.pwm_link): -LIVE_LINK_MAX..LIVE_LINK_MAX. */
#define LIVE_LINK_MAX 8

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
/* The chord arpeggio (live_arp_set), in General MIDI's undefined block 27..31: on/off (off
 * below 64), the rate (the value scaled over LIVE_ARP_RATES), the order (over the four) and the
 * span (1 octave below 64, 2 from 64). Not parameters of the map: no NRPN, and MIDI out does not
 * send them. */
#define LIVE_CC_ARP 27u
#define LIVE_CC_ARP_RATE 28u
#define LIVE_CC_ARP_ORDER 29u
#define LIVE_CC_ARP_SPAN 30u
/* The filter LFO's depth (LIVE_PARAM_FILTER_LFO), the last of the block 20..31. */
#define LIVE_CC_FILTER_LFO 31u
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

/* The chord arpeggio (live_arp_set): with it on, the notes held (the keys' and MIDI's) are one
 * voice's, which steps through them, a note every LIVE_ARP_RATES' frames (50 Hz, a C64 player's),
 * its gate held from the first note to the release of the last: only the frequency moves, at a
 * tick, so the chip changes pitch between two rendered blocks with no new attack. The other
 * voices stay free. */
#define LIVE_ARP_HELD 8u /* notes it keeps; a ninth drops the oldest */
#define LIVE_ARP_RATES 9u
/* Rates from here count MIDI clocks (live_arp_steps: a 32nd, a 16th, an 8th) while a clock comes
 * (one in the last LIVE_ARP_CLOCK_TICKS ticks), else as many frames (a clock a frame: 125 BPM). */
#define LIVE_ARP_RATE_CLOCK 6u
/* A fresh preset's rate (load_preset): 3F, three frames a note. At 1F a held chord sounds like a
 * wobble (the v0.3.0 device session); at 3F each note is heard, still a C64 arpeggio. */
#define LIVE_ARP_RATE_DEFAULT 2u
#define LIVE_ARP_CLOCK_TICKS 30u
#define LIVE_ARP_SPAN_MAX 2u
#define LIVE_ARP_NO_VOICE 0xffu
/* live_tick's ticks a frame of 50 Hz. */
#define LIVE_ARP_TICKS_PER_FRAME 2u

typedef enum {
    LIVE_ARP_UP = 0,      /* lowest to highest, then again */
    LIVE_ARP_DOWN = 1,    /* highest to lowest */
    LIVE_ARP_UP_DOWN = 2, /* up then down, the ends once each */
    LIVE_ARP_PLAYED = 3   /* in the order the keys went down */
} live_arp_order;

/* live_arp_set's fields. */
typedef enum {
    LIVE_ARP_ON = 0,    /* 0 off, 1 on */
    LIVE_ARP_RATE = 1,  /* 0..LIVE_ARP_RATES - 1: live_arp_steps */
    LIVE_ARP_ORDER = 2, /* a live_arp_order */
    LIVE_ARP_SPAN = 3   /* octaves, 1..LIVE_ARP_SPAN_MAX: the notes, then again an octave up */
} live_arp_field;

/* Frames a step of each rate (1, 2, 3, 4, 6 and 8: 50 Hz to 6.25 Hz), then the clock's: MIDI
 * clocks a step from LIVE_ARP_RATE_CLOCK (3 a 32nd, 6 a 16th, 12 an 8th). */
extern const uint8_t live_arp_steps[LIVE_ARP_RATES];

/* What velocity sets, per preset. The SID has no per-voice VCA: these stand in for one. A
 * velocity of 127 sounds as OFF does; lower ones lower the target. */
typedef enum {
    LIVE_VELOCITY_OFF = 0,
    LIVE_VELOCITY_SUSTAIN = 1,    /* the voice's sustain level, scaled by velocity / 127 */
    LIVE_VELOCITY_CUTOFF = 2,     /* the filter, 8 steps lower per velocity below 127 */
    LIVE_VELOCITY_PULSE_WIDTH = 3 /* the voice's pulse, narrower by velocity / 127 */
} live_velocity_target;

/* How a note's attack and release start (LIVE_PARAM_ENVELOPE_MODE), for the whole
 * instrument: not per preset, and kept across preset and model changes. */
typedef enum {
    /* The rate counter restarts at each gate edge (sid_set_gate_reset): the attack at a
     * note-on and the release at a note-off take their first step within one rate period and
     * 3 chip cycles (12 cycles at attack 0), whatever the rate before them. The default: an
     * instrument played live wants every note, and every note's end, on time. */
    LIVE_ENVELOPE_INSTANT = 0,
    /* The chip's ADSR delay bug, as reSIDfp has it: after a slower rate (a release before
     * an attack, a decay before a release), most notes and note-offs wait up to 33 ms for
     * the rate counter to come round. */
    LIVE_ENVELOPE_AUTHENTIC = 1
} live_envelope_mode;

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
/* A preparer: one slice of preparing the chip a reset will swap in for profile (a SID_CHIP_*,
 * whose model the reset names) and the character's controls the sink has; 1 once that chip
 * is whole. LIVE_PREPARE_CANCEL (the request went back to the chip's own profile) drops the
 * preparation and returns 0. live_tick is its only caller, once a tick at most. */
typedef int (*live_prepare_fn)(void* ctx, uint8_t profile);
#define LIVE_PREPARE_CANCEL 0xffu

/* The context of live_sid_sink: a chip and the clock and rate it was initialised with, the
 * character's ramp (sid_character.h; zero-initialised: the model's default profile, every
 * control at 0, the settings as the sink last got them in ramp.target), and two tables for
 * the character's DACs (dacs[0] and dacs[1]; the ramp builds into the one the chip does not
 * point at): NULL plays the model's default DACs whatever the character says. */
typedef struct {
    sid* chip;
    uint32_t clock_hz;
    uint32_t sample_rate;
    sid_character_ramp ramp;
    sid_dac* dacs;
    /* Optional, for live_sid_prepare: the chip it prepares, a slice a call, as the device's
     * register ring prepares its spare (app/reg_ring.h), and a reset copies into chip. */
    sid* spare;
    sid_character_prep prep;
    uint8_t prep_on;
    uint8_t prep_profile;
    uint8_t prep_tick;          /* a slice ran since the last LIVE_REG_CHAR_TICK */
    uint8_t default_8580_spent; /* as reg_ring_chips' */
} live_sid_target;

/* Applies writes to ctx (a live_sid_target*) at once: sid_write for registers, sid_init and
 * then the character (sid_character_apply) for LIVE_REG_RESET, unless live_sid_prepare has
 * made the spare whole for it (then the spare, copied into the chip), sid_set_gate_reset for
 * LIVE_REG_GATE_RESET, the character's settings kept for LIVE_REG_CHAR, and at
 * LIVE_REG_CHAR_TICK the ramp's tick (sid_character_ramp_tick) and its stage committed;
 * other pseudo-registers (the slots') are ignored. For the host and for code that already
 * runs in the audio context. */
void live_sid_sink(void* ctx, uint8_t reg, uint8_t value);
/* A live_prepare_fn, ctx a live_sid_target* with a spare: the device's reg_ring_prepare_step
 * on the host, slice for slice (the same preparation, the same slices on the 8580's default
 * DACs, the ramp held off the DAC tables while it runs and in a tick that ran a slice), so a
 * host render with it swaps chips at the device's tick. Each tick stands for a main-loop
 * pass: in the device's catch-up pass of two ticks (platform/fm1/main.c) the second runs no
 * slice and starts no DAC table, a tick later than here. */
int live_sid_prepare(void* ctx, uint8_t profile);
/* A LIVE_REG_CHAR(field) write into the settings it changes (any other reg: nothing). For
 * the sinks. */
void live_char_store(sid_character* c, uint8_t reg, uint8_t value);

typedef enum {
    LIVE_KNOB_CUTOFF = 0,    /* K1: one detent = one entry of live_cutoff_steps */
    LIVE_KNOB_RESONANCE = 1, /* K2: 0..15 */
    LIVE_KNOB_TONE = 2,      /* K3: the sound's own control (live_tone) */
    LIVE_KNOB_RELEASE = 3    /* K4: 0..15, a step a detent */
} live_knob_id;

/* What K3 turns, by the sound (live_tone). */
typedef enum {
    LIVE_TONE_RATIO = 0, /* the sync ratio in semitones, -12..+12 from the preset's (SYNC LEAD) */
    LIVE_TONE_PULSE = 1, /* the pulse width, one side of the square (a sound with the pulse bit) */
    LIVE_TONE_DECAY = 2  /* LIVE_PARAM_DECAY, 0..15 (the saw and the noise) */
} live_tone_kind;

typedef enum {
    LIVE_BUTTON_OCT_DOWN = 0,
    LIVE_BUTTON_OCT_UP = 1,
    LIVE_BUTTON_FX = 2 /* toggles the chip 6581/8580, keeping every parameter: the 8580 R5
                        * profile, or back to the last 6581 profile; at once, or with a
                        * preparer the model asked for, which live_tick prepares. No panel
                        * button calls it since FX opens LAB (ALGORITHM on TONE and CC 25
                        * set the model their own ways); the tests do */
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
/* The four presets and DICE at index 4 (live_dice.h): what PRESETS steps through. A program change
 * and the menu's MIDI side know LIVE_PRESET_COUNT only. */
#define LIVE_PRESET_SLOTS 5u
#define LIVE_PRESET_DICE 4u
/* The USER sounds (app/user_sound.h USER_SOUNDS). */
#define LIVE_SOUNDS 16u
/* NULL past DICE. */
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

/* K1's walk (live_cutoff.c, generated; checked by the host tests): per chip model (a
 * sid_model, 0 6581, 1 8580), LIVE_CUTOFF_STEPS registers rising strictly in register and in Hz,
 * from the first one a semitone above the chip's lowest cutoff to the highest, spaced evenly in
 * pitch. live_cutoff_up[m][b] and live_cutoff_down[m][b], for b = the register >> 5: the first
 * entry above every register of the bucket (LIVE_CUTOFF_STEPS if none), and the last entry below
 * every register of it (-1 if none). */
#define LIVE_CUTOFF_STEPS 65u
extern const uint16_t live_cutoff_steps[2][LIVE_CUTOFF_STEPS];
extern const int8_t live_cutoff_up[2][64];
extern const int8_t live_cutoff_down[2][64];

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

/* One preset's edits as PRESETS keeps them while another preset plays: the fields of the live
 * instrument the knobs, the parameters and MIDI change from the preset's own values, as the
 * preset was left. valid is 0 until the preset has been left once (and after a program change
 * or a revert has loaded its factory settings over them). */
typedef struct {
    uint16_t cutoff, pulse_width;
    uint8_t resonance, control, attack_decay, sustain_release, filter_voices, filter_mode;
    uint8_t bend_range, velocity_target, mod_target, pressure_target;
    uint8_t vibrato_rate, vibrato_depth, vibrato_delay;
    int8_t sync_semis;
    uint8_t arp_on, arp_rate, arp_order, arp_span; /* the chord arpeggio's settings */
    uint8_t valid;
    uint16_t pwm_depth, pwm_rate; /* the pulse-width LFO's, as live's */
    int8_t tune;                  /* the fine tune, cents */
    uint8_t filter_lfo;           /* the filter LFO's depth, as live's */
    int8_t reso_link, pwm_link;   /* the macro links, as live's */
} live_edit;

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
    uint8_t volume;       /* 0..100, the pot's travel in percent: what the views show */
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
    uint8_t envelope_mode; /* a live_envelope_mode */
    /* The chip's character (LIVE_PARAM_PROFILE..LIVE_PARAM_BLEED), for the whole instrument
     * like the model. The profile as the chip has it, as asked for (live_tick applies it,
     * with a reset), and the last 6581 profile asked for (live_button's toggle and the model
     * go back to it). The controls as asked for, and as last sent; live_tick sends them. */
    uint8_t profile;
    uint8_t profile_req;
    uint8_t profile_6581;
    uint8_t character[LIVE_CHAR_FIELDS - 1u];
    uint8_t character_sent[LIVE_CHAR_FIELDS - 1u];
    /* The preparer (live_set_preparer; NULL: none), and 1 while live_tick has asked it for
     * the profile it waits for (profile_req). */
    live_prepare_fn prepare;
    void* prepare_ctx;
    uint8_t preparing;
    /* Two chips (M2b Task 4, app/keys_route.h; all 0 after live_init, one chip as in M1):
     * 1 once a keys_route plays the instrument on chip A or B; the chip the keys play (0 A,
     * 1 B); the other chip's model and profile as it has them (for the views); and the KEYS
     * setting (a live_keys_mode). */
    uint8_t two_chips;
    uint8_t keys_chip;
    uint8_t other_model;
    uint8_t other_profile;
    uint8_t keys_mode;
    /* A UI sound holds this voice of the keys' chip (app/ui_sound.h): v + 1, 0 none. The
     * allocator skips it, and takes it back (setting it to 0) only when the other voices are
     * all held. */
    uint8_t ui_voice;
    /* Bit 0: a persisted setter ran; bit 1: settings work pending. Setters assign 1 because
     * bit 0 forces settings.c's pending() to recompute the pending work. */
    uint8_t settings_dirty;
    /* The USER chip profiles (M2a Task 6; app/user_profile.h keeps them on the store): bit n of
     * user_valid, USER n + 1 holds one, its base profile and four character controls in
     * user_base[n] and user_ctl[n]; user_applied, n + 1 while the keys' setup was set by
     * picking USER n + 1 (any other profile or model change clears it), else 0. At the end:
     * the fields above keep their offsets. */
    uint8_t user_valid;
    uint8_t user_applied;
    uint8_t user_base[LIVE_USER_PROFILES];
    uint8_t user_ctl[LIVE_USER_PROFILES][LIVE_CHAR_FIELDS - 1u];
    /* The screens' theme (app/theme.h THEME_*; the menu's THEME, persisted). Not an
     * instrument setting: it is here for every view to reach through its `live`. At the end. */
    uint8_t theme;
    /* SETTINGS' SPREAD, 0..100 % (app/chip_spread.h: chip A left, chip B right; persisted). Not an
     * instrument setting either: here for the menu, MIDI's map and the settings to reach. */
    uint8_t spread;
    uint8_t expert[SID_EXPERT_COUNT];
    uint8_t expert_sent[SID_EXPERT_COUNT];
    uint8_t user_expert[LIVE_USER_PROFILES][SID_EXPERT_COUNT];
    uint8_t expert_dirty;
    uint8_t expert_active;
    uint32_t expert_version; /* raw expert changes, including transitions to macro-following */
    /* K1's place in live_cutoff_steps: valid while the table holds l->cutoff there. */
    uint8_t cutoff_pos;
    /* K3 on a sync sound: the ratio's offset in semitones, and the ratio in 1/256 as voice 1 plays
     * it (the preset's times 2^(sync_semis / 12)). Not a MIDI parameter and not saved: the panel's
     * alone; a preset load puts them back to the preset's. */
    int8_t sync_semis;
    uint16_t sync_q8;
    /* Each preset's edits while another preset plays (live_recall_preset). */
    live_edit edits[LIVE_PRESET_SLOTS];
    /* DICE's rolls so far (wraps), the number in its name. Not saved. */
    uint8_t dice_rolls;
    /* The chord arpeggio (live_arp_set): its settings, per preset like the knobs' (live_edit; a
     * preset load turns it off); the notes held, in the order played and sorted, and how many;
     * the voice it plays (LIVE_ARP_NO_VOICE while no note is held); its place in the cycle; the
     * ticks, and with a clock rate the MIDI clocks, to the next step; and the ticks a MIDI clock
     * still counts as running. */
    uint8_t arp_on;
    uint8_t arp_rate;
    uint8_t arp_order;
    uint8_t arp_span;
    uint8_t arp_held[LIVE_ARP_HELD];
    uint8_t arp_sorted[LIVE_ARP_HELD];
    uint8_t arp_n;
    uint8_t arp_voice;
    uint8_t arp_pos;
    uint8_t arp_wait;
    uint8_t arp_clocks;
    uint8_t arp_clock_seen;
    /* The sound's fields a preset sets and nothing else did until LAB (the preset's, copied by a
     * load; live_tick, the allocator and live_freq_at read these): the pulse-width LFO's depth
     * (pulse-width steps, 0 off) and its phase step a tick (65536 a cycle); the sync ratio's
     * base in 1/256 (sync_q8 is it times the semitones' ratio); 1 for SYNC LEAD's one-voice sync;
     * the fine tune in cents, -LIVE_TUNE_CENTS..LIVE_TUNE_CENTS, in every voice's pitch (0 on
     * every built-in). */
    uint16_t pwm_depth;
    uint16_t pwm_rate;
    uint16_t sync_base_q8;
    uint8_t mono_sync;
    int8_t tune;
    /* The USER sounds (app/user_sound.h keeps them on the store): bit n, USER n + 1 holds one;
     * and the current sound's: 0 a built-in or DICE, n + 1 USER n + 1. */
    uint16_t sounds_valid;
    uint8_t user_sound;
    /* The current sound's base (lab_sound.h): the sound as loaded (a built-in's from its preset,
     * with the slots as they were; DICE's from its roll) or as last kept, and its name. The
     * edited mark `*` is the sound differing from it (live_preset_edited). */
    uint8_t base[LAB_SOUND_BYTES];
    char base_name[LAB_SOUND_NAME + 1u];
    /* PRESETS past DICE (app/user_sound.h sets the hook): user_load loads USER sound n (0..15) as
     * the current sound, 1 if it did. user_edit is the edit of the USER sound last left while it
     * differed from its base (the sound as edited, for USER user_edit_slot - 1; 0: none), given
     * back when that sound is loaded again: one shared edit, not one a slot. */
    int (*user_load)(void* ctx, uint32_t n);
    void* user_ctx;
    uint8_t user_edit_slot;
    uint8_t user_edit[LAB_SOUND_BYTES];
    /* The sounds loaded so far (load_preset counts them; wraps): LAB ends its A/B and its undo
     * history when another sound loads (app/lab_view.h). Not saved. */
    uint8_t loads;
    /* The sound's own since 1.1 (lab_sound.h's last bytes): the filter LFO's depth, 0 (off) ..
     * LIVE_FILTER_LFO_MAX K1 detents each way at the PWM's rate; the macro links,
     * -LIVE_LINK_MAX..LIVE_LINK_MAX (0 none): the resonance following the cutoff, the PWM's depth
     * following the width. */
    uint8_t filter_lfo;
    int8_t reso_link;
    int8_t pwm_link;
} live;

/* live.other_profile for a chip not made yet (the views show "--"). */
#define LIVE_PROFILE_NONE 0xffu

/* The KEYS setting: which chip the keys play while a song plays on chip A. */
typedef enum {
    LIVE_KEYS_AUTO = 0, /* chip B while a song plays, else chip A (the default) */
    LIVE_KEYS_A = 1     /* chip A always: during a song a key takes a voice from it */
} live_keys_mode;

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
    /* A live_envelope_mode, 0..1: one write (LIVE_REG_GATE_RESET), for the gates after it. */
    LIVE_PARAM_ENVELOPE_MODE = 36,
    /* The chip's character (sid_character.h), recorded; live_tick applies it. The profile,
     * 0..SID_CHIPS - 1 (6581 R2, 6581 R3, 6581 R4AR, 8580 R5, worn 6581), sets the chip
     * model too (a reset, as LIVE_PARAM_MODEL); SID_CHIPS.. is USER 1..8: the slot's base
     * profile and controls, set as a profile change and four control changes (an empty slot
     * does nothing). The model sets the 8580 R5 profile, or back to the last 6581 one. The
     * controls, 0..127 each, change the playing chip without a reset. */
    LIVE_PARAM_PROFILE = 37,
    LIVE_PARAM_AGE = 38,
    LIVE_PARAM_DAC = 39,
    LIVE_PARAM_THUMP = 40,
    LIVE_PARAM_BLEED = 41,
    LIVE_PARAM_EXPERT = 42,
    /* The sound's own, after the experts (LAB; LIVE_PARAM_EXPERT_END stays the experts' end):
     * the pulse-width LFO's depth, 0..LIVE_PWM_DEPTH_MAX pulse-width steps each way (0 off); its
     * rate, 0..255 in 0.05 Hz (live_pwm_step: PWM BASS's 10 is 0.5 Hz); and the fine tune,
     * 0..100 for -50..+50 cents (50 in tune). Per preset, as the expression ones; not in the
     * settings record (its learned CCs are). */
    LIVE_PARAM_PWM_DEPTH = LIVE_PARAM_EXPERT + SID_EXPERT_COUNT,
    LIVE_PARAM_PWM_RATE,
    LIVE_PARAM_TUNE,
    /* 1.1's: the filter LFO's depth, 0..LIVE_FILTER_LFO_MAX K1 detents each way, at the PWM's
     * rate (live.filter_lfo). */
    LIVE_PARAM_FILTER_LFO,
    LIVE_PARAM_COUNT
} live_param;

/* One past the last slot parameter, i.e. LIVE_PARAM_BEND_RANGE (live.c asserts it). A macro,
 * not an enumerator: the enum gives each value one name, so a switch over live_param cannot
 * meet the same value under two case labels. */
#define LIVE_PARAM_SLOT_END (LIVE_PARAM_SLOT + LIVE_SLOTS * LIVE_SLOT_FIELDS)

/* Code that runs once a turn or a redraw, never a tick: built for size where the compiler can say
 * so (clang's minsize; the device's compiler and the host's). */
#if defined(__clang__)
#define LIVE_COLD __attribute__((minsize))
#else
#define LIVE_COLD
#endif
#define LIVE_PARAM_EXPERT_END (LIVE_PARAM_EXPERT + SID_EXPERT_COUNT)
/* The parameters past the experts: LIVE_PARAM_PWM_DEPTH .. LIVE_PARAM_COUNT - 1. */
#define LIVE_PARAM_TAIL (LIVE_PARAM_COUNT - LIVE_PARAM_EXPERT_END)
/* LIVE_PARAM_PWM_RATE's value (0..LIVE_PWM_RATE_MAX, 0.05 Hz) as the LFO's phase step a tick,
 * round(rate * 65536 * 0.05 / LIVE_TICK_HZ), and a phase step back as the nearest rate. */
uint32_t live_pwm_step(uint32_t rate);
uint32_t live_pwm_rate(uint32_t step);

/* Preset 0, octave 0, full volume, every effect slot off, the envelope instant, the model's
 * default profile (6581 R4AR, 8580 R5) and every character control at 0. Sends the
 * character's settings, LIVE_REG_RESET(model), LIVE_REG_GATE_RESET, then every register,
 * then every slot setting (fx_live.h). */
void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model);
/* Voice v's control register as live writes it (waveform, ring, sync and the gate). */
uint8_t live_voice_control(const live* l, uint32_t v);
/* Voice v's registers again (envelope, frequency, pulse width, control) and the filter
 * routing: a voice a UI sound gives back (app/ui_sound.h). */
void live_rewrite_voice(live* l, uint32_t v);
/* Every register again, the gate reset first (a chip the keys have just moved to): a voice
 * the keys hold gates again on it. The character and the slots are not registers: the chip
 * has the keys' setup already (app/keys_route.h). */
void live_rewrite(live* l);
/* The KEYS setting (a live_keys_mode; anything else is AUTO). Not a MIDI parameter. */
void live_set_keys_mode(live* l, uint32_t mode);
/* The THEME setting (a THEME_* of app/theme.h, LIVE_THEMES of them; anything else is the default,
 * HIGH CONTRAST). Not a MIDI parameter. */
#define LIVE_THEMES 2u
void live_set_theme(live* l, uint32_t theme);
/* From now on a model or profile change waits for prepare (above); NULL: none, as after
 * live_init. A preparation already asked for is dropped. */
void live_set_preparer(live* l, live_prepare_fn prepare, void* ctx);
/* Absolute MIDI notes; the octave is not applied here (see live_key_note). Velocity 0
 * is a note-off. A note that is already held is retriggered on its voice. The note starts at
 * the current bend, with its pressure at 0 and its vibrato at the start of the delay; its
 * velocity applies at once (the sustain level before the gate). With the chord arpeggio on
 * (live_arp_set) a note joins its cycle instead (the first starts its voice) and a note-off
 * leaves it (the last releases the voice). */
void live_note_on(live* l, uint8_t note, uint8_t velocity);
void live_note_off(live* l, uint8_t note);
void live_all_notes_off(live* l);
/* The MIDI note for a key whose note at octave 0 is base_note, or LIVE_NO_NOTE.
 * The device must keep the note it got at key-down and send that same note at key-up:
 * the octave may change while the key is held. */
uint8_t live_key_note(const live* l, uint8_t base_note);
void live_knob(live* l, live_knob_id knob, int32_t delta);
/* The place on K1's walk of cutoff c (0..LIVE_CUTOFF_MAX) on the chip's model: the last entry of
 * live_cutoff_steps at or below it (0 below the first). */
uint32_t live_cutoff_place(const live* l, uint32_t c);
/* What K3 turns now (a live_tone_kind): RATIO on a mono-sync preset, else PULSE while the waveform
 * has the pulse bit, else DECAY. */
uint32_t live_tone(const live* l);
/* round(65536 * 2^((k - 12) / 12)) for k = 0..24: the sync ratio's semitones (live.c). */
extern const uint32_t live_ratio_q16[2 * LIVE_RATIO_SEMIS + 1];
/* PRESETS: steps the sound a detent each (clockwise the next), clamped at both ends: the
 * built-ins and DICE as live_recall_preset does, then on past DICE into the USER sounds that hold
 * one (live.sounds_valid; the empty slots are skipped; live.user_load loads one and the edit it
 * was left with comes back, live.user_edit). Anticlockwise goes the same way back. DICE with no
 * USER sound after it rolls again at the end, as it always has. Revert (EDIT held) puts the
 * current sound back to its base instead: a built-in to its factory settings (live_set_preset,
 * which forgets its kept edits too), a USER sound to the sound as kept. */
void live_preset_step(live* l, int32_t steps, int revert);
/* Goes to preset q (clamped) keeping what it was: the current preset's edits are stored, q's
 * factory settings loaded and q's stored edits (if it has any) applied over them. live_set_preset
 * (a MIDI program change, the settings) is the factory's: it loads q's settings over any edits. */
void live_recall_preset(live* l, uint8_t q);
/* 1 when the current sound differs from its base (live.base: the preset as loaded, the effect
 * slots included; a bend range that RPN 0 set does not count). Never on DICE, whose every turn is
 * a new sound. */
uint32_t live_preset_edited(const live* l);
/* live_preset_edited with the sound already packed (live_sound_get's now). */
uint32_t live_sound_edited(const live* l, const uint8_t now[LAB_SOUND_BYTES]);
/* The current sound's name (live.base_name) into buf (at least LIVE_PRESET_NAME_CHARS bytes), then
 * `*` while live_preset_edited; DICE's with its roll's number. */
#define LIVE_PRESET_NAME_CHARS 12u
void live_preset_name(const live* l, char* buf);
/* What live_set_preset sets for preset `index` (live_set_preset's loaded parameters; the others of
 * out[] are not written: the experts' and the rest), as live_param_get would read them: for MIDI
 * out, whose program change carries them. */
void live_preset_params(uint8_t index, uint16_t out[LIVE_PARAM_COUNT]);
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
 * most once a tick and only when it differs from the chip's (with a preparer, once the chip
 * for it is whole); live_button's toggle applies at once without a preparer. */
void live_param_set(live* l, uint32_t param, uint32_t value);
/* A parameter's value; 0 for an unknown one. For the chip model, the one last asked for
 * (model_req), so a set reads back at once; the chip's is the model field. */
uint32_t live_param_get(const live* l, uint32_t param);
/* Every parameter's value at once, out[p] as live_param_get(l, p): straight-line, for MIDI out,
 * which compares them all several times a tick. */
void live_params(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
/* The original parameter prefix only, without resolving or copying the expert layer. */
void live_params_base(const live* l, uint16_t out[LIVE_PARAM_EXPERT]);
/* The parameters past the experts, into out[LIVE_PARAM_EXPERT_END..LIVE_PARAM_COUNT - 1] (the
 * others of out[] are not written). */
void live_params_tail(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
/* MIDI snapshots keep 255 for macro-following experts: the macro carries that change,
 * so it must not also emit a burst of independent NRPN edits that would freeze its path. */
void live_params_wire(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
/* RPN 0, pitch-bend sensitivity: sets the bend range (clamped to 0..LIVE_BEND_RANGE_MAX) as
 * the channel's setting, which program changes keep (RP-015 makes RPN values channel state)
 * until power-off. Setting LIVE_PARAM_BEND_RANGE (NRPN 6/0) is a preset edit and ends that:
 * of RPN 0 and NRPN 6/0 the last one wins, and only RPN 0's survives a program change. */
void live_bend_range_rpn(live* l, uint32_t semitones);
/* Loads a preset: releases every voice and resets cutoff, resonance and pulse width, the
 * five SID settings the parameters edit (waveform with ring and sync, attack/decay,
 * sustain/release, filter routing, filter mode), the expression settings (bend range,
 * targets, vibrato) and the sound's own (the PWM, the fine tune, the sync), to the preset's;
 * the base (live.base) is the preset so loaded. Loading the preset already loaded does the same, so it
 * discards those edits. Volume, octave, chip model, envelope mode, the effect slots, the
 * controllers (bend, wheel, pressure) and a bend range set by RPN 0 are kept. An index past
 * the last is clamped. */
void live_set_preset(live* l, uint8_t index);
/* The MASTER pot, 0..1023 (clamped). It scales the final output digitally (app/master_gain.h,
 * applied by the audio path after the chips and the effect slots, from l->master): the
 * chip's volume register stays at LIVE_CHIP_VOLUME whatever it is. l->volume, the pot's
 * travel in percent (0..100, rounded), is only for display. */
void live_set_master(live* l, uint16_t value);
/* Applies a chip model or profile asked for since the last tick (live_param_set; with a
 * preparer, one slice of its preparation, and the reset once it is whole), sends the
 * character's controls that changed and LIVE_REG_CHAR_TICK, advances
 * the pulse-width LFO and each voice's vibrato, then writes every frequency, pulse width and
 * the cutoff that the controllers, the vibrato or the LFO changed (nothing that did not
 * change); call at LIVE_TICK_HZ. At most 14 writes besides a model change and the
 * character's. */
void live_tick(live* l);

/* The chord arpeggio's field (live_arp_field), clamped. ON with notes held takes them into one
 * voice (the oldest held, its gate kept; the others released); OFF gives each held note a voice
 * again (a new attack). A rate's change restarts the step's count; an order's or a span's keeps
 * the note that sounds where the new cycle has it. */
void live_arp_set(live* l, uint32_t field, uint32_t value);
/* The rate, the order or the span turned by detents, clamped at both ends. */
void live_arp_turn(live* l, uint32_t field, int32_t detents);
/* A MIDI real-time byte (0xF8 clock, 0xFA start): a clock rate steps on the clocks while they
 * come; a start puts the next step a whole step away. Others do nothing. */
void live_arp_realtime(live* l, uint8_t status);

/* The sound (lab_sound.h). live_sound_get packs the current sound into out. live_sound_apply sets
 * in's on the instrument as a MIDI edit would, every field through its setter (live_param_set,
 * live_arp_set, live_slot_set: the registers, the arpeggio and the slots written as they change;
 * a change of the one-voice sync releases the notes first), clamped; the base stays.
 * live_sound_set_base makes in the base, and name (up to LAB_SOUND_NAME characters) its name
 * unless NULL: nothing sounds different, only `*` may. in is checked by lab_sound_valid first
 * where it comes from outside (the store). */
void live_sound_get(const live* l, uint8_t out[LAB_SOUND_BYTES]);
void live_sound_apply(live* l, const uint8_t in[LAB_SOUND_BYTES]);
void live_sound_set_base(live* l, const uint8_t in[LAB_SOUND_BYTES], const char* name);
/* USER sound n (0..15; app/user_sound.h reads it from the store) loaded as a preset is: every
 * voice released, the instrument's settings and the controllers kept, the sound set, its slots
 * included, as its base under its name; live.user_sound n + 1. live.preset is SAW LEAD's, the
 * frame a USER sound loads over. */
void live_load_user(live* l, uint32_t n, const uint8_t sound[LAB_SOUND_BYTES], const char* name);

/* Effect slots. The number of algorithms slot has (its NONE included). */
uint32_t live_slot_algorithms(uint32_t slot);
/* Sets a slot's field: an algorithm clamped to the slot's, a control to 0..LIVE_SLOT_MAX.
 * Sends it when it changed. */
void live_slot_set(live* l, uint32_t slot, uint32_t field, int32_t value);
/* Turns a slot control by detents (LIVE_SLOT_DETENT steps each), clamped at both ends. */
void live_slot_turn(live* l, uint32_t slot, uint32_t field, int32_t detents);
/* Amount, A, B and Mix (0..127) an algorithm starts from when picked from the panel
 * (app/fx_defaults.c): [slot][algorithm]; NONE's row is zeros. */
extern const uint8_t live_fx_defaults[LIVE_SLOTS][8][4];
/* Picks a slot's algorithm from the panel. A slot still silent (Amount and Mix 0) or still at
 * the controls of the algorithm it has (untouched since it was picked) takes the new
 * algorithm's defaults with it, unless that is NONE; an edited slot keeps its controls. MIDI's
 * algorithm CC (live_param_set) loads nothing: it is live_slot_set. */
void live_slot_pick(live* l, uint32_t slot, uint32_t algorithm);
/* The slot's next algorithm (live_slot_pick), wrapping from the last to NONE. */
void live_slot_next_algorithm(live* l, uint32_t slot);
/* The panel's slot: the next one, wrapping. */
void live_fx_select_next(live* l);
/* The panel's slot: slot (wrapped into 0..LIVE_SLOTS - 1). */
void live_fx_select(live* l, uint32_t slot);

#ifdef __cplusplus
}
#endif

#endif
