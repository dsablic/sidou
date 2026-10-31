// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCENE_PX_H
#define SIDOU_SCENE_PX_H

#include <stdint.h>

#include "grid.h"
#include "scene_art.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_PX_LAYERS 24u
#define SCENE_PX_W 240
#define SCENE_PX_H 240
#define SCENE_PX_RUN_CELLS 4u
#define SCENE_PX_SCAN_SLICE 8u

enum {
    SCENE_PX_NONE,
    SCENE_PX_FILL,
    SCENE_PX_GRADIENT,
    SCENE_PX_IMAGE,
    SCENE_PX_MARKS,
    SCENE_PX_TEXT,
    SCENE_PX_CALL
};

typedef struct {
    int16_t x, y;
    uint8_t colour;
} scene_px_mark;

typedef void (*scene_px_fn)(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out);

typedef struct {
    uint8_t kind;
    uint8_t colour;
    uint8_t mirror;
    uint8_t n;
    int16_t x, y;
    int16_t x0, y0, x1, y1;
    const void* src;
    const uint8_t* remap;
    scene_px_fn fn;
    uint32_t param;
} scene_px_layer;

typedef struct {
    scene_px_layer layer[SCENE_PX_LAYERS];
    int16_t box[SCENE_PX_LAYERS][4];
    uint8_t live[SCENE_PX_LAYERS];
    uint8_t nlive;
    uint32_t hash[GRID_CELLS];
    uint8_t cand[(GRID_CELLS + 7u) / 8u];
    uint16_t ncand;
    uint16_t next;
    uint16_t sweep;
    uint32_t scanned;
    uint32_t painted;
    uint32_t marked;
    uint8_t fresh;
} scene_px;

void scene_px_init(scene_px* sp, grid* g);
void scene_px_set(scene_px* sp, uint32_t i, const scene_px_layer* l);
void scene_px_touch(scene_px* sp, int32_t x0, int32_t y0, int32_t x1, int32_t y1);
uint32_t scene_px_scan(scene_px* sp, grid* g, uint32_t most);
void scene_px_refresh(scene_px* sp, grid* g, uint32_t n);
void scene_px_tile(const scene_px* sp, uint32_t col, uint32_t row, uint8_t out[64]);
uint32_t scene_px_hash(const uint8_t px[64]);

#ifdef __cplusplus
}
#endif

#endif
