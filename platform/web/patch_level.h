// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PATCH_LEVEL_H
#define SIDOU_PATCH_LEVEL_H

#include <stdint.h>

#include "player/song_ctl.h"
#include "sid/sid.h"
#include "sid/sid_character.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PATCH_LEVEL_RATE 44118u
#define PATCH_LEVEL_ERROR_MAX 512u

enum { PATCH_LEVEL_PATCH, PATCH_LEVEL_BANK, PATCH_LEVEL_STEP };

typedef struct {
    song_ctl sc;
    gt2_shape shapes[2];
    song_work work;
    sid chip;
    sid_character character;
    sid_dac dac;
    int16_t pcm[SONG_BLOCK];
} patch_level_rig;

uint32_t patch_level_rms(patch_level_rig* r, gt2_song* s, uint32_t instr, uint32_t model);
int patch_level_check(patch_level_rig* r, gt2_song* s, uint32_t first, uint32_t count,
                      uint32_t style, char* error, uint32_t max);

#ifdef __cplusplus
}
#endif

#endif
