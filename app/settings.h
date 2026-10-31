// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SETTINGS_H
#define SIDOU_SETTINGS_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "store.h"
#include "ui_sound.h"
#include "user_sound.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SETTINGS_KEY 0x100u
#define SETTINGS_TYPE 1u
#define SETTINGS_VERSION 9u
#define SETTINGS_VERSION_CHARS 14u
#define SETTINGS_PARAMS_V1 42u
#define SETTINGS_PARAMS_V8 3u
#define SETTINGS_PARAMS_V9 7u
#define SETTINGS_V1_BYTES (6u + SETTINGS_PARAMS_V1 + 2u + SETTINGS_PARAMS_V1 + SETTINGS_VERSION_CHARS)
#define SETTINGS_V2_BYTES (SETTINGS_V1_BYTES + 1u)
#define SETTINGS_V3_BYTES (SETTINGS_V2_BYTES + 1u)
#define SETTINGS_V4_BYTES (SETTINGS_V3_BYTES + STORE_NAME_MAX)
enum {
    SETTINGS_V5_BYTES = SETTINGS_V4_BYTES + 1u + 2u * SID_EXPERT_COUNT,
    SETTINGS_V6_BYTES = SETTINGS_V5_BYTES + 1u,
    SETTINGS_V7_BYTES = SETTINGS_V6_BYTES + 1u,
    SETTINGS_V8_BYTES = SETTINGS_V7_BYTES + 1u + SETTINGS_PARAMS_V8,
    SETTINGS_V9_BYTES = SETTINGS_V7_BYTES + 1u + SETTINGS_PARAMS_V9,
    SETTINGS_BYTES = SETTINGS_V9_BYTES
};
#define SETTINGS_SID_AUTO 0u
#define SETTINGS_SID_KEYS 1u
#define SETTINGS_DEBOUNCE_MS 5000u
#define SETTINGS_FORCE_MS 2000u

typedef enum {
    SETTINGS_NOTHING = 0,
    SETTINGS_SAVED = 1,
    SETTINGS_SAVED_GAPS = 2,
    SETTINGS_FOREIGN = 3,
    SETTINGS_FAILED = 4
} settings_result;

typedef struct {
    live* l;
    midi_learn* ml;
    uint8_t* cards;
    uint8_t* tracker_rows;
    ui_sound* snd;
    store* st;
    const char* version;
    void (*force)(int on);
    uint8_t out[SETTINGS_BYTES];
    uint32_t seen;
    uint32_t saved;
    uint32_t changed_at;
    uint32_t asked_at;
    uint8_t asked;
    uint8_t forcing;
    uint8_t result;
    uint8_t started;
    uint8_t errors;
    uint8_t watch;
    uint8_t tries;
    char last_song[STORE_NAME_MAX + 1u];
    uint8_t sid_model;
    user_sounds* sounds;
    uint8_t edit[USER_SOUND_EDIT_BYTES];
    uint32_t edit_seen;
    uint32_t edit_saved;
} settings;

void settings_init(settings* s, live* l, midi_learn* ml, uint8_t* cards, uint8_t* tracker_rows, ui_sound* snd,
                   store* st, const char* version, void (*force)(int on));
void settings_sounds(settings* s, user_sounds* sounds);
uint32_t settings_gather(const settings* s, uint8_t* out);
int settings_apply(settings* s, const uint8_t* rec, uint32_t len);
int settings_load(settings* s, uint32_t now_ms);
void settings_tick(settings* s, uint32_t now_ms);
void settings_save_now(settings* s, uint32_t now_ms);
void settings_reset(settings* s);
void settings_forget(settings* s);
void settings_set_song(settings* s, const char* name);
settings_result settings_take_result(settings* s);

#ifdef __cplusplus
}
#endif

#endif
