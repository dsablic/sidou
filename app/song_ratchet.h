// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_RATCHET_H
#define SIDOU_SONG_RATCHET_H

#include <stdint.h>

#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_RATCHET_OFF 1u
#define SONG_RATCHET_MAX 4u
#define SONG_RATCHET_HIT_ROWS 3u
#define SONG_RATCHET_ROWS ((SONG_RATCHET_MAX - 1u) * SONG_RATCHET_HIT_ROWS + 1u)
#define SONG_RATCHET_MIN_FRAMES 2u

uint32_t song_ratchet_rows(const gt2_song* s, uint32_t base, uint32_t count, uint32_t row_frames,
                           uint8_t left[SONG_RATCHET_ROWS], uint8_t right[SONG_RATCHET_ROWS]);
uint32_t song_ratchet_count(const gt2_song* s, uint32_t instr);
uint32_t song_ratchet_base(const gt2_song* s, uint32_t instr);
uint32_t song_ratchet_copy(const gt2_song* s, uint32_t base, uint32_t count, uint32_t row_frames);
void song_ratchet_instr(const gt2_song* s, uint32_t base, uint32_t count, uint32_t wave,
                        uint8_t out[GT2_INSTR_BYTES]);

#ifdef __cplusplus
}
#endif

#endif
