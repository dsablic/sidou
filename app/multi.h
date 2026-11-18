// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MULTI_H
#define SIDOU_MULTI_H

#include <stdint.h>

#include "keys_route.h"
#include "lab_sound.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MULTI_PARTS 3u
#define MULTI_PROGRAM_KEYS LIVE_PRESET_COUNT
#define MULTI_PROGRAM_USER 16u
#define MULTI_PROGRAMS (MULTI_PROGRAM_USER + LIVE_SOUNDS)
#define MULTI_NO_NOTE 0xffu
#define MULTI_LOCK_CUTOFF 1u
#define MULTI_LOCK_RESONANCE 2u
#define MULTI_FILTER_UNSENT 0xffffu

typedef int (*multi_user_fn)(void* ctx, uint32_t slot, char name[LAB_SOUND_NAME + 1u],
                             uint8_t sound[LAB_SOUND_BYTES]);

typedef struct {
    uint8_t control, attack_decay, sustain_release, routed;
    uint8_t resonance, filter_mode, bend_range;
    uint8_t velocity_target, mod_target, pressure_target;
    uint8_t vibrato_rate, vibrato_depth, vibrato_delay, program;
    uint16_t pulse_width, cutoff, pwm_depth, pwm_rate;
    int32_t tune;
    uint8_t table[LAB_SOUND_TABLE_BYTES];
    char name[LAB_SOUND_NAME + 1u];
} multi_sound;

typedef struct {
    multi_sound s;
    uint8_t note, gate, velocity, pressure, wheel;
    uint8_t vibrato_wait, table_pos, table_wait;
    uint16_t pulse_width, vibrato_phase, lfo_phase, freq_out, pw_out;
    int32_t bend;
} multi_part;

typedef struct {
    multi_part p[MULTI_PARTS];
    live_sink_fn sink;
    void* sink_ctx;
    const live* l;
    const keys_route* kr;
    multi_user_fn user;
    void* user_ctx;
    void (*taking)(void* ctx, uint32_t taken);
    void* taking_ctx;
    uint8_t locks, resonance_cc, resonance_out, routed_out, mode_out;
    uint8_t song, tune, profile, table_frame, taken;
    uint16_t cutoff_cc, cutoff_out;
} multi;

void multi_init(multi* m, live_sink_fn sink, void* sink_ctx, const live* l, const keys_route* kr);
int multi_program(multi* m, uint32_t part, uint32_t program);
uint32_t multi_program_step(const multi* m, uint32_t program, int32_t steps);
void multi_note_on(multi* m, uint32_t part, uint8_t note, uint8_t velocity, uint32_t legato);
void multi_note_off(multi* m, uint32_t part);
void multi_bend(multi* m, uint32_t part, int32_t offset);
void multi_wheel(multi* m, uint32_t part, uint8_t value);
void multi_pressure(multi* m, uint32_t part, uint8_t value);
void multi_reset_controllers(multi* m, uint32_t part);
void multi_filter(multi* m, uint32_t locks, uint32_t cutoff, uint32_t resonance);
void multi_fresh(multi* m);
void multi_reset(multi* m);
void multi_tick(multi* m);

#ifdef __cplusplus
}
#endif

#endif
