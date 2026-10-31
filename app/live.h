// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
#define LIVE_REG_RESET 0x80u
#define LIVE_REG_GATE_RESET 0x81u
#define LIVE_CHAR_FIELDS 5u
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
#define LIVE_PULSE_FIRST 128
#define LIVE_PULSE_STEP 64
#define LIVE_PULSE_POSITIONS 31
#define LIVE_RATIO_SEMIS 12
#define LIVE_MASTER_MAX 1023u
#define LIVE_CHIP_VOLUME 15u
#define LIVE_TICK_HZ 100u
#define LIVE_PWM_DEPTH_MAX 2048u
#define LIVE_PWM_STEP_MAX 8356u
#define LIVE_TUNE_CENTS 50
#define LIVE_PWM_RATE_MAX 255u
#define LIVE_FILTER_LFO_MAX 32u
#define LIVE_LINK_MAX 8

#define LIVE_PITCH_SEMITONE 8192
#define LIVE_BEND_CENTRE 8192u
#define LIVE_BEND_MAX 16383u
#define LIVE_BEND_RANGE_MAX 24u
#define LIVE_BEND_RANGE_DEFAULT 2u
#define LIVE_EXPRESSION_MAX 127u
#define LIVE_VIBRATO_RATE_DEFAULT 60u

#define LIVE_CC_PULSE_WIDTH 1u
#define LIVE_CC_MOD_WHEEL 1u
#define LIVE_CC_VOLUME 7u
#define LIVE_CC_RESONANCE 71u
#define LIVE_CC_CUTOFF 74u
#define LIVE_CC_VIBRATO_RATE 76u
#define LIVE_CC_VIBRATO_DEPTH 77u
#define LIVE_CC_VIBRATO_DELAY 78u
#define LIVE_CC_SLOT_FIRST 102u
#define LIVE_CC_SLOT_LAST 116u
#define LIVE_CC_ARP 27u
#define LIVE_CC_ARP_RATE 28u
#define LIVE_CC_ARP_ORDER 29u
#define LIVE_CC_ARP_SPAN 30u
#define LIVE_CC_FILTER_LFO 31u
#define LIVE_CC_ALL_SOUND_OFF 120u
#define LIVE_CC_RESET_ALL 121u
#define LIVE_CC_ALL_NOTES_OFF 123u

#define LIVE_SLOTS 3u
#define LIVE_SLOT_FIELDS 5u
#define LIVE_SLOT_MAX 127u
#define LIVE_SLOT_DETENTS 32u

typedef enum {
    LIVE_SLOT_ALGORITHM = 0,
    LIVE_SLOT_AMOUNT = 1,
    LIVE_SLOT_A = 2,
    LIVE_SLOT_B = 3,
    LIVE_SLOT_MIX = 4
} live_slot_field;

#define LIVE_ARP_HELD 8u
#define LIVE_ARP_RATES 9u
#define LIVE_ARP_RATE_CLOCK 6u
#define LIVE_ARP_RATE_DEFAULT 2u
#define LIVE_ARP_CLOCK_TICKS 30u
#define LIVE_ARP_SPAN_MAX 2u
#define LIVE_ARP_NO_VOICE 0xffu
#define LIVE_ARP_TICKS_PER_FRAME 2u

typedef enum {
    LIVE_ARP_UP = 0,
    LIVE_ARP_DOWN = 1,
    LIVE_ARP_UP_DOWN = 2,
    LIVE_ARP_PLAYED = 3
} live_arp_order;

typedef enum {
    LIVE_ARP_ON = 0,
    LIVE_ARP_RATE = 1,
    LIVE_ARP_ORDER = 2,
    LIVE_ARP_SPAN = 3
} live_arp_field;

extern const uint8_t live_arp_steps[LIVE_ARP_RATES];

typedef enum {
    LIVE_VELOCITY_OFF = 0,
    LIVE_VELOCITY_SUSTAIN = 1,
    LIVE_VELOCITY_CUTOFF = 2,
    LIVE_VELOCITY_PULSE_WIDTH = 3
} live_velocity_target;

