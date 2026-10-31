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
#define SCENE_PIECES 6u
#define SCENE_PETSCII 5u
#define SCENE_FIRE 0u
#define SCENE_PLASMA 1u
#define SCENE_RULE30 2u
#define SCENE_STARS 3u
#define SCENE_ANT 4u
#define SCENE_RMR 5u

#define SCENE_NO_NOTE 0xFFu
#define SCENE_DARK_CUTOFF 512u

#define SCENE_FIRE_ROWS 14u
#define SCENE_FIRE_H (2u * SCENE_FIRE_ROWS)
#define SCENE_RULE30_W (2u * GRID_COLS)
#define SCENE_RULE30_H (2u * GRID_ROWS)
#define SCENE_RULE30_MAX_STEPS 3u
#define SCENE_RULE30_START 24u
#define SCENE_STARS_N 90u
#define SCENE_STARS_LIT 5u
#define SCENE_RIPPLE_FRAMES 30u
#define SCENE_ANT_STEPS 3u
#define SCENE_ANT_RESET 9000u
#define SCENE_ENV_STEP 512u

typedef struct {
    uint8_t gate[SCENE_VOICES];
    uint8_t trig[SCENE_VOICES];
    uint8_t note[SCENE_VOICES];
    uint8_t env[SCENE_VOICES];
    uint8_t color[SCENE_VOICES];
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
    uint64_t row[SCENE_RULE30_H];
    uint8_t color[SCENE_RULE30_H];
    uint64_t cur;
    uint8_t head;
    uint16_t acc;
} scene_rule30;

typedef struct {
    int16_t x, y;
    uint16_t z;
} scene_star;

typedef struct {
    scene_star st[SCENE_STARS_N];
    int16_t cx;
    int16_t aim;
    uint8_t lit[SCENE_VOICES];
} scene_stars;

typedef struct {
    uint8_t x, y, d;
} scene_ant_walker;

typedef struct {
    uint8_t cell[GRID_ROWS][GRID_COLS];
    scene_ant_walker a[SCENE_VOICES];
    uint16_t acc[SCENE_VOICES];
    uint16_t steps;
} scene_ant;

typedef struct {
    uint8_t piece;
    uint32_t frame;
    uint32_t rng;
    uint8_t filled;
    union {
        scene_fire fire;
        scene_plasma plasma;
        scene_rule30 rule30;
        scene_stars stars;
        scene_ant ant;
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
uint64_t scene_rule30_next(uint64_t cur);
uint8_t scene_dark(uint8_t color);

#ifdef __cplusplus
}
#endif

#endif
