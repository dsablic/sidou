// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GRID_H
#define SIDOU_GRID_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define GRID_COLS 30u
#define GRID_ROWS 30u
#define GRID_CELLS (GRID_COLS * GRID_ROWS)
#define GRID_CELL_PX 8u
#define GRID_BITMAP_SEEK 64u
#define GRID_OVERLAYS 255u
#ifndef __cplusplus
_Static_assert(GRID_OVERLAYS <= 255u,
               "a cell's byte holds 1 + its overlay slot, ovl_nfree the count");
#endif
#define GRID_RUN_PIXELS (GRID_COLS * GRID_CELL_PX * GRID_CELL_PX)
#define GRID_BIG_FIRST 0x01u
#define GRID_PIECE_PLAIN (GRID_BIG_FIRST + 13u)
#define GRID_BIG_PIECES 14u

#define GRID_BLACK 0u
#define GRID_WHITE 1u
#define GRID_RED 2u
#define GRID_CYAN 3u
#define GRID_PURPLE 4u
#define GRID_GREEN 5u
#define GRID_BLUE 6u
#define GRID_YELLOW 7u
#define GRID_ORANGE 8u
#define GRID_BROWN 9u
#define GRID_LIGHT_RED 10u
#define GRID_DARK_GREY 11u
#define GRID_GREY 12u
#define GRID_LIGHT_GREEN 13u
#define GRID_LIGHT_BLUE 14u
#define GRID_LIGHT_GREY 15u

#define GRID_ATTR(fg, bg) ((uint8_t)((((bg) & 15u) << 4) | ((fg) & 15u)))

extern const uint16_t grid_palette[16];

typedef struct {
    uint16_t w;
    uint16_t h;
    const uint16_t* rows;
    const uint8_t* data;
} grid_bitmap;

typedef void (*grid_painter)(void* ctx, uint32_t col, uint32_t row, uint8_t px[64]);

typedef struct {
    uint8_t row;
    uint8_t col;
    uint8_t len;
} grid_run;

typedef struct {
    uint8_t ch[GRID_CELLS];
    uint8_t attr[GRID_CELLS];
    uint8_t ovl[GRID_CELLS];
    uint8_t shown_ch[GRID_CELLS];
    uint8_t shown_attr[GRID_CELLS];
    uint8_t stale[(GRID_CELLS + 7u) / 8u];
    uint8_t ovl_mask[GRID_OVERLAYS][8];
    uint8_t ovl_color[GRID_OVERLAYS];
    uint8_t ovl_free[GRID_OVERLAYS];
    uint8_t ovl_nfree;
    uint32_t rows;
    uint32_t hold;
    uint8_t scan_row;
    uint8_t scan_col;
    const grid_bitmap* region_bitmap;
    uint8_t region_col;
    uint8_t region_row;
    uint8_t region_w;
    uint8_t region_h;
    uint8_t region_bx;
    uint8_t region_by;
    grid_painter region_painter;
    void* region_ctx;
    uint8_t region_run;
    uint8_t src[GRID_CELLS];
    uint32_t big_guard;
    uint8_t big_touched;
    grid_painter region_peek;
} grid;

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
    uint8_t src[GRID_ROWS][GRID_COLS];
} grid_canvas;

void grid_init(grid* g, uint8_t ch, uint8_t attr);
void grid_invalidate(grid* g);

void grid_put(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr);
void grid_row(grid* g, uint32_t row, const uint8_t ch[GRID_COLS], const uint8_t attr[GRID_COLS]);
uint32_t grid_text(grid* g, uint32_t col, uint32_t row, const char* s, uint8_t attr);
uint32_t grid_text_big(grid* g, uint32_t col, uint32_t row, uint32_t scale, const char* s,
                       uint8_t attr);
uint32_t grid_canvas_text_big(grid_canvas* c, uint32_t col, uint32_t row, uint32_t scale,
                              const char* s, uint8_t attr);
void grid_canvas_text(grid_canvas* c, uint32_t col, uint32_t row, const char* s, uint8_t attr);
int grid_canvas_differs(const grid* g, const grid_canvas* c, uint32_t col, uint32_t row);
void grid_canvas_put(grid* g, const grid_canvas* c, uint32_t col, uint32_t row);
void grid_canvas_row(grid* g, const grid_canvas* c, uint32_t row);
void grid_put_top(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr);
void grid_fill_top(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                   uint8_t attr);
void grid_big_guard(grid* g, uint32_t rows);
int grid_big_touched(grid* g);
void grid_unstale(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h);
void grid_fill(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
               uint8_t attr);
void grid_box(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
              int round);

int grid_overlay(grid* g, uint32_t col, uint32_t row, const uint8_t mask[8], uint8_t color);
void grid_overlay_clear(grid* g, uint32_t col, uint32_t row);
void grid_overlay_color(grid* g, uint32_t col, uint32_t row, uint8_t color);
void grid_overlay_clear_all(grid* g);
int grid_overlay_same(const grid* g, uint32_t col, uint32_t row, const uint8_t mask[8],
                      uint8_t color);
int grid_region(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                const grid_bitmap* b, uint32_t bx, uint32_t by);
int grid_region_painter(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                        grid_painter fn, void* ctx, uint32_t run);
void grid_region_clear(grid* g);
void grid_bitmap_row(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n, uint8_t* out);

void grid_mark(grid* g, uint32_t col, uint32_t row);
int grid_marked(const grid* g, uint32_t col, uint32_t row);

uint32_t grid_pending(const grid* g);
uint32_t grid_pending_in(const grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h);

void grid_hold(grid* g, uint32_t mask);

int grid_next_run(grid* g, grid_run* run);
void grid_render_run(const grid* g, const grid_run* run, const uint16_t pal[16], uint16_t* px);

#ifdef __cplusplus
}
#endif

#endif