typedef enum {
    LIVE_ENVELOPE_INSTANT = 0,
    LIVE_ENVELOPE_AUTHENTIC = 1
} live_envelope_mode;

typedef enum {
    LIVE_MOD_PULSE_WIDTH = 0,
    LIVE_MOD_VIBRATO = 1,
    LIVE_MOD_CUTOFF = 2
} live_mod_target;

typedef enum {
    LIVE_PRESSURE_OFF = 0,
    LIVE_PRESSURE_VIBRATO = 1,
    LIVE_PRESSURE_PULSE_WIDTH = 2,
    LIVE_PRESSURE_CUTOFF = 3
} live_pressure_target;

typedef void (*live_sink_fn)(void* ctx, uint8_t reg, uint8_t value);
typedef int (*live_prepare_fn)(void* ctx, uint8_t profile);
#define LIVE_PREPARE_CANCEL 0xffu

typedef struct {
    sid* chip;
    uint32_t clock_hz;
    uint32_t sample_rate;
    sid_character_ramp ramp;
    sid_dac* dacs;
    sid* spare;
    sid_character_prep prep;
    uint8_t prep_on;
    uint8_t prep_profile;
    uint8_t prep_tick;
    uint8_t default_8580_spent;
} live_sid_target;

void live_sid_sink(void* ctx, uint8_t reg, uint8_t value);
int live_sid_prepare(void* ctx, uint8_t profile);
void live_char_store(sid_character* c, uint8_t reg, uint8_t value);

typedef enum {
    LIVE_KNOB_CUTOFF = 0,
    LIVE_KNOB_RESONANCE = 1,
    LIVE_KNOB_TONE = 2,
    LIVE_KNOB_RELEASE = 3
} live_knob_id;

typedef enum {
    LIVE_TONE_RATIO = 0,
    LIVE_TONE_PULSE = 1,
    LIVE_TONE_DECAY = 2
} live_tone_kind;

typedef enum {
    LIVE_BUTTON_OCT_DOWN = 0,
    LIVE_BUTTON_OCT_UP = 1,
    LIVE_BUTTON_FX = 2
} live_button_id;

typedef struct {
    const char* name;
    uint8_t control;
    uint8_t attack_decay;
    uint8_t sustain_release;
    uint16_t pulse_width;
    uint16_t cutoff;
    uint8_t resonance;
    uint8_t filter_voices;
    uint8_t filter_mode;
    uint8_t mono_sync;
    uint16_t sync_ratio_q8;
    uint16_t pwm_depth;
    uint16_t pwm_rate;
    uint8_t bend_range;
    uint8_t velocity_target;
    uint8_t mod_target;
    uint8_t pressure_target;
    uint8_t vibrato_rate;
    uint8_t vibrato_depth;
    uint8_t vibrato_delay;
} live_preset;

#define LIVE_PRESET_COUNT 4u
#define LIVE_PRESET_SLOTS 5u
#define LIVE_PRESET_DICE 4u
#define LIVE_SOUNDS 16u
const live_preset* live_preset_get(uint8_t index);

extern const uint16_t live_freq_table[128];
extern const uint32_t live_freq_top_q4[12];
extern const uint32_t live_freq_fine_q20[65];

uint16_t live_freq_at(uint8_t note, int32_t offset);

#define LIVE_CUTOFF_STEPS 65u
extern const uint16_t live_cutoff_steps[2][LIVE_CUTOFF_STEPS];
extern const int8_t live_cutoff_up[2][64];
extern const int8_t live_cutoff_down[2][64];

typedef struct {
    uint8_t note;
    uint8_t gate;
    uint8_t velocity;
    uint8_t pressure;
    uint32_t stamp;
    uint16_t vibrato_phase;
    uint8_t vibrato_wait;
    uint16_t freq_out;
    uint16_t pw_out;
} live_voice;

