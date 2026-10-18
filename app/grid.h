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
 * logo, app/logo.h) or from a painter, a function that fills a cell's pixels as it is sent (a
 * pixel scene composited on demand, app/scene_px.h). Rows can be held: their changes wait
 * until they are released (the screen's wipe transition, app/screen.h). Large text
 * (grid_text_big) draws each character 2 or 3 times as large, from the same 8 x 8 font, over
 * scale x scale cells that each draw their own part of it: a cell's code is its part's (a
 * piece, GRID_BIG_FIRST..) and the grid keeps the character it is a piece of.
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
_Static_assert(GRID_OVERLAYS <= 255u,
               "a cell's byte holds 1 + its overlay slot, ovl_nfree the count");
#endif
/* A run's pixels: at most a full row of cells. */
#define GRID_RUN_PIXELS (GRID_COLS * GRID_CELL_PX * GRID_CELL_PX)
/* The cell codes of large text's pieces: GRID_BIG_FIRST + y * 2 + x for the cell (x, y) of a
 * character at 2 times the font (4 pieces), then GRID_BIG_FIRST + 4 + y * 3 + x at 3 times
 * (9 pieces), then GRID_PIECE_PLAIN: a glyph at its own size that no overlay shows over
 * (grid_put_top). They are blank glyphs in the font, so a cell that holds one and is drawn by
 * anything else, as an ordinary cell, shows what it draws. */
#define GRID_BIG_FIRST 0x01u
#define GRID_PIECE_PLAIN (GRID_BIG_FIRST + 13u)
#define GRID_BIG_PIECES 14u

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

/* A painter region's function: fills the 8 x 8 background pixels of cell (col, row), as
 * colours (0..15), row by row, into px, as the cell is sent. */
typedef void (*grid_painter)(void* ctx, uint32_t col, uint32_t row, uint8_t px[64]);

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
     * show the bitmap's cells from (region_bx, region_by), or (grid_region_painter) what
     * region_painter paints for them. Both NULL: none. */
    const grid_bitmap* region_bitmap;
    uint8_t region_col;
    uint8_t region_row;
    uint8_t region_w;
    uint8_t region_h;
    uint8_t region_bx;
    uint8_t region_by;
    grid_painter region_painter;
    void* region_ctx;
    /* A painter region's cells a run takes at most (grid_region_painter). */
    uint8_t region_run;
    /* The character a large-text or plain piece is a piece of (ch holds the piece), for each
     * cell. */
    uint8_t src[GRID_CELLS];
    /* The rows grid_text_big leaves alone (bit r: row r): a layer drawing over a view's own
     * large text (app/screen.h) guards its rows so that the view's compose does not move it. */
    uint32_t big_guard;
    uint8_t big_touched; /* a piece's character changed (grid_big_touched) */
} grid;

/* A frame composed off the grid by a view that copies it onto the grid cell by cell (app/chip_view.c,
 * app/song_view.c): each cell's code and colour, and for a piece of large text (ch in
 * GRID_BIG_FIRST..GRID_BIG_FIRST + GRID_BIG_PIECES - 1) the character it is a piece of, as the
 * grid keeps it. */
typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
    uint8_t src[GRID_ROWS][GRID_COLS];
} grid_canvas;

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
/* Large text: s at scale 2 or 3 from (col, row): each character is scale x scale cells, its
 * glyph pixel-doubled or tripled and cut into the cells. The text is clipped at the row's end
 * (whole characters). It uses the cells' colours: foreground for the glyph, background
 * elsewhere; their overlays and a pixel region do not show. Rows under grid_big_guard are
 * left alone. Returns the column after it. A cell the text leaves is the next view's, drawn
 * as any other. */
uint32_t grid_text_big(grid* g, uint32_t col, uint32_t row, uint32_t scale, const char* s,
                       uint8_t attr);
/* s at scale 2 or 3 from (col, row) into the canvas c, as grid_text_big does into the grid
 * (clipped at the row's end, whole characters); none of the grid's guards apply. Returns the
 * column after it. */
uint32_t grid_canvas_text_big(grid_canvas* c, uint32_t col, uint32_t row, uint32_t scale,
                              const char* s, uint8_t attr);
/* 1 if canvas cell (col, row) differs from the grid's: its code or colour, or for a piece its
 * character. */
int grid_canvas_differs(const grid* g, const grid_canvas* c, uint32_t col, uint32_t row);
/* Canvas cell (col, row) onto the grid, its piece's character too (a changed one is sent again). */
void grid_canvas_put(grid* g, const grid_canvas* c, uint32_t col, uint32_t row);
/* A whole row of the canvas onto the grid, as grid_row does for a row without pieces (a row that
 * shows what it showed marks nothing to send), the pieces' characters too. */
