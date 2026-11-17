// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_SWING_H
#define SIDOU_SONG_SWING_H

#include <stdint.h>

#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_SWING_FUNKTEMPO 0x0Eu
#define SONG_SWING_BEAT_ROWS 4u
#define SONG_SWING_SET_BYTES (GT2_MAX_PATTERNS / 8u)
#define SONG_SWING_VOICE_1 1u
#define SONG_SWING_ALL_VOICES 7u
enum { SONG_SWING_NONE, SONG_SWING_LONG_FIRST, SONG_SWING_SHORT_FIRST, SONG_SWING_PAIRS };

void song_swing_played(const gt2_song* s, uint32_t subtune, uint32_t voices,
                       uint8_t set[SONG_SWING_SET_BYTES]);
int song_swing_in(const uint8_t set[SONG_SWING_SET_BYTES], uint32_t pattern);
int song_swing_eighths(const gt2_song* s, uint32_t subtune);
uint32_t song_swing_pair_at(uint32_t row, int eighths);
int song_swing_owns(const gt2_song* s, const uint8_t cell[GT2_ROW_BYTES], uint32_t speed_sum);

#ifdef __cplusplus
}
#endif

#endif
