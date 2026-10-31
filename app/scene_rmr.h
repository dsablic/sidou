// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_RMR_H
#define SIDOU_SCENE_RMR_H

#include <stdint.h>

#include "scene_art.h"
#include "scene_px.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_RMR_REACH 8u

#define SCENE_RMR_KICK 1u
#define SCENE_RMR_SNARE 2u
#define SCENE_RMR_HAT 4u
#define SCENE_RMR_ARP 8u
enum { SCENE_RMR_BASS_NONE, SCENE_RMR_BASS_LOW, SCENE_RMR_BASS_HIGH };

#define SCENE_RMR_MID 120
#define SCENE_RMR_DRIFT 8
#define SCENE_RMR_APART 30
#define SCENE_RMR_HP 8u
#define SCENE_RMR_SKY_ROW0 2u
#define SCENE_RMR_SKY_ROWS 15u
#define SCENE_RMR_SKY_DIM 737u
#define SCENE_RMR_SKY_MID 1392u
#define SCENE_RMR_CALM_FRAMES 25u
#define SCENE_RMR_ARC_MARKS 14u
#define SCENE_RMR_SPARK_UP 92
#define SCENE_RMR_ARC_UP 60

enum {
    SCENE_RMR_SLOT_HUD,
    SCENE_RMR_SLOT_EMPTY0,
    SCENE_RMR_SLOT_FULL0,
    SCENE_RMR_SLOT_TIP0,
    SCENE_RMR_SLOT_EMPTY1,
    SCENE_RMR_SLOT_FULL1,
    SCENE_RMR_SLOT_TIP1,
    SCENE_RMR_SLOT_SKY,
    SCENE_RMR_SLOT_ARENA,
    SCENE_RMR_SLOT_FLAME0,
    SCENE_RMR_SLOT_FLAME1,
    SCENE_RMR_SLOT_STUCK_BEHIND,
    SCENE_RMR_SLOT_SHADOW0,
    SCENE_RMR_SLOT_FIGHTER0,
    SCENE_RMR_SLOT_SHADOW1,
    SCENE_RMR_SLOT_FIGHTER1,
    SCENE_RMR_SLOT_SPARK,
    SCENE_RMR_SLOT_ARC,
    SCENE_RMR_SLOT_STUCK_OVER,
    SCENE_RMR_SLOT_SWORD,
    SCENE_RMR_SLOT_COVER,
    SCENE_RMR_SLOTS
};

typedef struct {
    uint8_t row;
    uint8_t s;
    uint8_t bar;
    uint8_t rpb;
    uint8_t hits;
    uint8_t bass;
} scene_rmr_beat;

typedef struct {
    int16_t x;
    uint8_t lead;
    uint8_t pose[2];
    uint8_t t[2];
    uint8_t next;
    uint8_t stance;
    uint8_t hp[2];
    int8_t unarmed;
    uint8_t sword;
    uint8_t sword_n;
    int16_t sword_x;
    uint8_t spark;
    uint8_t spin;
    uint8_t spin_who;
    uint8_t shake;
    uint8_t flare;
    uint8_t sky[SCENE_RMR_SKY_ROWS];
    uint32_t frame;
    int8_t shake_dx;
    uint8_t spark_shown;
    uint8_t flame_big;
    uint8_t arc;
    uint8_t sky_row;
    uint8_t quiet;
    uint32_t anim;
    scene_px_mark arc_marks[SCENE_RMR_ARC_MARKS];
} scene_rmr;

void scene_rmr_init(scene_rmr* r);
void scene_rmr_step(scene_rmr* r, const scene_rmr_beat* e);
void scene_rmr_frame(scene_rmr* r, uint32_t cutoff, int sounding);
uint32_t scene_rmr_sky_of(uint32_t cutoff);
void scene_rmr_draw(scene_rmr* r, scene_px* sp);
int scene_rmr_sword_at(const scene_rmr* r, int32_t* gx, int32_t* gy, uint32_t* k);

#ifdef __cplusplus
}
#endif

#endif