typedef struct {
    uint16_t cutoff, pulse_width;
    uint8_t resonance, control, attack_decay, sustain_release, filter_voices, filter_mode;
    uint8_t bend_range, velocity_target, mod_target, pressure_target;
    uint8_t vibrato_rate, vibrato_depth, vibrato_delay;
    int8_t sync_semis;
    uint8_t arp_on, arp_rate, arp_order, arp_span;
    uint8_t valid;
    uint16_t pwm_depth, pwm_rate;
    int8_t tune;
    uint8_t filter_lfo;
    int8_t reso_link, pwm_link;
} live_edit;

typedef struct {
    live_sink_fn sink;
    void* sink_ctx;
    sid_model model;
    sid_model model_req;
    uint8_t preset;
    int8_t octave;
    uint16_t cutoff;
    uint8_t resonance;
    uint16_t pulse_width;
    uint16_t master;
    uint8_t volume;
    uint16_t cutoff_out;
    uint16_t lfo_phase;
    uint32_t clock;
    live_voice voice[LIVE_VOICES];
    uint8_t slot[LIVE_SLOTS][LIVE_SLOT_FIELDS];
    uint8_t fx_select;
    uint8_t control;
    uint8_t attack_decay;
    uint8_t sustain_release;
    uint8_t filter_voices;
    uint8_t filter_mode;
    uint8_t bend_range;
    uint8_t bend_range_rpn;
    uint8_t velocity_target;
    uint8_t mod_target;
    uint8_t pressure_target;
    uint8_t vibrato_rate;
    uint8_t vibrato_depth;
    uint8_t vibrato_delay;
    uint16_t bend;
    uint8_t mod_wheel;
    uint8_t channel_pressure;
    uint8_t last_velocity;
    uint32_t note_ons;
    uint8_t envelope_mode;
    uint8_t profile;
    uint8_t profile_req;
    uint8_t profile_6581;
    uint8_t character[LIVE_CHAR_FIELDS - 1u];
    uint8_t character_sent[LIVE_CHAR_FIELDS - 1u];
    live_prepare_fn prepare;
    void* prepare_ctx;
    uint8_t preparing;
    uint8_t two_chips;
    uint8_t keys_chip;
    uint8_t other_model;
    uint8_t other_profile;
    uint8_t keys_mode;
    uint8_t ui_voice;
    uint8_t settings_dirty;
    uint8_t user_valid;
    uint8_t user_applied;
    uint8_t user_base[LIVE_USER_PROFILES];
    uint8_t user_ctl[LIVE_USER_PROFILES][LIVE_CHAR_FIELDS - 1u];
    uint8_t theme;
    uint8_t spread;
    uint8_t expert[SID_EXPERT_COUNT];
    uint8_t expert_sent[SID_EXPERT_COUNT];
    uint8_t user_expert[LIVE_USER_PROFILES][SID_EXPERT_COUNT];
    uint8_t expert_dirty;
    uint8_t expert_active;
    uint32_t expert_version;
    uint8_t cutoff_pos;
    int8_t sync_semis;
    uint16_t sync_q8;
    live_edit edits[LIVE_PRESET_SLOTS];
    uint8_t dice_rolls;
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
    uint16_t pwm_depth;
    uint16_t pwm_rate;
    uint16_t sync_base_q8;
    uint8_t mono_sync;
    int8_t tune;
    uint16_t sounds_valid;
    uint8_t user_sound;
    uint8_t base[LAB_SOUND_BYTES];
    char base_name[LAB_SOUND_NAME + 1u];
    int (*user_load)(void* ctx, uint32_t n);
    void* user_ctx;
    uint8_t user_edit_slot;
    uint8_t user_edit[LAB_SOUND_BYTES];
    uint8_t loads;
    uint8_t filter_lfo;
    int8_t reso_link;
    int8_t pwm_link;
    uint8_t res_out;
} live;

#define LIVE_PROFILE_NONE 0xffu

