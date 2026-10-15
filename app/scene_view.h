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
 * sound (no waveform: SYNC LEAD's voice 3, which only clocks the sync) counts as silent.
 *
 * The budget: a frame sends at most SCENE_VIEW_FRAME_CELLS cells. The piece draws into its
 * canvas; the compose copies the canvas's changed cells onto the grid until the frame has spent
 * what the screen's toast and raster leave (screen_layer_cells), and goes on from that cell the
 * next frame, so a frame that changes more than the budget spreads over the next ones. What
 * the screen drew over (a toast, the raster, another view) differs from the canvas, so it is
 * put back the same way, within the budget; only under a wipe are the rows copied whole (the
 * wipe holds the ones it has not reached). While a wipe or a raster runs the piece holds
 * still, as LIVE and About do.
 *
 * The piece steps half way between two frames (scene_view_prepare, from views_prepare), so a
 * frame's compose is the copy alone; a compose with nothing prepared steps itself.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SCENE_VIEW_H
#define SIDOU_SCENE_VIEW_H

#include <stdint.h>

#include "live.h"
#include "scene.h"
#include "scope.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cells a frame of SCENE sends at most: 200 cells and their windows are about 17.5 ms of
 * SPI (one window a cell, 18.5 ms), inside the 20 ms budget (docs/art/README.md). */
#define SCENE_VIEW_FRAME_CELLS 200u

typedef struct {
    const live* l;
    scope* tap;       /* NULL: no tap, every envelope 0 */
    uint8_t on;       /* the tap is on for SCENE */
    uint8_t which;    /* the piece it shows (its index), kept between visits */
    uint8_t prepared; /* the piece stepped for the next frame (scene_view_prepare) */
    uint32_t stamp[SCENE_VOICES]; /* each voice's stamp at the last frame */
    uint16_t cursor;  /* the cell the copy goes on from */
    uint32_t held;    /* frames whose copy the budget cut short (diagnostic) */
    scene_music music; /* what the last step heard */
    scene piece;       /* the piece running */
} scene_view;

extern const view scene_view_view;

/* Shows piece 0 (FIRE) first, on l and the tap sc (may be NULL). */
void scene_view_init(scene_view* sv, const live* l, scope* sc);
/* The tap is SCENE's while on (it shows: on top, or under MIDI learn's frame); every frame,
 * before the screen's. The tap itself is switched by views_frame. */
void scene_view_tap(scene_view* sv, int on);
/* Steps the piece now, half way between two frames: only while it shows and holds no wipe. */
void scene_view_prepare(scene_view* sv);

#ifdef __cplusplus
}
#endif

#endif
