// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_SLOTS_H
#define SIDOU_FX_SLOTS_H

#include <stddef.h>
#include <stdint.h>

#include "fx_chorus.h"
#include "fx_crt.h"
#include "fx_crush.h"
#include "fx_echo.h"
#include "fx_flanger.h"
#include "fx_fuzz.h"
#include "fx_noise.h"
#include "fx_overdrive.h"
#include "fx_phaser.h"
#include "fx_plate.h"
#include "fx_reverb.h"
#include "fx_tape.h"
#include "fx_tremolo.h"
#include "fx_wow.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_SLOT_FADE_MS 20.0f
#define SID_SLOT_POOL_LEN 65536u

enum { SID_SLOT_DIST, SID_SLOT_MOD, SID_SLOT_FX, SID_SLOT_COUNT };

#define SID_SLOT_DIST_GAIN 4.0f

enum {
    SID_DIST_NONE,
    SID_DIST_TAPE,
    SID_DIST_OVERDRIVE,
    SID_DIST_FUZZ,
    SID_DIST_CRUSH,
    SID_DIST_LOFI,
    SID_DIST_COUNT
};

enum {
    SID_MOD_NONE,
    SID_MOD_CHORUS,
    SID_MOD_FLANGER,
    SID_MOD_PHASER,
    SID_MOD_TREMOLO,
    SID_MOD_AUTOPAN,
    SID_MOD_WOW,
    SID_MOD_COUNT
};

enum {
    SID_FXSLOT_NONE,
    SID_FXSLOT_DELAY,
    SID_FXSLOT_PINGPONG,
    SID_FXSLOT_REVERB_LIGHT,
    SID_FXSLOT_REVERB_LUSH,
    SID_FXSLOT_COUNT
};

typedef struct {
    uint32_t algorithm;
    float amount, a, b, mix;
} sid_slot_params;

#define SID_SLOT_WAKING 0xffffffffu

enum { SID_SLOT_RUN, SID_SLOT_FADE_OUT, SID_SLOT_CLEAR, SID_SLOT_FADE_IN };

typedef union {
    int16_t s16[SID_SLOT_POOL_LEN];
    float f32[SID_SLOT_POOL_LEN / 2u];
} sid_slot_pool;

typedef struct {
    sid_slot_params target;
    float amount, a, b, mix;
    float inc[4];
    uint32_t algorithm;
    uint32_t phase;
    uint32_t awake;
    uint32_t clear_pos;
    uint32_t warm;
    uint32_t may_switch;
    float fade;
} sid_slot;

typedef struct {
    sid_fx_crt crt;
    sid_fx_noise noise;
} sid_slot_lofi;

typedef union {
    sid_fx_tape tape;
    sid_fx_overdrive overdrive;
    sid_fx_fuzz fuzz;
    sid_fx_crush crush;
    sid_slot_lofi lofi;
} sid_slot_dist_state;

typedef union {
    sid_fx_chorus chorus;
    sid_fx_flanger flanger;
    sid_fx_phaser phaser;
    sid_fx_tremolo tremolo;
    sid_fx_wow wow;
} sid_slot_mod_state;

typedef union {
    sid_fx_echo echo;
    sid_fx_reverb reverb;
    sid_fx_plate plate;
} sid_slot_fx_state;

#define SID_SLOT_TAIL(type, first) (sizeof(type) - offsetof(type, first))
#define SID_SLOT_ALGS 8u

typedef struct {
    uint16_t dst, src, n, clear;
} sid_slot_start;

typedef struct {
    sid_fx_tape tape;
    sid_fx_overdrive overdrive;
    sid_fx_fuzz fuzz;
    sid_fx_crush crush;
    sid_slot_lofi lofi;
    sid_fx_phaser phaser;
    sid_fx_tremolo tremolo;
    sid_fx_echo delay, pingpong;
    sid_fx_reverb reverb;
    sid_fx_plate plate;
    unsigned char chorus[SID_SLOT_TAIL(sid_fx_chorus, line)];
    unsigned char flanger[SID_SLOT_TAIL(sid_fx_flanger, line)];
    unsigned char wow[SID_SLOT_TAIL(sid_fx_wow, line)];
    uint16_t warm[SID_SLOT_COUNT][SID_SLOT_ALGS];
} sid_slot_templates;

typedef struct {
    sid_slot slot[SID_SLOT_COUNT];
    uint32_t sample_rate;
    uint32_t seed;
    uint32_t clock;
    float smooth_ahead, fade_step;
    uint32_t clear_per_sample;
    sid_slot_pool* pool;
    sid_slot_dist_state dist;
    sid_slot_mod_state mod;
    sid_slot_fx_state fx;
    sid_slot_templates tpl;
} sid_slots;

void sid_slots_init(sid_slots* s, uint32_t sample_rate, uint32_t seed, sid_slot_pool* pool);
void sid_slots_set(sid_slots* s, uint32_t slot, const sid_slot_params* p);
void sid_slots_process(sid_slots* s, const int16_t* in, int16_t* out, uint32_t n);
void sid_slots_process_wide(sid_slots* s, const int16_t* in, const int16_t* over, int16_t* out,
                            uint32_t n);
void sid_slots_process_float(sid_slots* s, const int16_t* in, const int16_t* over, float* out,
                             uint32_t n);
float sid_slots_dry(const sid_slots* s);

#ifdef __cplusplus
}
#endif

#endif
