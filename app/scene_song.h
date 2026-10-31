// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_SONG_H
#define SIDOU_SCENE_SONG_H

#include <stdint.h>

#include "live.h"
#include "player/song_ctl.h"
#include "scene_rmr.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_SONG_VOICES 3u
#define SCENE_SONG_BEATS 16u
#define SCENE_SONG_KICK_BELOW 100u
#define SCENE_SONG_HAT_FROM 106u
#define SCENE_SONG_BASS_SPLIT 50u
#define SCENE_SONG_RPB 4u
#define SCENE_SONG_CLOCK 0u
#define SCENE_SONG_FREE_FRAMES 3u
#define SCENE_SONG_NO_NOTE 0xffu

typedef struct {
    const song_ctl* sc;
    uint8_t playing;
    uint8_t rpb;
    uint8_t role[SCENE_SONG_VOICES];
    uint8_t drums;
    uint8_t drum[SONG_SCENE_DRUMS_MAX][2];
    uint8_t s, bar;
    uint8_t rows_seen;
    uint8_t gate[SCENE_SONG_VOICES];
    uint8_t noise[2][SCENE_SONG_VOICES];
    uint8_t low[2][SCENE_SONG_VOICES];
    uint8_t blo[2], bhi[2];
    uint8_t open;
    uint16_t call;
    uint8_t row_ev, row;
    uint8_t note[SCENE_SONG_VOICES], instr[SCENE_SONG_VOICES];
    scene_rmr_beat beat[SCENE_SONG_BEATS];
    uint8_t n;
    uint32_t lost;
    uint32_t stamp[SCENE_SONG_VOICES];
    uint8_t free;
} scene_song;

typedef struct {
    scene_rmr_beat beat[SCENE_SONG_BEATS];
    uint32_t n;
    uint32_t cutoff;
    uint8_t sounding;
} scene_song_frame_out;

void scene_song_init(scene_song* ss);
void scene_song_events(scene_song* ss, const song_ctl* sc, const song_event* ev, uint32_t n);
void scene_song_frame(scene_song* ss, const live* l, scene_song_frame_out* out);
uint32_t scene_song_drum_of_pitch(uint32_t pitch);

#ifdef __cplusplus
}
#endif

#endif
