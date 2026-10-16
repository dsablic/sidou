// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Pixel art for SCENE's drawn vignettes (docs/art/README.md "RED MOON RITE"), in flash, and
 * its decoders.
 *
 * A scene_image is a picture with transparency: a frame of fw x fh stored pixels, of which
 * the w x h rectangle from (x, y) is coded; the rest of the frame is transparent, or, for a
 * patch, the base image's. Each stored pixel is gx screen pixels wide (2 for the C64's wide
 * multicolour pixels, 1 for backgrounds). Rows are run-length coded as grid_bitmap's
 * (app/grid.h): row r of the rectangle starts at data[rows[r]], a byte a run,
 * ((length - 1) << 4) | colour for 1..16 pixels, the runs adding up to w exactly; rows alike
 * share their runs, within an image and across images. The colour `clear` (one the image
 * never shows; the converter picks one that no image of the set uses) codes a transparent
 * run, so a row decodes with the same loop as an opaque bitmap.
 *
 * (ax, ay) is the anchor, in screen pixels of the unmirrored frame: the point the drawing
 * places (a fighter's feet, a sword's grip). Drawn mirrored, the frame's stored pixel x is
 * read from fw - 1 - x and the anchor is (fw * gx - ax, ay); no mirrored copies are stored.
 *
 * A scene_gradient is a raster-split sky computed on the device: flat bands of colour from
 * each stop's row, each joined to the next by a seam of `seam` rows in the 4 x 4 Bayer order
 * on wide pixels (the prototype's gradient()).
 *
 * host/crates/sidou-tests/src/scene_art.rs writes the images from the PNGs that
 * assets/scene/red-moon-rite.js draws (app/scene_rmr_art.c). */
#ifndef SIDOU_SCENE_ART_H
#define SIDOU_SCENE_ART_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* What scene_image_row writes for a transparent pixel. */
#define SCENE_ART_CLEAR 0xffu

typedef struct scene_image scene_image;
struct scene_image {
    uint8_t fw, fh;   /* the frame, in stored pixels */
    uint8_t x, y;     /* the coded rectangle's place in the frame */
    uint8_t w, h;     /* its size (w x h > 0) */
    uint8_t gx;       /* screen pixels a stored pixel */
    uint8_t clear;    /* the colour that codes a transparent run */
    uint8_t ax, ay;   /* the anchor, in screen pixels */
    const uint16_t* rows; /* h offsets into data */
    const uint8_t* data;
    const scene_image* base; /* NULL, or a patch's base: the frame outside the rectangle */
};

/* Stored pixels [x, x + n) of the frame's row y (x + n <= fw, y < fh), as colours, or
 * SCENE_ART_CLEAR where transparent, into out. Mirrored, pixel x is the frame's pixel
 * fw - 1 - x (out still runs left to right). */
void scene_image_row(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, int mirror,
                     uint8_t* out);

typedef struct {
    uint8_t y;      /* the band's first row, on the screen */
    uint8_t colour;
} scene_stop;

typedef struct {
    uint8_t y0, y1; /* the rows it covers, [y0, y1) */
    uint8_t seam;   /* rows of dither before each stop after the first */
    uint8_t n;      /* stops, the first at y0 */
    const scene_stop* stops;
} scene_gradient;

/* Screen pixels [x, x + n) of the gradient's row y (y0 <= y < y1, x + n <= 240) into out. */
void scene_gradient_row(const scene_gradient* g, uint32_t x, uint32_t y, uint32_t n,
                        uint8_t* out);

/* ---- RED MOON RITE (app/scene_rmr_art.c, generated) ---- */

/* The fighters' armed poses, in the prototype's DUEL order. */
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
/* The unarmed poses (after the disarm). */
enum {
    SCENE_RMR_UNARMED_GUARD,
    SCENE_RMR_UNARMED_GUARD2,
    SCENE_RMR_UNARMED_HIT,
    SCENE_RMR_UNARMED_REACH,
    SCENE_RMR_UNARMED_POSES
};
#define SCENE_RMR_SWORD_FRAMES 8u
#define SCENE_RMR_SKIES 3u
/* The fighters' feet line, on the screen: a fighter at x is drawn with his anchor at
 * (x, SCENE_RMR_FEET). */
#define SCENE_RMR_FEET 228u

/* [fighter][pose]: fighter 0 faces right, fighter 1 is drawn mirrored. */
extern const scene_image scene_rmr_barb[2][SCENE_RMR_POSES];
extern const scene_image scene_rmr_barb_unarmed[2][SCENE_RMR_UNARMED_POSES];
/* The sword in flight, rotation k pointing k x 45 degrees from straight down toward the
 * right, anchored at its grip; and stuck in the floor, anchored at its grip. */
extern const scene_image scene_rmr_sword[SCENE_RMR_SWORD_FRAMES];
extern const scene_image scene_rmr_sword_stuck;
/* The skies: each a gradient and the moon, wisps and hills over it (a frame of the screen's
 * rows 16-139, anchored at (0, 0) of the frame: it goes at screen row 16). */
extern const scene_gradient scene_rmr_sky_gradient[SCENE_RMR_SKIES];
extern const scene_image scene_rmr_sky[SCENE_RMR_SKIES];
#define SCENE_RMR_SKY_Y 16u
/* The temple and floor, a frame of the whole screen, transparent where the sky shows. */
extern const scene_image scene_rmr_arena;
/* The torches' flame areas, where the colour cycling draws: {x, y, w, h} on the screen. */
extern const uint8_t scene_rmr_flames[2][4];

#ifdef __cplusplus
}
#endif

#endif