void grid_canvas_row(grid* g, const grid_canvas* c, uint32_t row);
/* Cell (col, row) in glyph ch at its own size, over everything: no overlay shows on it (a layer
 * drawing over a view that has overlays, app/screen.h's popup). Cells outside are ignored. */
void grid_put_top(grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr);
/* A w x h block of grid_put_top. */
void grid_fill_top(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                   uint8_t attr);
/* Rows grid_text_big leaves alone (bit r: row r). */
void grid_big_guard(grid* g, uint32_t rows);
/* 1 if a piece's character changed (its cell is sent again) since the last call; clears it. */
int grid_big_touched(grid* g);
/* The w x h cells from (col, row) are no longer marked to be sent again for their own sake
 * (an overlay under large text changed): they go out only if their glyph or colours differ
 * from the panel's. Cells outside the grid are ignored. */
void grid_unstale(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h);
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
/* The colour of the cell's overlay, its mask as it is (sent again only if the colour differs);
 * nothing for a cell with no overlay. */
void grid_overlay_color(grid* g, uint32_t col, uint32_t row, uint8_t color);
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
 * nothing, and one over the same cells of the same bitmap (grown or shrunk, as a picture
 * revealed a few columns a frame) sends only the cells it adds or leaves. Returns 0, or -1
 * when the rectangle is empty or leaves the grid or the bitmap (nothing changes). */
int grid_region(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                const grid_bitmap* b, uint32_t bx, uint32_t by);
/* A painter region: the w x h cells from (col, row) take their background from fn(ctx, col,
 * row, px), called for each as it is sent, as a bitmap region's take it from the bitmap (the
 * glyphs' foreground and the overlays draw over it). The grid sends only the cells that
 * changed, so the painter's owner marks the cells whose pixels it changed (grid_mark). A run
 * takes at most `run` of the region's cells (1..GRID_COLS), so that what grid_render_run
 * spends painting a run is bounded: a run that reaches it ends there, and the next goes on.
 * It replaces a bitmap region, and the reverse; the same painter, context, cells and cap
 * again send nothing, others send the cells either covers. Returns 0, or -1 when fn is NULL,
 * run is out of range or the rectangle is empty or leaves the grid (nothing changes). */
int grid_region_painter(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h,
                        grid_painter fn, void* ctx, uint32_t run);
/* No region: its cells are sent again in their background colour. */
void grid_region_clear(grid* g);
/* Pixels [x, x + n) of the bitmap's row y, as colours, into out (x + n <= b->w, y < b->h). */
void grid_bitmap_row(const grid_bitmap* b, uint32_t x, uint32_t y, uint32_t n, uint8_t* out);

/* Cell (col, row) to be sent again (a painter region's pixels changed under it). Cells
 * outside the grid are ignored. */
void grid_mark(grid* g, uint32_t col, uint32_t row);
/* 1 if cell (col, row) differs from the panel or is marked to be sent; cells outside: 0. */
int grid_marked(const grid* g, uint32_t col, uint32_t row);

/* Cells that differ from the panel (grid_next_run would send them), held rows included: what
 * a view that holds itself to a budget has spent of it so far. */
uint32_t grid_pending(const grid* g);
/* The same in the w x h cells from (col, row), clipped to the grid. */
uint32_t grid_pending_in(const grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h);

/* Holds the rows in mask (bit r: row r) and releases the others: grid_next_run passes over held
 * rows, and their changes go out once they are released. */
void grid_hold(grid* g, uint32_t mask);

/* The next run of changed cells, in screen order from where the last call stopped (wrapping
 * to the top for cells that changed behind it), counted as shown from now on: adjacent
 * changed cells of one row, at most a painter region's cap of the region's cells. 0 when the
 * panel shows the composed frame, held rows aside. */
int grid_next_run(grid* g, grid_run* run);
/* A run's pixels into px: 8 rows of run->len * 8 pixels, top row first, as one LCD window
 * takes them. pal maps the 16 colours to the panel's pixel format. px is 4-byte aligned: two
 * pixels go in a 32-bit store. The region's cells in the run decode the bitmap's runs a pixel
 * row at a time, or are painted a cell at a time. */
void grid_render_run(const grid* g, const grid_run* run, const uint16_t pal[16], uint16_t* px);

#ifdef __cplusplus
}
#endif

#endif
