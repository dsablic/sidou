// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The pixel scene's benchmark (bench/workload_scene.h). */
#include "workload_scene.h"

#include <stddef.h>

#include "scene_art.h"

static const uint8_t VS[2] = {'V', 'S'};
static const uint8_t CYCLE[4] = {GRID_RED, GRID_ORANGE, GRID_YELLOW, GRID_WHITE};

/* The torches' colour cycling: every other pixel of the flame's box, by the phase at ctx. */
static void flame(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    uint32_t p = *(const uint32_t*)ctx;
    for (uint32_t k = 0; k < n; k++) {
        if (((x + k) ^ y) & 1u) {
            out[k] = CYCLE[(x + k + y + p) & 3u];
        }
    }
}

static const scene_px_layer EMPTY;

static scene_px_layer screen_layer(uint8_t kind) {
    scene_px_layer l = EMPTY;
    l.kind = kind;
    l.x1 = SCENE_PX_W;
    l.y1 = SCENE_PX_H;
    return l;
}

static scene_px_layer image(const scene_image* s, int16_t x, int16_t y, uint8_t mirror) {
    scene_px_layer l = screen_layer(SCENE_PX_IMAGE);
    l.src = s;
    l.x = x;
    l.y = y;
    l.mirror = mirror;
    return l;
}

static scene_px_layer fill(int16_t x0, int16_t y0, int16_t x1, int16_t y1, uint8_t colour) {
    scene_px_layer l = screen_layer(SCENE_PX_FILL);
    l.x0 = x0, l.y0 = y0, l.x1 = x1, l.y1 = y1;
    l.colour = colour;
    return l;
}

static scene_px_layer sky(uint32_t k, int gradient) {
    scene_px_layer l;
    if (gradient) {
        l = screen_layer(SCENE_PX_GRADIENT);
        l.src = &scene_rmr_sky_gradient[k];
    } else {
        l = image(&scene_rmr_sky[k], 0, SCENE_RMR_SKY_Y, 0);
    }
    l.y0 = 16;
    l.y1 = 140;
    return l;
}

/* The slots. */
enum { HUD, TEXT, GRADIENT, SKY, ARENA, FLAME0, FLAME1, SHADOW0, FIGHTER0, SHADOW1, FIGHTER1,
       SWORD, SPARK };

static void set_up(workload_scene* w) {
    scene_px* sp = &w->sp;
    grid_init(&w->g, ' ', 0);
    scene_px_init(sp, &w->g);
    scene_px_layer l = fill(0, 0, SCENE_PX_W, 16, GRID_BLACK);
    scene_px_set(sp, HUD, &l);
    l = screen_layer(SCENE_PX_TEXT);
    l.x = 112, l.y = 4, l.n = 2, l.colour = GRID_WHITE, l.src = VS;
    scene_px_set(sp, TEXT, &l);
    l = sky(1, 1);
    scene_px_set(sp, GRADIENT, &l);
    l = sky(1, 0);
    scene_px_set(sp, SKY, &l);
    l = image(&scene_rmr_arena, 0, 0, 0);
    scene_px_set(sp, ARENA, &l);
    for (uint32_t f = 0; f < 2u; f++) {
        const uint8_t* b = scene_rmr_flames[f];
        l = screen_layer(SCENE_PX_CALL);
        l.x0 = b[0], l.y0 = b[1];
        l.x1 = (int16_t)(b[0] + b[2]), l.y1 = (int16_t)(b[1] + b[3]);
        l.fn = flame;
        l.src = &w->phase[f];
        scene_px_set(sp, FLAME0 + f, &l);
    }
    l = fill(73, 230, 109, 232, GRID_BLACK);
    scene_px_set(sp, SHADOW0, &l);
    l = image(&scene_rmr_barb[0][SCENE_RMR_GUARD], 33, 116, 0);
    scene_px_set(sp, FIGHTER0, &l);
    l = fill(133, 230, 169, 232, GRID_BLACK);
    scene_px_set(sp, SHADOW1, &l);
    l = image(&scene_rmr_barb[1][SCENE_RMR_GUARD], 81, 116, 1);
    scene_px_set(sp, FIGHTER1, &l);
    l = image(&scene_rmr_sword[3], 100, 40, 0);
    scene_px_set(sp, SWORD, &l);
    for (uint32_t i = 0; i < 12u; i++) {
        w->spark[i].x = (int16_t)(116 + (int)(i % 4u) * 2);
        w->spark[i].y = (int16_t)(150 + (int)(i / 4u) * 2);
        w->spark[i].colour = GRID_WHITE;
    }
    l = screen_layer(SCENE_PX_MARKS);
    l.n = 12, l.src = w->spark;
    scene_px_set(sp, SPARK, &l);
}

