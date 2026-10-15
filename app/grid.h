// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The cell-grid renderer: the 240 x 240 screen as 30 x 30 cells of 8 x 8 pixels, each a font
 * code (app/font.h) and a colour byte (docs/art/README.md).
 *
 * The grid keeps the frame being composed and the frame the panel shows. Only cells that
 * differ go out: grid_next_run hands out the changed cells one run at a time, a run being
 * horizontally adjacent changed cells on one row (one LCD window), and counts them as shown;
 * grid_render_run turns a run into pixels. A cell can carry a pixel overlay, a one-colour
 * 8 x 8 mask drawn over its glyph (oscilloscopes, the logo). A rectangle of cells can be a
 * pixel region, taking its background pixel for pixel from a bitmap in flash (a pixel-art
 * logo, app/logo.h). Rows can be held: their changes wait until they are released (the
 * screen's wipe transition, app/screen.h).
 *
 * Portable C with no hardware access: platform/fm1/ui.c sends the runs, the host tests
 * drive it directly. */
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
/* Cells that can carry a pixel overlay at once: LIVE's three scopes (M1e Task 4) take about
 * 240. 255 keeps a slot index (plus 1) in the cell's byte. A slot costs 10 bytes of RAM
 * (mask, colour, free-list entry): 2,550 bytes, 2.3 KB more than M1e Task 1's 32 slots. */
#define GRID_OVERLAYS 255u
#ifndef __cplusplus
_Static_assert(GRID_OVERLAYS <= 255u, "a cell's byte holds 1 + its overlay slot, ovl_nfree the count");
#endif
/* A run's pixels: at most a full row of cells. */
#define GRID_RUN_PIXELS (GRID_COLS * GRID_CELL_PX * GRID_CELL_PX)

/* The 16 colours (docs/art/README.md), indexes into grid_palette. */
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

/* A cell's colour byte: foreground in the low nibble, background in the high one. */
#define GRID_ATTR(fg, bg) ((uint8_t)((((bg) & 15u) << 4) | ((fg) & 15u)))

/* The palette as RGB565 (red in the top 5 bits), in flash. */
extern const uint16_t grid_palette[16];

/* A bitmap of w x h pixels (whole cells), each a colour (0..15), in flash, run-length coded
 * row by row so that any row decodes on its own: row y's runs start at data[rows[y]], each a
 * byte, ((length - 1) << 4) | colour for 1..16 pixels, and they add up to w exactly. Rows
 * alike may share their runs. host/crates/sidou-tests/src/logo.rs writes them from indexed
 * PNGs (app/logo_bitmap.c). */
typedef struct {
    uint16_t w;
    uint16_t h;
    const uint16_t* rows;
    const uint8_t* data;
} grid_bitmap;

/* Changed cells [col, col + len) of row. */
typedef struct {
    uint8_t row;
    uint8_t col;
    uint8_t len;
} grid_run;

typedef struct {
    /* The frame being composed. */
    uint8_t ch[GRID_CELLS];
    uint8_t attr[GRID_CELLS];
    uint8_t ovl[GRID_CELLS]; /* 0, or 1 + the cell's overlay slot */
    /* The frame the panel shows. */
    uint8_t shown_ch[GRID_CELLS];
    uint8_t shown_attr[GRID_CELLS];
    /* Bit per cell: resend it even if ch and attr match (its overlay changed, or the panel's
     * content is unknown). */
    uint8_t stale[(GRID_CELLS + 7u) / 8u];
    uint8_t ovl_mask[GRID_OVERLAYS][8]; /* bit 7 the leftmost pixel, as the font */
    uint8_t ovl_color[GRID_OVERLAYS];
    /* The free slots, a stack: ovl_free[0 .. ovl_nfree) */
    uint8_t ovl_free[GRID_OVERLAYS];
    uint8_t ovl_nfree;
    /* Bit r: row r may differ from the panel. Set by every change, cleared once a scan of the
     * whole row finds nothing to send, so a frame that changes nothing costs nothing. */
    uint32_t rows;
    /* Bit r: row r is held (grid_hold). */
    uint32_t hold;
    /* Where grid_next_run goes on. */
    uint8_t scan_row;
    uint8_t scan_col;
    /* The pixel region (grid_region): region_w x region_h cells from (region_col, region_row)
     * show the bitmap's cells from (region_bx, region_by). NULL: none. */
    const grid_bitmap* region_bitmap;
    uint8_t region_col;
    uint8_t region_row;
    uint8_t region_w;
    uint8_t region_h;
    uint8_t region_bx;
    uint8_t region_by;
} grid;

/* Every cell ch in attr, no overlays, no region, and all of it to be sent: what the panel
 * shows is unknown. */
void grid_init(grid* g, uint8_t ch, uint8_t attr);
/* Sends everything again on the next runs (e.g. after the panel was drawn by someone else). */
void grid_invalidate(grid* g);

/* Cells outside the grid are ignored. */
void grid_put(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr);
/* A whole row: cell c gets ch[c] in attr[c], as grid_put on each (a view that composes into
 * a buffer of its own writes each cell once, so a frame that changes nothing marks nothing). */
void grid_row(grid* g, uint32_t row, const uint8_t ch[GRID_COLS], const uint8_t attr[GRID_COLS]);
/* A NUL-terminated string from (col, row), clipped at the row's end. Returns the column after
 * it. Codes are font codes, so ASCII shows as itself. */
uint32_t grid_text(grid* g, uint32_t col, uint32_t row, const char* s, uint8_t attr);
/* A w x h block of ch. */
void grid_fill(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
               uint8_t attr);
/* A frame of box glyphs around w x h cells (w, h >= 2), with rounded corners if round. */
void grid_box(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
              int round);

/* Draws mask (8 rows, bit 7 the leftmost pixel) in colour over the cell's glyph until
 * grid_overlay_clear. Setting the same mask and colour again sends nothing. Returns 0, or -1
 * when all GRID_OVERLAYS slots are taken (the cell keeps its glyph alone). */
int grid_overlay(grid* g, uint32_t col, uint32_t row, const uint8_t mask[8], uint8_t color);
void grid_overlay_clear(grid* g, uint32_t col, uint32_t row);
/* Every overlay (a view switch). */
void grid_overlay_clear_all(grid* g);
/* 1 if the cell's overlay is mask in colour already, or if it has none and mask is empty: then
 * grid_overlay (or grid_overlay_clear, for an empty mask) would send nothing. Cells outside
 * the grid: 1. */
int grid_overlay_same(const grid* g, uint32_t col, uint32_t row, const uint8_t mask[8],
                      uint8_t color);
/* A pixel region: the w x h cells from (col, row) take their background, pixel for pixel,
 * from the bitmap's cells from (bx, by) (in cells), instead of their background colour. Their
 * glyphs' foreground and their overlays draw over it as anywhere else, so text and overlays
 * stay possible on it; a blank glyph shows the bitmap alone. One region at a time: a new one
 * replaces the last, and the cells either covers are sent again; the same one again sends
 * nothing. Returns 0, or -1 when the rectangle is empty or leaves the grid or the bitmap
 * (nothing changes). */
int grid_region(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                const grid_bitmap* b, uint32_t bx, uint32_t by);
/* No region: its cells are sent again in their background colour. */
void grid_region_clear(grid* g);
/* Pixels [x, x + n) of the bitmap's row y, as colours, into out (x + n <= b->w, y < b->h). */
void grid_bitmap_row(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n, uint8_t* out);

/* Cells that differ from the panel (grid_next_run would send them), held rows included: what
 * a view that holds itself to a budget has spent of it so far. */
uint32_t grid_pending(const grid* g);

/* Holds the rows in mask (bit r: row r) and releases the others: grid_next_run passes over held
 * rows, and their changes go out once they are released. */
void grid_hold(grid* g, uint32_t mask);

/* The next run of changed cells, in screen order from where the last call stopped (wrapping
 * to the top for cells that changed behind it), counted as shown from now on. 0 when the
 * panel shows the composed frame, held rows aside. */
int grid_next_run(grid* g, grid_run* run);
/* A run's pixels into px: 8 rows of run->len * 8 pixels, top row first, as one LCD window
 * takes them. pal maps the 16 colours to the panel's pixel format. px is 4-byte aligned: two
 * pixels go in a 32-bit store. The region's cells in the run decode the bitmap's runs a pixel
 * row at a time. */
void grid_render_run(const grid* g, const grid_run* run, const uint16_t pal[16], uint16_t* px);

#ifdef __cplusplus
}
#endif

#endif
