// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_SCOPE_H
#define SIDOU_SCENE_SCOPE_H

#include <stdint.h>

#include "live.h"
#include "scene_px.h"
#include "scope.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_SCOPE_LANES SCOPE_VOICES
#define SCENE_SCOPE_LANE_H (SCENE_PX_H / SCENE_SCOPE_LANES)
#define SCENE_SCOPE_MID (SCENE_SCOPE_LANE_H / 2)
#define SCENE_SCOPE_AMP 34
#define SCENE_SCOPE_POINTS SCOPE_READ_MAX
#define SCENE_SCOPE_DOTS 4u
#define SCENE_SCOPE_CALM_FRAMES 25u
#define SCENE_SCOPE_STILL 1
#define SCENE_SCOPE_EDGE_JITTER 2u
#define SCENE_SCOPE_LANES_A_FRAME 1u

enum { SCENE_SCOPE_CHIP_A, SCENE_SCOPE_CHIP_B, SCENE_SCOPE_CHIPS };
enum { SCENE_SCOPE_CORE, SCENE_SCOPE_HALO };

typedef struct {
    uint8_t y[SCENE_SCOPE_LANES][SCENE_PX_W];
    uint8_t chip;
    uint8_t quiet;
    uint8_t lane;
} scene_scope;

extern const uint8_t scene_scope_colour[SCENE_SCOPE_CHIPS][2];

void scene_scope_init(scene_scope* so);
uint32_t scene_scope_trigger(const uint8_t wave[SCOPE_W]);
void scene_scope_frame(scene_scope* so, const scope* tap, const live* l, scene_px* sp);

#ifdef __cplusplus
}
#endif

#endif
