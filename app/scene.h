// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_H
#define SIDOU_SCENE_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_VOICES 3u
#define SCENE_PIECES 5u
#define SCENE_PETSCII 2u
#define SCENE_FIRE 0u
#define SCENE_PLASMA 1u
#define SCENE_SCOPE 2u
#define SCENE_DEMO 3u
#define SCENE_RMR 4u

#define SCENE_NO_NOTE 0xFFu

#define SCENE_FIRE_ROWS 14u
#define SCENE_FIRE_H (2u * SCENE_FIRE_ROWS)
#define SCENE_RIPPLE_FRAMES 30u

typedef struct {
    uint8_t gate[SCENE_VOICES];
    uint8_t trig[SCENE_VOICES];
    uint8_t note[SCENE_VOICES];
    uint8_t env[SCENE_VOICES];
    uint16_t cutoff;
    uint8_t resonance;
} scene_music;

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} scene_canvas;

typedef struct {
    uint8_t heat[SCENE_FIRE_H][GRID_COLS];
    uint8_t hot[SCENE_VOICES];
    uint8_t flare[SCENE_VOICES];
    uint16_t burns;
} scene_fire;

typedef struct {
    uint8_t x, y;
    uint8_t age;
    uint8_t amp;
} scene_ripple;

typedef struct {
    uint32_t phase[4];
    uint8_t warm;
    scene_ripple rip[SCENE_VOICES];
} scene_plasma;

typedef struct {
    uint8_t piece;
    uint32_t frame;
    uint32_t rng;
    uint8_t filled;
    union {
        scene_fire fire;
        scene_plasma plasma;
    } u;
    scene_canvas cv;
} scene;

extern const uint8_t scene_dist4[GRID_ROWS][GRID_COLS];
extern const int8_t scene_sin8[256];
extern const char* const scene_names[SCENE_PIECES];

void scene_start(scene* s, uint32_t piece, const scene_music* m);
void scene_begin(scene* s, uint32_t piece);
void scene_fill(scene* s, const scene_music* m, uint32_t rows);
void scene_step(scene* s, const scene_music* m);
uint32_t scene_random(uint32_t* state);

#ifdef __cplusplus
}
#endif

#endif
