// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_VIEW_H
#define SIDOU_SCENE_VIEW_H

#include <stdint.h>

#include "live.h"
#include "scene.h"
#include "scene_px.h"
#include "scene_rmr.h"
#include "scene_song.h"
#include "scope.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_VIEW_FRAME_CELLS 200u

typedef struct {
    scene_px px;
    scene_rmr rmr;
    grid* g;
    uint16_t left;
} scene_view_rite;

typedef struct {
    const live* l;
    scope* tap;
    uint8_t on;
    uint8_t which;
    uint8_t prepared;
    uint8_t starting;
    uint8_t say;
    uint32_t stamp[SCENE_VOICES];
    uint16_t cursor;
    uint32_t held;
    uint8_t idle;
    scene_music music;
    scene_song song;
    union {
        scene piece;
        scene_view_rite rite;
    } u;
} scene_view;

extern const view scene_view_view;

void scene_view_init(scene_view* sv, const live* l, scope* sc);
void scene_view_tap(scene_view* sv, int on);
void scene_view_prepare(scene_view* sv);
int scene_view_idle(scene_view* sv);
void scene_view_song(scene_view* sv, const song_ctl* sc, const song_event* ev, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
