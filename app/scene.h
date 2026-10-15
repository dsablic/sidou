// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SCENE's pieces: generative PETSCII visuals that react to the three voices, after the browser
 * prototype's (docs/art/README.md "SCENE"). M1e's prototype of M2's SCENE.
 *
 * Each piece draws into a canvas of its own (scene_canvas, the 30 x 30 cells' glyphs and
 * colours), a frame at a time, from what the music does that frame (scene_music): the voices'
 * gates and the note-ons since the last frame, their notes, envelopes (ENV3) and colours, and
 * the filter's cutoff and resonance. The view (app/scene_view.h) fills scene_music from the
 * instrument and the audio's tap, and copies the canvas onto the grid within a budget of cells.
 * The canvas holds every cell, so a piece that changes a few cells a step (the ant) draws only
 * those, and the view's compose still draws every cell.
 *
 * The pieces, and what moves them:
 * - FIRE: three flames, a voice each (columns 0-9, 10-19, 20-29), in the bottom
 *   SCENE_FIRE_ROWS rows, two heat rows a cell (an upper-half glyph). A voice's envelope heats
 *   its flame, a note-on flares it, its pitch sets how high it burns (higher notes cool
 *   slower), the cutoff how hot (orange to white), the resonance lowers the cooling. A silent
 *   voice's flame dies down to the ember bed, which holds still. The fire burns every other
 *   frame, two steps at once (12.5 fps, rising as fast as at 25), and each frame draws half
 *   its rows, alternately, so a frame sends at most half the area.
 * - PLASMA: four sines over the screen in a dither ramp. The cutoff sets its speed, the
 *   resonance its twist, the voices' envelopes speed it up (silence runs an eighth of it) and
 *   voice 3's warms it; a note-on drops a bright ring, growing from a place its pitch sets
 *   (the pitch class across, the pitch up). A frame draws every other row, alternately.
 * - RULE30: Wolfram's rule 30 on a ring of 60 cells, a generation a row of quadrant pixels,
 *   written down the screen by a moving head that clears two rows ahead of it. Each note-on
 *   steps it once, the row in that voice's colour; voice 3's note-on also flips the cell its
 *   pitch picks; a held voice keeps it stepping with its envelope. A low cutoff darkens the
 *   colours.
 * - STARS: a starfield flying out of the centre, a third of the stars a voice's. A voice's
 *   envelope speeds its stars, a note-on lights them in its colour for a few frames, the
 *   latest note's pitch steers the centre across, and the cutoff sets the cruising speed.
 * - ANT: three Langton's ants, a voice each. A note-on walks its ant SCENE_ANT_STEPS steps and
 *   a sounding voice keeps it walking with its envelope; the trail takes the voice's colour,
 *   darker under a low cutoff. A silent voice's ant stops and greys.
 *
 * Randomness is a seeded xorshift (scene_random): the same music gives the same frames.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SCENE_H
#define SIDOU_SCENE_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_VOICES 3u
#define SCENE_PIECES 5u
#define SCENE_FIRE 0u
#define SCENE_PLASMA 1u
#define SCENE_RULE30 2u
#define SCENE_STARS 3u
#define SCENE_ANT 4u

/* No note (app/live.h LIVE_NO_NOTE). */
#define SCENE_NO_NOTE 0xFFu
/* The cutoff under which RULE30's and the ant's colours darken (0..2047). */
#define SCENE_DARK_CUTOFF 512u

/* FIRE: the rows it burns in, at the bottom; two heat rows a cell. */
#define SCENE_FIRE_ROWS 14u
#define SCENE_FIRE_H (2u * SCENE_FIRE_ROWS)
/* RULE30: the ring's cells and generations, two a cell each way. */
#define SCENE_RULE30_W (2u * GRID_COLS)
#define SCENE_RULE30_H (2u * GRID_ROWS)
/* RULE30: the generations a frame steps at most. */
#define SCENE_RULE30_MAX_STEPS 3u
/* RULE30: the generations drawn when it starts (in grey), so it starts with its pattern. */
#define SCENE_RULE30_START 24u
#define SCENE_STARS_N 90u
/* STARS: frames a star stays lit after its voice's note-on. */
#define SCENE_STARS_LIT 5u
/* PLASMA: ripples at once, one a voice; and how long one lasts, in frames (1.2 s). */
#define SCENE_RIPPLE_FRAMES 30u
/* ANT: steps a note-on walks; the steps after which the board clears and the ants start again. */
#define SCENE_ANT_STEPS 3u
#define SCENE_ANT_RESET 9000u
/* An envelope's steps (RULE30, ANT): the envelope adds up each frame, a step every
 * SCENE_ENV_STEP of it (a full envelope: every other frame). */
#define SCENE_ENV_STEP 512u

