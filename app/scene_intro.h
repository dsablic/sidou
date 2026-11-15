// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_INTRO_H
#define SIDOU_SCENE_INTRO_H

#include <stdint.h>

#include "grid.h"
#include "scene_px.h"
#include "scene_rmr.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_INTRO_LOGO_Y0 64
#define SCENE_INTRO_LOGO_Y1 160
#define SCENE_INTRO_LOGO_ROWS ((SCENE_INTRO_LOGO_Y1 - SCENE_INTRO_LOGO_Y0) / 8)
#define SCENE_INTRO_CUTOFF_TOP 2048u
#define SCENE_INTRO_CALM_FRAMES 25u
#define SCENE_INTRO_STARS 24u
#define SCENE_INTRO_STARS_TOP 11u
#define SCENE_INTRO_STAR_TOP_Y 10
#define SCENE_INTRO_STAR_LOW_Y 166
#define SCENE_INTRO_STAR_PITCH 5
#define SCENE_INTRO_STAR_DEPTHS 3u
#define SCENE_INTRO_STAR_SUBPX 16u
#define SCENE_INTRO_STAR_SEED 0x51D0u
#define SCENE_INTRO_GEAR_TOP 4u
#define SCENE_INTRO_STARS_CALM 2u
#define SCENE_INTRO_GLOW_EVERY 2u

enum { SCENE_INTRO_SLOT_STARS_TOP, SCENE_INTRO_SLOT_STARS_LOW, SCENE_INTRO_SLOT_LOGO };

enum {
    SCENE_INTRO_GLOW_CALM,
    SCENE_INTRO_GLOW_KICK,
    SCENE_INTRO_GLOW_SNARE,
    SCENE_INTRO_GLOW_HAT,
    SCENE_INTRO_GLOW_ARP,
    SCENE_INTRO_GLOW_BASS,
    SCENE_INTRO_GLOWS
};

typedef struct {
    uint32_t frame;
    uint32_t cutoff;
    uint8_t quiet;
    uint8_t glow;
    uint8_t glow_next;
    uint8_t glow_row[SCENE_INTRO_LOGO_ROWS];
    uint16_t star[SCENE_INTRO_STARS];
    grid_bitmap_cursor logo_at[8];
} scene_intro;

void scene_intro_init(scene_intro* in);
void scene_intro_beat(scene_intro* in, const scene_rmr_beat* e);
void scene_intro_frame(scene_intro* in, uint32_t cutoff, int sounding);
void scene_intro_draw(scene_intro* in, scene_px* sp, grid* g);

#ifdef __cplusplus
}
#endif

#endif
