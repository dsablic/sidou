// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_SOUND_H
#define SIDOU_LAB_SOUND_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_SOUND_BYTES 103u
#define LAB_SOUND_BYTES_1_0 45u
#define LAB_SOUND_BYTES_1_5 48u
#define LAB_SOUND_BYTES_1_6 68u
#define LAB_SOUND_NAME 12u

#define LAB_SOUND_CONTROL 0u
#define LAB_SOUND_AD 1u
#define LAB_SOUND_SR 2u
#define LAB_SOUND_PULSE 3u
#define LAB_SOUND_CUTOFF 5u
#define LAB_SOUND_RESONANCE 7u
#define LAB_SOUND_FILTER_VOICES 8u
#define LAB_SOUND_FILTER_MODE 9u
#define LAB_SOUND_MONO_SYNC 10u
#define LAB_SOUND_SYNC_BASE 11u
#define LAB_SOUND_SYNC_SEMIS 13u
#define LAB_SOUND_PWM_DEPTH 14u
#define LAB_SOUND_PWM_RATE 16u
#define LAB_SOUND_TUNE 18u
#define LAB_SOUND_BEND_RANGE 19u
#define LAB_SOUND_VELOCITY 20u
#define LAB_SOUND_MOD 21u
#define LAB_SOUND_PRESSURE 22u
#define LAB_SOUND_VIBRATO_RATE 23u
#define LAB_SOUND_VIBRATO_DEPTH 24u
#define LAB_SOUND_VIBRATO_DELAY 25u
#define LAB_SOUND_ARP 26u
#define LAB_SOUND_SLOTS 30u
#define LAB_SOUND_FILTER_LFO 45u
#define LAB_SOUND_RESO_LINK 46u
#define LAB_SOUND_PWM_LINK 47u
#define LAB_SOUND_LAYERS 48u
#define LAB_SOUND_LAYER_TUNING 49u
#define LAB_SOUND_LAYER_TUNING_BYTES 2u
#define LAB_SOUND_LAYER_VOICE 55u
#define LAB_SOUND_LAYER_VOICE_BYTES 5u
#define LAB_SOUND_FILTER_ENV 65u
#define LAB_SOUND_FILTER_ENV_AD 66u
#define LAB_SOUND_FILTER_ENV_SR 67u
#define LAB_SOUND_TABLE 68u
#define LAB_SOUND_TABLE_LOOP 69u
#define LAB_SOUND_TABLE_RATE 70u
#define LAB_SOUND_TABLE_STEPS 71u
#define LAB_SOUND_TABLE_BYTES 35u

#define LAB_TABLE_ON 0x80u
#define LAB_TABLE_LAYER_SHIFT 5u
#define LAB_TABLE_LAYER_MASK 0x60u
#define LAB_TABLE_COUNT_MASK 0x1fu
#define LAB_TABLE_STEPS_MAX 16u
#define LAB_TABLE_RATE_MAX 16u
#define LAB_TABLE_NOTE_NONE 0x80u
#define LAB_TABLE_NOTE_UP_MAX 0x5fu
#define LAB_TABLE_NOTE_DOWN_FIRST 0x60u
#define LAB_TABLE_NOTE_LAST 0xdfu

#define LAB_SOUND_SYNC_BASE_MAX 4095u

uint32_t lab_sound_equal(const uint8_t a[LAB_SOUND_BYTES], const uint8_t b[LAB_SOUND_BYTES]);
uint32_t lab_sound_valid(const uint8_t s[LAB_SOUND_BYTES]);
int32_t lab_table_offset(uint8_t note);
uint32_t lab_table_absolute(uint8_t note);

#ifdef __cplusplus
}
#endif

#endif
