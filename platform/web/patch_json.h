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

void patch_json_defaults(uint8_t sound[LAB_SOUND_BYTES]);
int patch_json_read(patch_json* p, const char* text, uint32_t len);
uint32_t patch_json_write(const uint8_t sound[LAB_SOUND_BYTES], const char* name, char* out,
                          uint32_t max);
uint32_t patch_json_fields(char* out, uint32_t max);
int patch_json_song(gt2_song* s, const uint8_t sound[LAB_SOUND_BYTES], const char* name,
                    uint32_t model);

#ifdef __cplusplus
}
#endif

#endif
