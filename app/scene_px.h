// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* A pixel scene composited a tile at a time, on demand: no frame buffer (240 x 240 at 4 bits
 * would be 28.8 KB). docs/art/README.md "Pixel scenes".
 *
 * The scene is a stack of layers in slots, drawn bottom first: fills, gradients (a computed
 * sky), images (app/scene_art.h: backgrounds and sprites, each placed by its frame's top-left,
 * mirrored or not, with an optional colour remap), marks (single pixels: sparks, arcs, small
 * discs), text in the 8 x 8 font and calls (a function that paints a row span: colour
 * cycling over a mask). Each layer knows its box, so a tile visits only the layers over it.
 *
 * The scene owns the grid's painter region (grid_region_painter): the grid sends only the
 * cells it marks, and composites each again as it sends it. Which cells to mark: setting a
 * slot to a layer that differs from what it held makes candidates of the tiles under both
 * its old and its new box (scene_px_touch adds any others); scene_px_scan composites each
 * candidate into a scratch tile, hashes it, and marks it only if the hash differs from that
 * of what the panel shows. The hash of a tile is taken again as it is sent, so it is always
 * the panel's. The scan works in slices (a number of tiles a call) between frames, so a pass
 * of the main loop stays within its allowance; the grid sends the marked cells round the
 * screen from where it stopped, so none waits more than a sweep.
 *
 * A layer's identity is its fields: a layer whose drawing changes while its fields do not
 * (a mark array rewritten in place, a call whose output moves with time) changes `param`. */
#ifndef SIDOU_SCENE_PX_H
#define SIDOU_SCENE_PX_H

#include <stdint.h>

#include "grid.h"
#include "scene_art.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Layer slots: RED MOON RITE needs about 20 (the HUD, the sky's bands, the arena, the
 * torches, two shadows and fighters, the swords, the spark, the arc). */
#define SCENE_PX_LAYERS 24u
#define SCENE_PX_W 240
#define SCENE_PX_H 240

enum {
    SCENE_PX_NONE,     /* an empty slot */
    SCENE_PX_FILL,     /* colour over [x0, x1) x [y0, y1) */
    SCENE_PX_GRADIENT, /* src: a scene_gradient, over its rows within [y0, y1) and [x0, x1) */
    SCENE_PX_IMAGE,    /* src: a scene_image, its frame's top-left at (x, y), within the clip */
    SCENE_PX_MARKS,    /* src: n scene_px_marks */
    SCENE_PX_TEXT,     /* src: n font codes from (x, y), in colour, transparent between */
    SCENE_PX_CALL      /* fn(src, x, y, n, out) over [x0, x1) x [y0, y1) */
};

typedef struct {
    int16_t x, y;
    uint8_t colour;
} scene_px_mark;

/* Paints pixels [x, x + n) of screen row y into out (out[0] is pixel x), leaving alone the
 * pixels it does not draw. */
typedef void (*scene_px_fn)(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out);

typedef struct {
    uint8_t kind;
    uint8_t colour;  /* FILL, TEXT */
    uint8_t mirror;  /* IMAGE */
    uint8_t n;       /* MARKS, TEXT: how many */
    int16_t x, y;    /* IMAGE: the frame's top-left; TEXT: the first glyph's */
    /* The clip, [x0, x1) x [y0, y1) on the screen: FILL's rectangle, CALL's box; GRADIENT and
     * IMAGE draw within it (a full screen for none); MARKS and TEXT ignore it. */
    int16_t x0, y0, x1, y1;
    const void* src;
    const uint8_t* remap; /* IMAGE: 16 colours, or NULL */
    scene_px_fn fn;       /* CALL */
    uint32_t param;
} scene_px_layer;

typedef struct {
    scene_px_layer layer[SCENE_PX_LAYERS];
    /* Each slot's box on the screen, clipped to it: [bx0, bx1) x [by0, by1); empty if
     * bx0 >= bx1. */
    int16_t box[SCENE_PX_LAYERS][4];
    /* The hash of each tile as the panel shows it (taken as it is sent). */
    uint32_t hash[GRID_CELLS];
    uint8_t cand[(GRID_CELLS + 7u) / 8u]; /* bit per tile: to be composited and compared */
    uint16_t ncand;
    uint16_t next; /* where the scan goes on */
    /* Tiles composited: by the scan, and as the grid sent them. */
    uint32_t scanned;
    uint32_t painted;
} scene_px;

/* Empty slots, and the whole screen the grid's painter region, every cell to be sent. */
void scene_px_init(scene_px* sp, grid* g);
/* Slot i holds l from now on (l NULL: empty). If that differs from what it held, the tiles
 * under its old and new boxes become candidates. */
void scene_px_set(scene_px* sp, uint32_t i, const scene_px_layer* l);
/* The tiles under [x0, x1) x [y0, y1) become candidates. */
void scene_px_touch(scene_px* sp, int32_t x0, int32_t y0, int32_t x1, int32_t y1);
/* Composites up to `most` candidates and marks on g those that changed; candidates the grid
 * is to send already are dropped uncomposited. Returns the candidates left. */
uint32_t scene_px_scan(scene_px* sp, grid* g, uint32_t most);
/* Tile (col, row) composited, 64 colours row by row. */
void scene_px_tile(const scene_px* sp, uint32_t col, uint32_t row, uint8_t out[64]);
/* The hash scene_px keeps of a tile's 64 colours. */
uint32_t scene_px_hash(const uint8_t px[64]);

#ifdef __cplusplus
}
#endif

#endif