typedef enum {
    LIVE_KEYS_AUTO = 0,
    LIVE_KEYS_A = 1
} live_keys_mode;

typedef enum {
    LIVE_PARAM_CUTOFF = 0,
    LIVE_PARAM_RESONANCE = 1,
    LIVE_PARAM_PULSE_WIDTH = 2,
    LIVE_PARAM_VOLUME = 3,
    LIVE_PARAM_WAVEFORM = 4,
    LIVE_PARAM_RING = 5,
    LIVE_PARAM_SYNC = 6,
    LIVE_PARAM_ATTACK = 7,
    LIVE_PARAM_DECAY = 8,
    LIVE_PARAM_SUSTAIN = 9,
    LIVE_PARAM_RELEASE = 10,
    LIVE_PARAM_FILTER_MODE = 11,
    LIVE_PARAM_FILTER_VOICES = 12,
    LIVE_PARAM_MODEL = 13,
    LIVE_PARAM_SLOT = 14,
    LIVE_PARAM_BEND_RANGE = 29,
    LIVE_PARAM_VELOCITY_TARGET = 30,
    LIVE_PARAM_MOD_TARGET = 31,
    LIVE_PARAM_PRESSURE_TARGET = 32,
    LIVE_PARAM_VIBRATO_RATE = 33,
    LIVE_PARAM_VIBRATO_DEPTH = 34,
    LIVE_PARAM_VIBRATO_DELAY = 35,
    LIVE_PARAM_ENVELOPE_MODE = 36,
    LIVE_PARAM_PROFILE = 37,
    LIVE_PARAM_AGE = 38,
    LIVE_PARAM_DAC = 39,
    LIVE_PARAM_THUMP = 40,
    LIVE_PARAM_BLEED = 41,
    LIVE_PARAM_EXPERT = 42,
    LIVE_PARAM_PWM_DEPTH = LIVE_PARAM_EXPERT + SID_EXPERT_COUNT,
    LIVE_PARAM_PWM_RATE,
    LIVE_PARAM_TUNE,
    LIVE_PARAM_FILTER_LFO,
    LIVE_PARAM_ARP_RATE,
    LIVE_PARAM_RESO_LINK,
    LIVE_PARAM_PWM_LINK,
    LIVE_PARAM_COUNT
} live_param;

#define LIVE_PARAM_SLOT_END (LIVE_PARAM_SLOT + LIVE_SLOTS * LIVE_SLOT_FIELDS)

#if defined(__clang__)
#define LIVE_COLD __attribute__((minsize))
#else
#define LIVE_COLD
#endif
#define LIVE_PARAM_EXPERT_END (LIVE_PARAM_EXPERT + SID_EXPERT_COUNT)
#define LIVE_PARAM_TAIL (LIVE_PARAM_COUNT - LIVE_PARAM_EXPERT_END)
uint32_t live_pwm_step(uint32_t rate);
uint32_t live_pwm_rate(uint32_t step);

void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model);
uint8_t live_voice_control(const live* l, uint32_t v);
void live_rewrite_voice(live* l, uint32_t v);
void live_rewrite(live* l);
void live_set_keys_mode(live* l, uint32_t mode);
#define LIVE_THEMES 2u
void live_set_theme(live* l, uint32_t theme);
void live_set_preparer(live* l, live_prepare_fn prepare, void* ctx);
void live_note_on(live* l, uint8_t note, uint8_t velocity);
void live_note_off(live* l, uint8_t note);
void live_all_notes_off(live* l);
uint8_t live_key_note(const live* l, uint8_t base_note);
void live_knob(live* l, live_knob_id knob, int32_t delta);
uint32_t live_cutoff_place(const live* l, uint32_t c);
uint32_t live_resonance_effective(const live* l);
uint32_t live_pwm_depth_effective(const live* l);
void live_links_update(live* l);
uint32_t live_tone(const live* l);

