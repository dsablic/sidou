// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_DRAW_H
#define SIDOU_LAB_DRAW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "scope.h"
#include "screen.h"
#include "theme.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_DRAW_W 240u
#define LAB_DRAW_ROWS 14u
#define LAB_DRAW_H (LAB_DRAW_ROWS * GRID_CELL_PX)
#define LAB_DRAW_LAYERS 3u
#define LAB_DRAW_STEP 2u
#define LAB_DRAW_MARKER 2u
#define LAB_DRAW_NONE 0xFFu
#define LAB_DRAW_DECADE 48u
#define LAB_DRAW_T0 4u
#define LAB_DRAW_GATE_X 120u
#define LAB_DRAW_GATE_MAX_X 200u

typedef struct {
    uint8_t lo[LAB_DRAW_LAYERS];
    uint8_t hi[LAB_DRAW_LAYERS];
    uint8_t lit;
} lab_draw_col;

typedef void (*lab_draw_fn)(const void* ctx, uint32_t x, lab_draw_col* out);

typedef struct {
    lab_draw_fn fn;
    const void* ctx;
    uint8_t color[LAB_DRAW_LAYERS];
    uint8_t lit_color;
} lab_draw;

int lab_draw_apply(const lab_draw* d, grid* g, const screen* s, uint32_t row0, uint32_t* left);

#define LAB_DRAW_BLANK 0xFFu

uint32_t lab_draw_look(const live* l, const scope* sc, uint32_t page, uint32_t hl);
int lab_draw_page(const live* l, const scope* sc, uint32_t page, uint32_t hl, const theme* th,
                  grid* g, const screen* s, uint32_t row0, uint32_t* left);
void lab_draw_guides(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                     uint32_t row0);

uint32_t lab_draw_time_us(uint32_t x);
uint32_t lab_draw_x_of_us(uint32_t t);
uint32_t lab_draw_attack_us(uint32_t rate);
uint32_t lab_draw_decay_us(uint32_t rate);
uint32_t lab_draw_gate_us(uint8_t ad, uint8_t sr);
uint32_t lab_draw_level(uint8_t ad, uint8_t sr, uint32_t gate_us, uint32_t t);

#ifdef __cplusplus
}
#endif

#endif
