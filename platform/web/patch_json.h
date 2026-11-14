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

#define PATCH_STEP_FORMAT "sidou-step/1"
#define PATCH_STEP_ROWS 16u
#define PATCH_STEP_PAGES 4u
#define PATCH_STEP_MAX (PATCH_STEP_ROWS * PATCH_STEP_PAGES)
#define PATCH_STEP_NAME_DEFAULT "STEP"

enum { PATCH_STEP_PULSE, PATCH_STEP_BASS, PATCH_STEP_KIT, PATCH_STEP_PATCH };
enum { PATCH_JSON_PATCH_TEXT, PATCH_JSON_BANK_TEXT, PATCH_JSON_STEP_TEXT };

typedef struct {
    uint8_t row, note, drum, slide, accent, chance;
} patch_step_cell;

typedef struct {
    char name[PATCH_BANK_NAME + 1u];
    uint8_t voice, role, gate, pages, kind, count, instr;
    patch_step_cell steps[PATCH_STEP_MAX];
    char error[PATCH_JSON_ERROR_MAX];
    patch_json patch;
    patch_json scratch;
} patch_step;

#define PATCH_SONG_FORMAT "sidou-song/1"
#define PATCH_SONG_NAME_DEFAULT "SONG"
#define PATCH_SONG_SECTIONS 16u
#define PATCH_SONG_ROWS 128u
#define PATCH_SONG_VOICES GT2_MONO_CHANNELS
#define PATCH_SONG_ORDER 64u
#define PATCH_SONG_INSTRUMENTS 16u
#define PATCH_SONG_LABEL 8u
#define PATCH_SONG_BPM_MIN 60u
#define PATCH_SONG_BPM_MAX 300u
enum { PATCH_JSON_SONG_TEXT = PATCH_JSON_STEP_TEXT + 1 };
enum { PATCH_SONG_BUILTIN = PATCH_SONG_INSTRUMENTS, PATCH_SONG_REST_VOICE = 0xFF };

typedef struct {
    uint8_t note, drum, accent, slide, hold, rest;
} patch_song_cell;

typedef struct {
    char label[PATCH_SONG_LABEL + 1u];
    uint8_t rows;
    uint8_t instr[PATCH_SONG_VOICES];
    patch_song_cell cells[PATCH_SONG_VOICES][PATCH_SONG_ROWS];
} patch_song_section;

typedef struct {
    char name[PATCH_BANK_NAME + 1u];
    uint16_t bpm, bpm_played10;
    uint8_t sections, order_len, instruments;
    uint8_t order[PATCH_SONG_ORDER];
    char labels[PATCH_SONG_INSTRUMENTS][PATCH_SONG_LABEL + 1u];
    patch_json patches[PATCH_SONG_INSTRUMENTS];
    patch_song_section section[PATCH_SONG_SECTIONS];
    char error[PATCH_JSON_ERROR_MAX];
} patch_song;

void patch_json_defaults(uint8_t sound[LAB_SOUND_BYTES]);
int patch_json_read(patch_json* p, const char* text, uint32_t len);
uint32_t patch_json_write(const uint8_t sound[LAB_SOUND_BYTES], const char* name, char* out,
                          uint32_t max);
uint32_t patch_json_fields(char* out, uint32_t max);
int patch_json_is_bank(const char* text, uint32_t len);
int patch_json_kind(const char* text, uint32_t len);
int patch_json_read_step(patch_step* st, const char* text, uint32_t len);
int patch_json_step_song(gt2_song* s, patch_step* st, uint32_t model);
uint32_t patch_json_write_step(const gt2_song* s, uint32_t voice, char* out, uint32_t max);
int patch_json_read_song(patch_song* sg, const char* text, uint32_t len);
int patch_json_song_song(gt2_song* s, patch_song* sg, uint32_t model);
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