static uint32_t log_at;

static void put(int16_t* log, uint32_t v) {
    if (log_at < WORKLOAD_SCENE_LOG) {
        log[log_at++] = (int16_t)v;
    }
}

/* Sends every marked cell, a run a step, as ui_poll renders them; logs the runs. */
static void send(workload_scene* w, void (*mark)(void), int16_t* log) {
    uint32_t count_at = log_at;
    put(log, 0);
    uint32_t runs = 0;
    for (;;) {
        grid_run run;
        mark();
        int got = grid_next_run(&w->g, &run);
        if (got) {
            grid_render_run(&w->g, &run, grid_palette, w->px);
        }
        mark();
        if (!got) {
            break;
        }
        runs++;
        put(log, (uint32_t)run.row << 8 | run.col);
        put(log, run.len);
    }
    if (count_at < WORKLOAD_SCENE_LOG) {
        log[count_at] = (int16_t)runs;
    }
}

static void scan(workload_scene* w, void (*mark)(void), int16_t* log) {
    uint32_t scanned = w->sp.scanned;
    put(log, w->sp.ncand);
    mark();
    scene_px_scan(&w->sp, &w->g, GRID_CELLS);
    mark();
    put(log, w->sp.scanned - scanned);
    put(log, grid_pending(&w->g));
}

uint32_t workload_scene_run(workload_scene* w, void (*mark)(void), int16_t* log) {
    log_at = 0;
    scene_px* sp = &w->sp;
    w->phase[0] = w->phase[1] = 0;
    mark();
    set_up(w);
    mark();
    uint8_t tile[64];
    for (uint32_t t = 0; t < GRID_CELLS; t++) {
        mark();
        scene_px_tile(sp, t % GRID_COLS, t / GRID_COLS, tile);
        mark();
    }
    for (uint32_t t = 0; t < GRID_CELLS; t++) {
        grid_run one = {(uint8_t)(t / GRID_COLS), (uint8_t)(t % GRID_COLS), 1};
        mark();
        grid_render_run(&w->g, &one, grid_palette, w->px);
        mark();
    }
    send(w, mark, log);
    scan(w, mark, log);

    scene_px_layer l;
    mark();
    l = image(&scene_rmr_barb[0][SCENE_RMR_GUARD2], 37, 116, 0);
    scene_px_set(sp, FIGHTER0, &l);
    l = image(&scene_rmr_barb[1][SCENE_RMR_GUARD2], 77, 116, 1);
    scene_px_set(sp, FIGHTER1, &l);
    mark();
    scan(w, mark, log);
    send(w, mark, log);

    mark();
    l = sky(2, 1);
    scene_px_set(sp, GRADIENT, &l);
    l = sky(2, 0);
    scene_px_set(sp, SKY, &l);
    mark();
    scan(w, mark, log);
    send(w, mark, log);

    mark();
    for (uint32_t f = 0; f < 2u; f++) {
        w->phase[f]++;
        l = sp->layer[FLAME0 + f];
        l.param++;
        scene_px_set(sp, FLAME0 + f, &l);
    }
    mark();
    scan(w, mark, log);
    send(w, mark, log);

    mark();
    scene_px_refresh(sp, &w->g, 2);
    mark();
    send(w, mark, log);

    /* The sky back, scanned in slices as a view does between its frames. */
    l = sky(1, 1);
    scene_px_set(sp, GRADIENT, &l);
    l = sky(1, 0);
    scene_px_set(sp, SKY, &l);
    put(log, sp->ncand);
    uint32_t slices = 0;
    for (uint32_t left = 1; left;) {
        mark();
        left = scene_px_scan(sp, &w->g, SCENE_PX_SCAN_SLICE);
        mark();
        slices++;
    }
    put(log, slices);
    send(w, mark, log);

    for (uint32_t t = 0; t < GRID_CELLS; t++) {
        put(log, sp->hash[t] & 0xFFFFu);
        put(log, sp->hash[t] >> 16);
    }
    return log_at;
}