/* What the music does this frame, as the pieces see it. */
typedef struct {
    uint8_t gate[SCENE_VOICES];  /* the voice's note is held */
    uint8_t trig[SCENE_VOICES];  /* a note-on (or retrigger) since the last frame */
    uint8_t note[SCENE_VOICES];  /* its note, SCENE_NO_NOTE none */
    uint8_t env[SCENE_VOICES];   /* ENV3, 0..255; 0 for a voice that makes no sound */
    uint8_t color[SCENE_VOICES]; /* its waveform's colour (GRID_) */
    uint16_t cutoff;             /* 0..2047 */
    uint8_t resonance;           /* 0..15 */
} scene_music;

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} scene_canvas;

typedef struct {
    uint8_t heat[SCENE_FIRE_H][GRID_COLS]; /* 0..255, row 0 the top */
    uint8_t hot[SCENE_VOICES];             /* each flame's fuel, 0..255 */
    uint8_t flare[SCENE_VOICES];           /* a note-on waiting for the next step */
} scene_fire;

typedef struct {
    uint8_t x, y;  /* the centre, in cells */
    uint8_t age;   /* frames since the note-on; SCENE_RIPPLE_FRAMES: none */
    uint8_t amp;   /* its strength now */
} scene_ripple;

typedef struct {
    /* The four sines' phases, 65536 a turn: across, down, diagonal and round the centre, each
     * running at its own multiple of the speed. */
    uint32_t phase[4];
    uint8_t warm; /* voice 3's warmth, following its envelope a little a frame */
    scene_ripple rip[SCENE_VOICES];
} scene_plasma;

typedef struct {
    uint64_t row[SCENE_RULE30_H]; /* bit i: cell i of the generation shown on that row */
    uint8_t color[SCENE_RULE30_H];
    uint64_t cur; /* the newest generation */
    uint8_t head; /* the row the next generation goes on */
    uint16_t acc; /* the envelopes' steps (SCENE_ENV_STEP) */
} scene_rule30;

typedef struct {
    int16_t x, y; /* -768..768: 256 the unit */
    uint16_t z;   /* depth, 4096 the far end */
} scene_star;

typedef struct {
    scene_star st[SCENE_STARS_N];
    int16_t cx;                 /* the centre's offset across, pixels */
    int16_t aim;                /* where the latest note steers it */
    uint8_t lit[SCENE_VOICES];  /* frames each voice's stars stay lit */
} scene_stars;

typedef struct {
    uint8_t x, y, d; /* d: 0 up, 1 right, 2 down, 3 left */
} scene_ant_walker;

typedef struct {
    uint8_t cell[GRID_ROWS][GRID_COLS]; /* 0, or the trail's colour + 1 */
    scene_ant_walker a[SCENE_VOICES];
    uint16_t acc[SCENE_VOICES];
    uint16_t steps;
} scene_ant;

typedef struct {
    uint8_t piece;   /* SCENE_FIRE .. SCENE_ANT */
    uint32_t frame;  /* frames stepped since it started */
    uint32_t rng;    /* scene_random's state */
    uint8_t filled;  /* the canvas's rows of the start drawn (scene_fill), from the top */
    union {
        scene_fire fire;
        scene_plasma plasma;
        scene_rule30 rule30;
        scene_stars stars;
        scene_ant ant;
    } u;
    scene_canvas cv;
} scene;

/* round(4 * hypot(dx, dy)) for dx, dy = 0..29: distances in quarter cells, for the plasma
 * (generated; host/crates/sidou-tests checks it). */
extern const uint8_t scene_dist4[GRID_ROWS][GRID_COLS];
/* The pieces' names, as the screen shows them. */
extern const char* const scene_names[SCENE_PIECES];

/* Starts piece (wrapped into 0..SCENE_PIECES - 1) from its first frame, the canvas drawn
 * whole as the music m (NULL: none playing) shows it: the same piece always starts the same. */
void scene_start(scene* s, uint32_t piece, const scene_music* m);
/* scene_start with the canvas left to scene_fill: for a start that a wipe reveals
 * SCREEN_WIPE_ROWS rows a frame (app/scene_view.c). PLASMA, whose rows are costly, draws none
 * of them here; the other pieces draw the canvas whole. */
void scene_begin(scene* s, uint32_t piece);
/* The start's canvas rows above `rows` (up to GRID_ROWS) not drawn yet, as the music m
 * (NULL: none playing) shows them: scene_start is scene_begin and scene_fill of every row.
 * Rows drawn already cost nothing. */
void scene_fill(scene* s, const scene_music* m, uint32_t rows);
/* One frame of the piece with the music m: the canvas changes where the piece moved. The
 * start's rows not drawn yet (scene_begin) are drawn first, with m. */
void scene_step(scene* s, const scene_music* m);
/* The next number of the xorshift (nonzero state). */
uint32_t scene_random(uint32_t* state);
/* One generation of rule 30 on a ring of SCENE_RULE30_W cells (bit i: cell i). */
uint64_t scene_rule30_next(uint64_t cur);
/* A colour as RULE30 and the ant draw it under a low cutoff. */
uint8_t scene_dark(uint8_t color);

#ifdef __cplusplus
}
#endif

#endif
