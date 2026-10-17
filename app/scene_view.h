// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SCENE, from the menu: the generative pieces of app/scene.h on the whole screen, reacting to
 * what the instrument plays (docs/art/README.md "SCENE", docs/reference/controls.md).
 *
 * SELECT steps between the pieces (FIRE, PLASMA, RULE 30, STARS, ANT, and round): the new one
 * wipes in from the top, and a toast names it. HOME goes back. Everything else plays the
 * instrument, so the knobs move the pieces as well: K1 the cutoff, K2 the resonance.
 *
 * What the pieces hear (scene_music), each frame: the gates, notes and note-ons of the voices
 * from the instrument (a voice's stamp changes at each note-on and note-off), their waveforms'
 * colours (LIVE's), the cutoff and resonance, and each voice's envelope (ENV3) from the audio's
 * tap (app/scope.h), which runs while SCENE shows as it does for LIVE. A voice that makes no
 * sound (no waveform: SYNC LEAD's voices 2 and 3, which only clock the sync) counts as
 * silent.
 *
 * The budget: a frame sends at most SCENE_VIEW_FRAME_CELLS cells. The piece draws into its
 * canvas; the compose copies the canvas's changed cells onto the grid until the frame has spent
 * what the screen's toast, raster and cover leave (screen_layer_cells), and goes on from that
 * cell the next frame, so a frame that changes more than the budget spreads over the next ones.
 * The copy goes round the screen that way, so a changed cell waits at most about 900 / left
 * frames for its turn (left: the cells the budget leaves the copy that frame; 5 frames at the
 * whole 200). What the screen drew over (a toast gone, the raster, another view) differs from
 * the canvas, so it is put back the same way, within the budget; what is drawn over the canvas
 * after the compose this frame (a toast that shows, MIDI learn's frame: screen_covered) is
 * left alone and costs the copy nothing. Only under a wipe are the rows copied whole (the
 * wipe holds the ones it has not reached). While a wipe or a raster runs the piece holds
 * still, as LIVE and About do.
 *
 * The piece steps half way between two frames (scene_view_prepare, from views_prepare), so a
 * frame's compose is the copy alone; a compose with nothing prepared steps itself. A piece
 * picked (SELECT, or the menu's ALGORITHM on SCENE) starts in the compose of the wipe's first
 * frame, not in the input: the main loop runs the input and a frame in the same pass between
 * two drains of MIDI, and a start and that frame together would hold the drain too long.
 * PLASMA's start draws only the rows the wipe reveals each frame (scene_begin, scene_fill).
 *
 * RED MOON RITE (app/scene_rmr.h), the sixth piece, is drawn in pixels: while it shows, the
 * grid's whole screen is the tile compositor's painter region (app/scene_px.h) and every
 * cell is blank, so the painter's pixels show. What it hears comes from its music hooks
 * (app/scene_song.h): the song's events, which the main loop hands to the view every pass
 * (scene_view_song), or the instrument. A frame of it is spread over the main loop's passes:
 * - the compose: the vignette's step (the frame's music, its state, its layers set again with
 *   scene_rmr_draw, which makes candidates of the tiles under what moved), a top layer over
 *   what the screen draws over the scene this frame (MIDI learn's frame, the toast, the
 *   raster), where each cell that is not blank shows its own background, the cells the screen
 *   drew over put back blank within the frame's budget, the refresh sweep's tile while the
 *   music moves it, and the cells the budget leaves for the scan;
 * - the scan, a slice of SCENE_PX_SCAN_SLICE candidates in each ui_poll that has nothing to
 *   send (scene_view_idle): each composited, and marked to be sent only if it changed. The
 *   scan stops marking once the frame has spent its SCENE_VIEW_FRAME_CELLS, and goes on from
 *   that tile at the next frame, so nothing starves. The step is in the compose, not half way
 *   between frames, so a frame's cells (the compose's and the scan's) go out together after it,
 *   as the other pieces' do, and no 40 ms holds more than the budget.
 * A compose that finds the painter region gone (a view switch took it; MIDI learn's frame
 * leaves it, SCREEN_OVER) starts the compositor again, so every cell is sent once more
 * (scene_px.h's invariant). The vignette holds still under a wipe and a raster, as the other
 * pieces do, and a silent one sends nothing once calm.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SCENE_VIEW_H
#define SIDOU_SCENE_VIEW_H

#include <stdint.h>

#include "live.h"
#include "scene.h"
#include "scene_px.h"
#include "scene_rmr.h"
#include "scene_song.h"
#include "scope.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cells a frame of SCENE sends at most: 200 cells and their windows are about 17.5 ms of
 * SPI (one window a cell, 18.5 ms), inside the 20 ms budget (docs/art/README.md). */
#define SCENE_VIEW_FRAME_CELLS 200u

/* RED MOON RITE while it runs: the compositor, which owns the grid's painter region, and the
 * vignette. */
typedef struct {
    scene_px px;
    scene_rmr rmr;
    grid* g;        /* the grid it paints */
    uint16_t left;  /* cells the scan may still mark this frame */
} scene_view_rite;

typedef struct {
    const live* l;
    scope* tap;       /* NULL: no tap, every envelope 0 */
    uint8_t on;       /* the tap is on for SCENE */
    uint8_t which;    /* the piece it shows (its index), kept between visits */
    uint8_t prepared; /* the piece stepped for the next frame (scene_view_prepare) */
    uint8_t starting; /* the piece picked starts at the next compose */
    uint32_t stamp[SCENE_VOICES]; /* each voice's stamp at the last frame */
    uint16_t cursor;  /* the cell the copy goes on from */
    uint32_t held;    /* frames whose copy the budget cut short (diagnostic) */
    uint8_t idle;     /* RED MOON RITE's compose left the scan work (scene_view_idle) */
    scene_music music; /* what the last step heard */
    scene_song song;   /* RED MOON RITE's music hooks, fed every pass */
    union {
        scene piece;          /* a PETSCII piece running (which < SCENE_PETSCII) */
        scene_view_rite rite; /* RED MOON RITE running */
    } u;
} scene_view;

extern const view scene_view_view;

/* Shows piece 0 (FIRE) first, on l and the tap sc (may be NULL). */
void scene_view_init(scene_view* sv, const live* l, scope* sc);
/* The tap is SCENE's while on (it shows: on top, or under MIDI learn's frame); every frame,
 * before the screen's. The tap itself is switched by views_frame. */
void scene_view_tap(scene_view* sv, int on);
/* Steps the piece now, half way between two frames: only while it shows and holds no wipe
 * (RED MOON RITE steps in its compose). */
void scene_view_prepare(scene_view* sv);
/* RED MOON RITE's scan, a slice, in a ui_poll with nothing to send: 1 while it has candidates
 * left that the frame's budget lets it mark. */
int scene_view_idle(scene_view* sv);
/* Once a main-loop pass: the song control (NULL: none) and the events drained from its ring
 * this pass, for the music hooks. */
void scene_view_song(scene_view* sv, const song_ctl* sc, const song_event* ev, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
