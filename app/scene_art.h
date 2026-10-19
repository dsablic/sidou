// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Pixel art for SCENE's drawn vignettes (docs/art/README.md "RED MOON RITE"), in flash, and
 * its decoders.
 *
 * A scene_image is a picture with transparency: a frame of fw x fh stored pixels, of which
 * the w x h rectangle from (x, y) is coded; the rest of the frame is transparent, or, for a
 * patch, the base image's. Each stored pixel is gx screen pixels wide (2 for the C64's wide
 * multicolour pixels, 1 for backgrounds). Rows use the shared pool format in raw-run mode
 * at SCENE_ART_SEEK pixels (docs/art/bitmap-codec.md): rows[r] is a byte offset from data,
 * the runs adding up to w exactly. Row packets share byte substrings within a pool.
 * The colour `clear` (one the image
 * never shows; the converter picks one that no image of the set uses) codes a transparent
 * run, so a row decodes with the same loop as an opaque bitmap. A row wider than
 * SCENE_ART_SEEK has a run start at every SCENE_ART_SEEK pixels of it (the converter splits a
 * run there), and the row packet holds its checkpoints, so a span part way along a row is found
 * from the checkpoint before it instead of from the row's start.
 *
 * The decoders trust the converter for what they do not check: every row's runs add up to
 * w exactly (the round-trip tests decode every row), a patch's frame (fw, fh, gx) is its
 * base's (the converter asserts it as it writes the patch), and gx is 1 or 2. Breaking one
 * reads past a row or draws in the wrong place.
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
/* Stored pixels between a row's checkpoints (a power of 2). */
#define SCENE_ART_SEEK_LOG2 5u
#define SCENE_ART_SEEK (1u << SCENE_ART_SEEK_LOG2)

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
/* Draws the image's screen pixels [x, x + n) of its frame's rows [y, y + h) (x counted from
 * the frame's left as drawn, in screen pixels: a stored pixel is gx of them; x + n <= fw * gx,
 * y + h <= fh) over out, out[0] being pixel x of row y and each row stride bytes after the
 * last: its opaque pixels as their colour, through remap if not NULL (16 colours);
 * transparent ones leave out as it is. Mirrored, screen pixel x shows stored pixel
 * fw - 1 - x / gx. A run at a time: what a compositor draws a sprite with. */
void scene_image_blend(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, uint32_t h,
                       uint32_t stride, int mirror, const uint8_t* remap, uint8_t* out);

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
/* The status bar (rows 0-15): black, VS and the red line, opaque; and side 0's broadsword
 * meter as three layers, its frames at (0, 0) on the screen: empty (the hilt and the blade's
 * dark ghost), full (the hilt and eight notches, drawn clipped to the stamina left) and the
 * tip (5 x 5, drawn where the notches end). Side 1 is the mirror image: a frame drawn
 * mirrored at 240 minus its width. */
extern const scene_image scene_rmr_hud;
extern const scene_image scene_rmr_blade_empty;
extern const scene_image scene_rmr_blade_full;
extern const scene_image scene_rmr_blade_tip;
/* The meter's geometry (red-moon-rite.js's BLADE_X, BLADE_N): notch k of side 0 starts at
 * x = SCENE_RMR_BLADE_X + k * SCENE_RMR_BLADE_N, the tip at the first notch lost, on row
 * SCENE_RMR_BLADE_Y. */
#define SCENE_RMR_BLADE_X 17
#define SCENE_RMR_BLADE_N 11
#define SCENE_RMR_BLADE_Y 6

/* The motion the script computes once (assets/scene/red-moon-rite-motion.txt). */
typedef struct {
    int8_t dx; /* the sword's grip, from the disarmed fighter's place when it starts */
    uint8_t y; /* on the screen */
    uint8_t k; /* the rotation: scene_rmr_sword[k] */
} scene_rmr_grip;
/* A fighter's sword knocked from his hand: `frames` in the air (the grip at frames
 * 1 .. frames - 1, path[n - 1]), then stuck point-down with its grip at `stuck`. */
typedef struct {
    uint8_t frames;
    const scene_rmr_grip* path;
    scene_rmr_grip stuck;
} scene_rmr_flight;
/* [fighter disarmed] */
extern const scene_rmr_flight scene_rmr_flights[2];
/* A parry's spark at one stage: rows dy0 .. dy0 + rows - 1 around the point where the blades
 * meet, row r's pixels at dx[at[r] .. at[r + 1]) (left to right) in colour[]. */
typedef struct {
    int8_t dy0;
    uint8_t rows;
    const uint8_t* at;
    const int8_t* dx;
    const uint8_t* colour;
} scene_rmr_spark;
/* Stages 1, 2 and 3 at [0], [1], [2]: a parry shows 3, 2, then 1. */
extern const scene_rmr_spark scene_rmr_sparks[3];

#ifdef __cplusplus
}
#endif

#endif