enum {
    LIVE_WALK_PWM_RATE,
    LIVE_WALK_PWM_DEPTH,
    LIVE_WALK_VIB_RATE,
    LIVE_WALK_VIB_DEPTH,
    LIVE_WALK_VIB_DELAY,
    LIVE_WALK_FX_LIN,
    LIVE_WALK_FX_LOG,
    LIVE_WALK_FX_EQP,
    LIVE_WALKS
};
#define LIVE_WALK_NONE LIVE_WALKS
uint32_t live_walk_count(uint32_t walk);
uint32_t live_walk_at(uint32_t walk, uint32_t i);
uint32_t live_walk_slot(uint32_t slot, uint32_t algorithm, uint32_t field);
extern const uint8_t live_walk_fx[3][32];
extern const uint8_t live_walk_shapes[LIVE_SLOTS][8];
uint32_t live_walk_of(const live* l, uint32_t param);
uint32_t live_walk_step(uint32_t walk, uint32_t value, int32_t detents);
void live_walk_turn(live* l, uint32_t param, int32_t detents);
uint32_t live_walk_from_cc(const live* l, uint32_t param, uint32_t cc);
uint8_t live_walk_to_cc(const live* l, uint32_t param, uint32_t value);
void live_walk_cc(live* l, uint32_t param, uint32_t cc);
extern const uint32_t live_ratio_q16[2 * LIVE_RATIO_SEMIS + 1];
void live_preset_step(live* l, int32_t steps, int revert);
void live_recall_preset(live* l, uint8_t q);
uint32_t live_preset_edited(const live* l);
uint32_t live_sound_edited(const live* l, const uint8_t now[LAB_SOUND_BYTES]);
#define LIVE_PRESET_NAME_CHARS 12u
void live_preset_name(const live* l, char* buf);
void live_preset_params(uint8_t index, uint16_t out[LIVE_PARAM_COUNT]);
void live_button(live* l, live_button_id button);
void live_cc(live* l, uint8_t cc, uint8_t value);

void live_bend(live* l, uint16_t bend);
void live_pressure(live* l, uint8_t pressure);
void live_poly_pressure(live* l, uint8_t note, uint8_t pressure);
void live_reset_controllers(live* l);
uint32_t live_param_max(uint32_t param);
void live_param_set(live* l, uint32_t param, uint32_t value);
uint32_t live_param_get(const live* l, uint32_t param);
void live_params(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
void live_params_base(const live* l, uint16_t out[LIVE_PARAM_EXPERT]);
void live_params_tail(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
void live_params_wire(const live* l, uint16_t out[LIVE_PARAM_COUNT]);
void live_bend_range_rpn(live* l, uint32_t semitones);
void live_set_preset(live* l, uint8_t index);
void live_set_master(live* l, uint16_t value);
void live_tick(live* l);

void live_arp_set(live* l, uint32_t field, uint32_t value);
void live_arp_turn(live* l, uint32_t field, int32_t detents);
void live_arp_realtime(live* l, uint8_t status);

void live_sound_get(const live* l, uint8_t out[LAB_SOUND_BYTES]);
void live_sound_apply(live* l, const uint8_t in[LAB_SOUND_BYTES]);
void live_sound_set_base(live* l, const uint8_t in[LAB_SOUND_BYTES], const char* name);
void live_load_user(live* l, uint32_t n, const uint8_t sound[LAB_SOUND_BYTES], const char* name);

uint32_t live_slot_algorithms(uint32_t slot);
void live_slot_set(live* l, uint32_t slot, uint32_t field, int32_t value);
void live_slot_turn(live* l, uint32_t slot, uint32_t field, int32_t detents);
extern const uint8_t live_fx_defaults[LIVE_SLOTS][8][4];
void live_slot_pick(live* l, uint32_t slot, uint32_t algorithm);
void live_slot_next_algorithm(live* l, uint32_t slot);
void live_fx_select_next(live* l);
void live_fx_select(live* l, uint32_t slot);

#ifdef __cplusplus
}
#endif

#endif
