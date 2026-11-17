// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_TEMPO_H
#define SIDOU_SONG_TEMPO_H

#include <stdint.h>

#define SONG_TEMPO_PAL_HZ 985248u
#define SONG_TEMPO_NTSC_HZ 1022727u
#define SONG_TEMPO_PAL_CIA 0x4cc7u
#define SONG_TEMPO_NTSC_CIA 0x42c6u
#define SONG_TEMPO_MULTISPEED_MAX 4u
#define SONG_SWING_STRAIGHT 0u
#define SONG_SWING_LIGHT 1u
#define SONG_SWING_SHUFFLE 2u
#define SONG_SWING_HARD 3u
#define SONG_SWING_LEVELS 4u
#define SONG_SWING_TWELFTHS 12u
#define SONG_SWING_EVEN_TWELFTHS 6u
#define SONG_SWING_SPEED_MIN 2u

static inline uint32_t song_tempo_bpm10(uint32_t speed_sum, uint32_t multispeed, uint32_t ntsc,
                                        uint32_t rows_beat) {
    if (speed_sum == 0u || rows_beat == 0u) {
        return 0;
    }
    uint32_t cia = ntsc ? SONG_TEMPO_NTSC_CIA : SONG_TEMPO_PAL_CIA;
    uint32_t calls = multispeed > SONG_TEMPO_MULTISPEED_MAX ? SONG_TEMPO_MULTISPEED_MAX : multispeed;
    uint32_t period = multispeed ? cia / calls + 1u : 2u * cia + 1u;
    uint32_t num = 1200u * (ntsc ? SONG_TEMPO_NTSC_HZ : SONG_TEMPO_PAL_HZ);
    uint32_t x = num / period * 256u + num % period * 256u / period;
    uint32_t d = speed_sum * rows_beat;
    return (x + d * 128u) / (d * 256u);
}

static inline uint32_t song_swing_long(uint32_t speed_sum, uint32_t level) {
    if (level == SONG_SWING_STRAIGHT) {
        return (speed_sum + 1u) / 2u;
    }
    uint32_t twelfths = speed_sum * (SONG_SWING_EVEN_TWELFTHS + level);
    return (twelfths + SONG_SWING_TWELFTHS / 2u - 1u) / SONG_SWING_TWELFTHS;
}

static inline int song_swing_gives(uint32_t speed_sum, uint32_t level) {
    if (level >= SONG_SWING_LEVELS) {
        return 0;
    }
    uint32_t l = song_swing_long(speed_sum, level);
    return level == SONG_SWING_STRAIGHT ||
           (l > song_swing_long(speed_sum, level - 1u) && speed_sum - l >= SONG_SWING_SPEED_MIN);
}

#endif
