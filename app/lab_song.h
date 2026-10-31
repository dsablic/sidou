// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_SONG_H
#define SIDOU_LAB_SONG_H

#include <stdint.h>

#include "lab_sound.h"
#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_SONG_ROWS 10u

enum {
    LAB_SONG_ATTACK = 1u << 0,
    LAB_SONG_PWM = 1u << 1,
    LAB_SONG_FILTER = 1u << 2,
    LAB_SONG_LFO = 1u << 3,
    LAB_SONG_VIB = 1u << 4,
    LAB_SONG_ARP = 1u << 5,
    LAB_SONG_WAVE = 1u << 6,
    LAB_SONG_DRUM = 1u << 7,
    LAB_SONG_SWEEP = 1u << 8,
    LAB_SONG_GATE = 1u << 9,
    LAB_SONG_APPROX = (1u << 10) - 1u,
    LAB_SONG_SYNC = 1u << 10,
    LAB_SONG_TUNE = 1u << 11,
    LAB_SONG_PLAY = 1u << 12,
    LAB_SONG_FX = 1u << 13,
    LAB_SONG_DROPPED = 15u << 10,
    LAB_SONG_EXACT = 1u << 14
};

typedef struct {
    uint8_t instr[GT2_INSTR_BYTES];
    uint8_t rows[GT2_TABLES];
    uint8_t left[GT2_TABLES][LAB_SONG_ROWS];
    uint8_t right[GT2_TABLES][LAB_SONG_ROWS];
    uint16_t notes;
} lab_song_instr;

void lab_song_encode(const uint8_t sound[LAB_SOUND_BYTES], const char* name, uint32_t voice,
                     uint32_t model, const uint8_t first[GT2_TABLES], lab_song_instr* out);

uint32_t lab_song_tag(uint32_t instr);
uint32_t lab_song_crc(const gt2_song* s, uint32_t instr, const uint8_t rows[GT2_TABLES]);
int lab_song_kept(const gt2_song* s, uint32_t instr, uint8_t rows[GT2_TABLES],
                  uint8_t sound[LAB_SOUND_BYTES]);
#define LAB_SONG_KEPT_MAX 16u
int lab_song_keep(gt2_song* s, uint32_t instr, const lab_song_instr* in,
                  const uint8_t sound[LAB_SOUND_BYTES]);
uint32_t lab_song_crc_of(const lab_song_instr* in);

uint32_t lab_song_pull(const gt2_song* s, uint32_t instr, uint32_t model,
                       uint8_t sound[LAB_SOUND_BYTES]);

void lab_song_words(uint32_t notes, int dropped, char* out, uint32_t max);

uint32_t lab_song_free(const gt2_song* s);
uint32_t lab_song_table_end(const gt2_song* s, uint32_t t);
int lab_song_shared(const gt2_song* s, uint32_t instr, uint32_t t, uint32_t first, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
