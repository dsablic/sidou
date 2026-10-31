// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WIDGET_H
#define SIDOU_WIDGET_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MENU_VALUE_CHARS 16u
#define MENU_MAX_ROWS 8u
#define MENU_LINE_CELLS 60u
#define MENU_BUDGET_FORCED (1u << 31)

typedef struct {
    const char* label;
    void (*value)(void* ctx, char* buf);
    void (*turn)(void* ctx, int32_t steps);
} menu_item;

typedef struct {
    const menu_item* items;
    void* ctx;
    uint8_t count;
    uint8_t rows;
    uint8_t cursor;
    uint8_t top;
    uint8_t scale;
    uint8_t sel_attr;
    uint8_t value_attr;
    uint8_t barred;
    uint16_t budget;
    uint8_t lines;
    uint8_t pending;
    uint8_t valid;
    uint32_t force_rows;
} menu;

void widget_frame(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
                  const char* title);
void widget_screen(grid* g, uint8_t ch, uint8_t attr);
void widget_number(grid* g, uint32_t col, uint32_t row, uint32_t width, int32_t value, int sign,
                   uint8_t attr);
void widget_bar(grid* g, uint32_t col, uint32_t row, uint32_t cells, uint32_t value, uint32_t max,
                uint8_t attr);

void widget_blank_pieces(grid* g, uint32_t col, uint32_t row, uint32_t w);

void menu_init(menu* m, const menu_item* items, uint32_t count, uint32_t rows, void* ctx);
void menu_move(menu* m, int32_t steps);
void menu_turn(menu* m, int32_t steps);
void menu_draw(menu* m, grid* g, uint32_t col, uint32_t row, uint32_t w, uint8_t attr,
               int cursor_on);

#ifdef __cplusplus
}
#endif

#endif
