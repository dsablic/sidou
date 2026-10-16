// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_WORKLOAD_SCENE_H
#define SIDOU_WORKLOAD_SCENE_H

/* The pixel scene's benchmark (app/scene_px.h): a frame like RED MOON RITE's, 13 slots (the
 * HUD's bar and VS, the sky's gradient and overlay, the arena, the two torches' flames, two
 * shadows and two fighters in guard, the sword in flight and a spark), and what a frame of
 * the vignette does to it. bench/fm1/bench_scene_main.c runs it in fm1-emu and
 * host/crates/emu-gate/tests/scene_budget.rs counts its instructions and checks its log
 * against the host build's.
 *
 * The steps, each measured alone (mark() before and after it):
 *   1. the scene set up (scene_px_init and the 13 slots);
 *   2. each of the 900 tiles composited alone (scene_px_tile);
 *   3. each of the 900 cells rendered alone, as a run of one (the painter: the tile, its
 *      hash and the panel's pixels);
 *   4. the first frame's send: each grid_next_run and grid_render_run of the runs, and the
 *      last grid_next_run, which finds none;
 *   5. a scan of the 900 candidates of the set-up, none of them changed;
 *   6. the fighters each step 4 px toward the other (guard2): the two slots set, the scan,
 *      and the send as in 4;
 *   7. the sky's palette changed (the gradient and the overlay): the same;
 *   8. the torches' flames tick (their params): the same;
 *   9. the refresh sweep's two tiles of a frame (scene_px_refresh), and their send;
 *  10. the sky changed back, scanned in slices of SCENE_PX_SCAN_SLICE (each slice a step),
 *      and sent.
 *
 * The log: per send, its run count and each run (row << 8 | col, len); per scan, the
 * candidates before it, the tiles it composited and the cells marked after it; for the
 * sliced scan, the candidates and the slices; at the end every tile's hash (two halves
 * each). */

#include <stdint.h>

#include "grid.h"
#include "scene_px.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The log's length at most. */
#define WORKLOAD_SCENE_LOG 4096u

typedef struct {
    grid g;
    scene_px sp;
    uint32_t phase[2];
    scene_px_mark spark[12];
    uint16_t px[GRID_RUN_PIXELS] __attribute__((aligned(4)));
} workload_scene;

/* Runs the steps, calling mark() before and after each, and writes the log; returns its
 * length. */
uint32_t workload_scene_run(workload_scene* w, void (*mark)(void), int16_t* log);

#ifdef __cplusplus
}
#endif

#endif
