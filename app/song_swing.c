// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_swing.h"

#include <string.h>

#define SWING_VOICES 3u
#define OFFBEAT_ROW (SONG_SWING_BEAT_ROWS / 2u)

void song_swing_played(const gt2_song* s, uint32_t subtune, uint32_t voices,
                       uint8_t set[SONG_SWING_SET_BYTES]) {
    memset(set, 0, SONG_SWING_SET_BYTES);
    for (uint32_t v = 0; v < SWING_VOICES; v++) {
        const uint8_t* o = voices >> v & 1u ? gt2_order_data(s, subtune, v) : NULL;
        uint32_t n = o ? gt2_order_entries(s, subtune, v) : 0u;
        for (uint32_t i = 0; i < n; i++) {
            if (o[i] < GT2_MAX_PATTERNS) {
                set[o[i] >> 3] |= (uint8_t)(1u << (o[i] & 7u));
            }
        }
    }
}

int song_swing_in(const uint8_t set[SONG_SWING_SET_BYTES], uint32_t pattern) {
    return pattern < GT2_MAX_PATTERNS && set[pattern >> 3] >> (pattern & 7u) & 1u;
}

int song_swing_eighths(const gt2_song* s, uint32_t subtune) {
    uint8_t played[SONG_SWING_SET_BYTES];
    uint32_t odd = 0u, offbeat = 0u;
    song_swing_played(s, subtune, SONG_SWING_ALL_VOICES, played);
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        const uint8_t* d = gt2_pattern_data(s, p);
        uint32_t rows = d && song_swing_in(played, p) ? gt2_pattern_rows(s, p) - 1u : 0u;
        for (uint32_t r = 0; r < rows; r++) {
            uint8_t note = d[r * GT2_ROW_BYTES];
            if (note >= GT2_FIRSTNOTE && note <= GT2_LASTNOTE) {
                odd += r & 1u;
                offbeat += r % SONG_SWING_BEAT_ROWS == OFFBEAT_ROW;
            }
        }
    }
    return 2u * odd < offbeat;
}

uint32_t song_swing_pair_at(uint32_t row, int eighths) {
    if (!eighths) {
        return row & 1u ? SONG_SWING_NONE : SONG_SWING_LONG_FIRST;
    }
    return !(row & 1u)                             ? SONG_SWING_NONE
           : row % SONG_SWING_BEAT_ROWS == 1u ? SONG_SWING_LONG_FIRST
                                                   : SONG_SWING_SHORT_FIRST;
}

int song_swing_owns(const gt2_song* s, const uint8_t cell[GT2_ROW_BYTES], uint32_t speed_sum) {
    uint32_t data = cell[3];
    if (!cell[2]) {
        return 1;
    }
    return cell[2] == SONG_SWING_FUNKTEMPO && data &&
           s->ltable[GT2_STBL][data - 1u] + s->rtable[GT2_STBL][data - 1u] == speed_sum;
}
