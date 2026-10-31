// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_ART_H
#define SIDOU_SCENE_ART_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_ART_CLEAR 0xffu
#define SCENE_ART_SEEK_LOG2 5u
#define SCENE_ART_SEEK (1u << SCENE_ART_SEEK_LOG2)

typedef struct scene_image scene_image;
struct scene_image {
    uint8_t fw, fh;
    uint8_t x, y;
    uint8_t w, h;
    uint8_t gx;
    uint8_t clear;
    uint8_t ax, ay;
    const uint16_t* rows;
    const uint8_t* data;
    const scene_image* base;
};

void scene_image_row(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, int mirror,
                     uint8_t* out);
void scene_image_blend(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, uint32_t h,
                       uint32_t stride, int mirror, const uint8_t* remap, uint8_t* out);

typedef struct {
    uint8_t y;
    uint8_t colour;
} scene_stop;

typedef struct {
    uint8_t y0, y1;
    uint8_t seam;
    uint8_t n;
    const scene_stop* stops;
} scene_gradient;

void scene_gradient_row(const scene_gradient* g, uint32_t x, uint32_t y, uint32_t n,
                        uint8_t* out);

enum {
    SCENE_RMR_GUARD,
    SCENE_RMR_GUARD2,
    SCENE_RMR_RAISE,
    SCENE_RMR_CHOP,
    SCENE_RMR_BLOCK,
    SCENE_RMR_SPIN_A,
    SCENE_RMR_SPIN_B,
    SCENE_RMR_HIT,
    SCENE_RMR_POSES
};
enum {
    SCENE_RMR_UNARMED_GUARD,
    SCENE_RMR_UNARMED_GUARD2,
    SCENE_RMR_UNARMED_HIT,
    SCENE_RMR_UNARMED_REACH,
    SCENE_RMR_UNARMED_POSES
};
#define SCENE_RMR_SWORD_FRAMES 8u
#define SCENE_RMR_SKIES 3u
#define SCENE_RMR_FEET 228u

extern const scene_image scene_rmr_barb[2][SCENE_RMR_POSES];
extern const scene_image scene_rmr_barb_unarmed[2][SCENE_RMR_UNARMED_POSES];
extern const scene_image scene_rmr_sword[SCENE_RMR_SWORD_FRAMES];
extern const scene_image scene_rmr_sword_stuck;
extern const scene_gradient scene_rmr_sky_gradient[SCENE_RMR_SKIES];
extern const scene_image scene_rmr_sky[SCENE_RMR_SKIES];
#define SCENE_RMR_SKY_Y 16u
extern const scene_image scene_rmr_arena;
extern const uint8_t scene_rmr_flames[2][4];
extern const scene_image scene_rmr_hud;
extern const scene_image scene_rmr_blade_empty;
extern const scene_image scene_rmr_blade_full;
extern const scene_image scene_rmr_blade_tip;
#define SCENE_RMR_BLADE_X 17
#define SCENE_RMR_BLADE_N 11
#define SCENE_RMR_BLADE_Y 6

typedef struct {
    int8_t dx;
    uint8_t y;
    uint8_t k;
} scene_rmr_grip;
typedef struct {
    uint8_t frames;
    const scene_rmr_grip* path;
    scene_rmr_grip stuck;
} scene_rmr_flight;
extern const scene_rmr_flight scene_rmr_flights[2];
typedef struct {
    int8_t dy0;
    uint8_t rows;
    const uint8_t* at;
    const int8_t* dx;
    const uint8_t* colour;
} scene_rmr_spark;
extern const scene_rmr_spark scene_rmr_sparks[3];

#ifdef __cplusplus
}
#endif

#endif
