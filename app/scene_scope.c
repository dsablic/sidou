// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scene_scope.h"

#include "live_view.h"

#define W SCENE_PX_W
#define LANE ((int32_t)SCENE_SCOPE_LANE_H)
#define TILE ((int32_t)GRID_CELL_PX)
#define FULL_SWING (128 * 255)

const uint8_t scene_scope_colour[SCENE_SCOPE_CHIPS][2] = {{GRID_LIGHT_GREEN, GRID_GREEN},
                                                         {GRID_YELLOW, GRID_ORANGE}};

void scene_scope_init(scene_scope* so) {
    for (uint32_t v = 0; v < SCENE_SCOPE_LANES; v++) {
        for (uint32_t x = 0; x < W; x++) {
            so->y[v][x] = SCENE_SCOPE_MID;
        }
    }
    so->chip = SCENE_SCOPE_CHIP_A;
    so->quiet = SCENE_SCOPE_CALM_FRAMES;
}

uint32_t scene_scope_trigger(const uint8_t wave[SCOPE_W]) {
    for (uint32_t k = 0; k < SCOPE_W; k++) {
        if (wave[(k + SCOPE_W - 1u) % SCOPE_W] < 128u && wave[k] >= 128u) {
            return k;
        }
    }
    return 0;
}

static int lane_trace(const scene_scope* so, const scope* tap, const live* l, uint32_t v,
                      uint8_t now[W]) {
    int32_t env = tap ? tap->env[v] : 0;
    uint8_t wave[SCOPE_W];
    if (!env) {
        for (uint32_t k = 0; k < SCOPE_W; k++) {
            wave[k] = 128;
        }
    } else if (!scope_trace_ring(tap, SCENE_SCOPE_POINTS, v, live_view_scope_by(l, v), wave)) {
        return 0;
    }
    uint32_t t = scene_scope_trigger(wave);
    uint32_t cycles = live_view_scope_cycles(l->voice[v].note);
    uint32_t moved = 0;
    uint32_t at = t, part = 0;
    const int32_t gain_q16 = env * SCENE_SCOPE_AMP * 65536 / FULL_SWING;
    for (uint32_t x = 0; x < W; x++) {
        int32_t y = SCENE_SCOPE_MID - (((int32_t)wave[at] - 128) * gain_q16 >> 16);
        part += SCOPE_W * cycles;
        for (; part >= W; part -= W) {
            at = at + 1u < SCOPE_W ? at + 1u : 0u;
        }
        int32_t d = y - (int32_t)so->y[v][x];
        moved += d > SCENE_SCOPE_STILL || d < -SCENE_SCOPE_STILL || (!env && d);
        now[x] = (uint8_t)y;
    }
    return moved > (env ? cycles * SCENE_SCOPE_EDGE_JITTER : 0u);
}

static void touch_changes(scene_px* sp, uint32_t lane, const uint8_t* was, const uint8_t* now) {
    const int32_t top = (int32_t)lane * LANE;
    for (int32_t c = 0; c < (int32_t)GRID_COLS; c++) {
        int32_t x0 = c * TILE - 2, x1 = c * TILE + TILE + 1;
        x0 = x0 < 0 ? 0 : x0;
        x1 = x1 > W ? W : x1;
        int changed = 0;
        int32_t lo = LANE, hi = 0;
        for (int32_t x = x0; x < x1; x++) {
            changed |= was[x] != now[x];
            int32_t a = was[x] < now[x] ? was[x] : now[x];
            int32_t b = was[x] < now[x] ? now[x] : was[x];
            lo = a < lo ? a : lo;
            hi = b > hi ? b : hi;
        }
        if (changed) {
            scene_px_touch(sp, c * TILE, top + lo - 1, c * TILE + TILE, top + hi + 2);
        }
    }
}

static void spans(const uint8_t* ys, int32_t from, uint32_t n, int16_t* lo, int16_t* hi) {
    for (uint32_t k = 0; k < n; k++) {
        int32_t x = from + (int32_t)k;
        if (x < 0 || x >= W) {
            lo[k] = LANE;
            hi[k] = -LANE;
            continue;
        }
        uint8_t a = ys[x], b = ys[x ? x - 1 : 0];
        lo[k] = a < b ? a : b;
        hi[k] = a < b ? b : a;
    }
}

static void lane_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const scene_scope* so = ctx;
    const uint8_t* colour = scene_scope_colour[so->chip];
    const int32_t ly = (int32_t)(y % LANE);
    int16_t lo[TILE + 2], hi[TILE + 2];
    spans(so->y[y / LANE], (int32_t)x - 1, n + 2u, lo, hi);
    int32_t top = LANE, bottom = -LANE;
    for (uint32_t k = 0; k < n + 2u; k++) {
        top = lo[k] < top ? lo[k] : top;
        bottom = hi[k] > bottom ? hi[k] : bottom;
    }
    int near = ly >= top - 1 && ly <= bottom + 1;
    for (uint32_t i = 0; i < n; i++) {
        const int16_t* l = lo + i;
        const int16_t* h = hi + i;
        if (near && l[1] <= ly && ly <= h[1]) {
            out[i] = colour[SCENE_SCOPE_CORE];
        } else if (near && ((l[1] - 1 <= ly && ly <= h[1] + 1) || (l[0] <= ly && ly <= h[0]) ||
                            (l[2] <= ly && ly <= h[2]))) {
            out[i] = colour[SCENE_SCOPE_HALO];
        } else if (ly == SCENE_SCOPE_MID && (x + i) % SCENE_SCOPE_DOTS == 0u) {
            out[i] = GRID_DARK_GREY;
        }
    }
}

void scene_scope_frame(scene_scope* so, const scope* tap, const live* l, scene_px* sp) {
    so->chip = (uint8_t)(l->two_chips && l->keys_chip ? SCENE_SCOPE_CHIP_B : SCENE_SCOPE_CHIP_A);
    int sounding = 0;
    so->lane = (uint8_t)((so->lane + SCENE_SCOPE_LANES_A_FRAME) % SCENE_SCOPE_LANES);
    for (uint32_t v = 0; v < SCENE_SCOPE_LANES; v++) {
        uint8_t now[W];
        sounding |= tap && tap->env[v];
        uint32_t turn = (v + SCENE_SCOPE_LANES - so->lane) % SCENE_SCOPE_LANES;
        if (turn < SCENE_SCOPE_LANES_A_FRAME && lane_trace(so, tap, l, v, now)) {
            touch_changes(sp, v, so->y[v], now);
            for (uint32_t x = 0; x < W; x++) {
                so->y[v][x] = now[x];
            }
        }
    }
    if (sounding) {
        so->quiet = 0;
    } else if (so->quiet < SCENE_SCOPE_CALM_FRAMES) {
        so->quiet++;
    }
    for (uint32_t v = 0; v < SCENE_SCOPE_LANES; v++) {
        scene_px_layer lane = scene_px_call(lane_row, so, 0, (int32_t)v * LANE, W,
                                            (int32_t)(v + 1u) * LANE, so->chip);
        scene_px_set(sp, v, &lane);
    }
}
