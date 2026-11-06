// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PATCH_JSON_H
#define SIDOU_PATCH_JSON_H

#include <stdint.h>

#include "lab_sound.h"
#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PATCH_JSON_FORMAT "sidou-patch/1"
#define PATCH_JSON_NAME "PATCH"
#define PATCH_JSON_ERROR_MAX 320u
#define PATCH_JSON_DEPTH_MAX 8u

enum { PATCH_JSON_OK = 0, PATCH_JSON_QUIET = 1 };

typedef struct {
    uint8_t sound[LAB_SOUND_BYTES];
    char name[LAB_SOUND_NAME + 1u];
    char error[PATCH_JSON_ERROR_MAX];
} patch_json;

#define PATCH_BANK_FORMAT "sidou-bank/1"
#define PATCH_BANK_MAX 16u
#define PATCH_BANK_NAME 16u
#define PATCH_BANK_NAME_DEFAULT "BANK"

typedef struct {
    uint8_t sounds[PATCH_BANK_MAX][LAB_SOUND_BYTES];
    char names[PATCH_BANK_MAX][LAB_SOUND_NAME + 1u];
    char name[PATCH_BANK_NAME + 1u];
    uint32_t count;
    char error[PATCH_JSON_ERROR_MAX];
    patch_json part;
} patch_bank;

void patch_json_defaults(uint8_t sound[LAB_SOUND_BYTES]);
int patch_json_read(patch_json* p, const char* text, uint32_t len);
uint32_t patch_json_write(const uint8_t sound[LAB_SOUND_BYTES], const char* name, char* out,
                          uint32_t max);
uint32_t patch_json_fields(char* out, uint32_t max);
int patch_json_is_bank(const char* text, uint32_t len);
int patch_json_read_bank(patch_bank* b, const char* text, uint32_t len);
int patch_json_bank_song(gt2_song* s, patch_bank* b, uint32_t model);
uint32_t patch_json_write_bank(const patch_bank* b, char* out, uint32_t max);
uint32_t patch_json_song_sound(const gt2_song* s, uint32_t instr, char* out, uint32_t max);
int patch_json_song(gt2_song* s, const uint8_t sound[LAB_SOUND_BYTES], const char* name,
                    uint32_t model);

#ifdef __cplusplus
}
#endif

#endif
