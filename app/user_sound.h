// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_USER_SOUND_H
#define SIDOU_USER_SOUND_H

#include <stdint.h>

#include "lab_sound.h"
#include "live.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

#define USER_SOUNDS 16u
#define USER_SOUND_VERSION 1u
#define USER_SOUND_HEADER 3u
#define USER_SOUND_STRIDE (1u + LAB_SOUND_NAME + LAB_SOUND_BYTES)
#define USER_SOUND_FILE_BYTES (USER_SOUND_HEADER + USER_SOUNDS * USER_SOUND_STRIDE)
#define USER_SOUND_FORCE_MS 2000u

#define USER_SOUND_EDIT_KEY 0x109u
#define USER_SOUND_EDIT_TYPE 4u
#define USER_SOUND_EDIT_VERSION 1u
#define USER_SOUND_EDIT_BYTES (4u + LAB_SOUND_NAME + LAB_SOUND_BYTES)

typedef enum {
    USER_SOUND_NOTHING = 0,
    USER_SOUND_SAVED = 1,
    USER_SOUND_SAVED_GAPS = 2,
    USER_SOUND_FOREIGN = 3,
    USER_SOUND_FULL = 4,
    USER_SOUND_FAILED = 5
} user_sound_result;

typedef struct {
    live* l;
    store* st;
    uint8_t state;
    uint8_t slot;
    uint16_t at;
    uint8_t result;
    uint8_t result_slot;
    uint8_t forcing;
    uint8_t errors;
    uint32_t since;
    uint8_t rec[USER_SOUND_STRIDE];
} user_sounds;

void user_sounds_init(user_sounds* u, live* l, store* st);
void user_sounds_load(user_sounds* u);
void user_sounds_forget(user_sounds* u);
int user_sound_read(const user_sounds* u, uint32_t n, char* name, uint8_t sound[LAB_SOUND_BYTES]);
int user_sound_load(user_sounds* u, uint32_t n);
int user_sound_save(user_sounds* u, uint32_t n, const char* name,
                    const uint8_t sound[LAB_SOUND_BYTES], uint32_t now_ms);
int user_sounds_busy(const user_sounds* u);
void user_sounds_tick(user_sounds* u, uint32_t now_ms, void (*force)(int on));
user_sound_result user_sounds_take(user_sounds* u, uint32_t* slot);

uint32_t user_sound_edit_gather(const live* l, uint8_t* out);
uint8_t user_sound_preset_byte(const live* l);
int user_sound_edit_apply(live* l, const uint8_t* rec, uint32_t len);

#ifdef __cplusplus
}
#endif

#endif
