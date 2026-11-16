// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>

#include "art_pack.h"
#include "grid.h"
#include "scene_px.h"
#include "scene_rmr.h"

#define EXPORT(name) __attribute__((export_name(#name)))
#define SIDE (GRID_COLS * GRID_CELL_PX)
#define TILE GRID_CELL_PX
#define ROWS_A_BEAT 4u
#define FRAME_ROW_MAX 255u

static uint8_t pack[STORE_FILE_MAX];
static scene_rmr_art art;
static scene_rmr rmr;
static scene_px px;
static grid g;
static uint8_t screen[SIDE * SIDE];
static uint8_t row[FRAME_ROW_MAX];

EXPORT(art_pack_buffer) uint8_t* art_pack_buffer(void) {
    return pack;
}

EXPORT(art_pack_check) int32_t art_pack_check(uint32_t length) {
    return art_pack_bytes(&art, pack, length);
}

EXPORT(art_frame_row) uint8_t* art_frame_row(uint32_t frame, uint32_t y) {
    const scene_image* f = &art.frame[frame];
    scene_image_row(f, 0, y, f->fw, 0, row);
    return row;
}

EXPORT(art_sky_row) uint8_t* art_sky_row(uint32_t sky, uint32_t y) {
    scene_gradient_row(&scene_rmr_sky_gradient[sky], 0, SCENE_RMR_SKY_Y + y, SIDE, screen);
    return screen;
}

EXPORT(art_play_start) void art_play_start(void) {
    scene_px_init(&px, &g);
    scene_rmr_init(&rmr);
}

EXPORT(art_play_beat) void art_play_beat(uint32_t s, uint32_t bar, uint32_t hits, uint32_t bass) {
    const scene_rmr_beat b = {1u, (uint8_t)s, (uint8_t)bar, ROWS_A_BEAT, (uint8_t)hits,
                              (uint8_t)bass};
    scene_rmr_step(&rmr, &b);
}

EXPORT(art_play_frame) uint8_t* art_play_frame(uint32_t cutoff) {
    scene_rmr_frame(&rmr, cutoff, 1);
    scene_rmr_draw(&rmr, &art, &px);
    uint8_t tile[TILE * TILE];
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        for (uint32_t c = 0; c < GRID_COLS; c++) {
            scene_px_tile(&px, c, r, tile);
            for (uint32_t y = 0; y < TILE; y++) {
                for (uint32_t x = 0; x < TILE; x++) {
                    screen[(r * TILE + y) * SIDE + c * TILE + x] = tile[y * TILE + x];
                }
            }
        }
    }
    return screen;
}
