// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scene_intro.h"

#include "logo.h"

#define W SCENE_PX_W

void scene_intro_init(scene_intro* in) {
    static const scene_intro none;
    *in = none;
    in->quiet = SCENE_INTRO_CALM_FRAMES;
    uint32_t seed = SCENE_INTRO_STAR_SEED;
    for (uint32_t i = 0; i < SCENE_INTRO_STARS; i++) {
        seed = seed * 1664525u + 1013904223u;
        in->star[i] = (uint16_t)((seed >> 16) % (W * SCENE_INTRO_STAR_SUBPX));
    }
    for (uint32_t i = 0; i < 8u; i++) {
        in->logo_at[i].x = in->logo_at[i].y = GRID_BITMAP_CURSOR_NONE;
    }
}

void scene_intro_beat(scene_intro* in, const scene_rmr_beat* e) {
    in->glow = e->hits & SCENE_RMR_KICK          ? SCENE_INTRO_GLOW_KICK
               : e->hits & SCENE_RMR_SNARE       ? SCENE_INTRO_GLOW_SNARE
               : e->bass != SCENE_RMR_BASS_NONE  ? SCENE_INTRO_GLOW_BASS
               : e->hits & SCENE_RMR_ARP         ? SCENE_INTRO_GLOW_ARP
               : e->hits & SCENE_RMR_HAT         ? SCENE_INTRO_GLOW_HAT
                                                 : in->glow;
}

void scene_intro_frame(scene_intro* in, uint32_t cutoff, int sounding) {
    in->frame++;
    in->cutoff = cutoff;
    if (sounding) {
        in->quiet = 0;
    } else if (in->quiet < SCENE_INTRO_CALM_FRAMES) {
        in->quiet++;
    }
    if (in->quiet >= SCENE_INTRO_CALM_FRAMES) {
        in->glow = SCENE_INTRO_GLOW_CALM;
    }
}

static const uint8_t STAR_COLOUR[SCENE_INTRO_STAR_DEPTHS] = {GRID_DARK_GREY, GRID_LIGHT_GREY,
                                                            GRID_WHITE};
static const uint8_t STAR_SPEED[SCENE_INTRO_STAR_DEPTHS] = {4, 8, 16};

static uint32_t star_depth(uint32_t i) {
    return (2u * i + i / 3u) % SCENE_INTRO_STAR_DEPTHS;
}

static int32_t star_y(uint32_t i) {
    int32_t low = i >= SCENE_INTRO_STARS_TOP;
    int32_t k = (int32_t)i - (low ? (int32_t)SCENE_INTRO_STARS_TOP : 0);
    return (low ? SCENE_INTRO_STAR_LOW_Y : SCENE_INTRO_STAR_TOP_Y) + SCENE_INTRO_STAR_PITCH * k;
}

static void star_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const scene_intro* in = ctx;
    int32_t low = (int32_t)y >= SCENE_INTRO_STAR_LOW_Y;
    int32_t d = (int32_t)y - (low ? SCENE_INTRO_STAR_LOW_Y : SCENE_INTRO_STAR_TOP_Y);
    if (d % SCENE_INTRO_STAR_PITCH) {
        return;
    }
    uint32_t i = (uint32_t)(d / SCENE_INTRO_STAR_PITCH) + (low ? SCENE_INTRO_STARS_TOP : 0u);
    uint32_t at = in->star[i] / SCENE_INTRO_STAR_SUBPX - x;
    if (at < n) {
        out[at] = STAR_COLOUR[star_depth(i)];
    }
}

static uint32_t star_gear(const scene_intro* in) {
    if (in->quiet >= SCENE_INTRO_CALM_FRAMES) {
        return SCENE_INTRO_CUTOFF_TOP / SCENE_INTRO_STARS_CALM;
    }
    uint32_t open = in->cutoff < SCENE_INTRO_CUTOFF_TOP ? in->cutoff : SCENE_INTRO_CUTOFF_TOP - 1u;
    return SCENE_INTRO_CUTOFF_TOP + (SCENE_INTRO_GEAR_TOP - 1u) * open;
}

static void stars_drift(scene_intro* in, scene_px* sp) {
    const uint32_t span = W * SCENE_INTRO_STAR_SUBPX, gear = star_gear(in);
    for (uint32_t i = 0; i < SCENE_INTRO_STARS; i++) {
        uint32_t step = STAR_SPEED[star_depth(i)] * gear / SCENE_INTRO_CUTOFF_TOP;
        uint32_t was = in->star[i];
        in->star[i] = (uint16_t)((was + span - step) % span);
        int32_t from = (int32_t)(was / SCENE_INTRO_STAR_SUBPX);
        int32_t to = (int32_t)(in->star[i] / SCENE_INTRO_STAR_SUBPX);
        if (from != to) {
            int32_t y = star_y(i);
            scene_px_touch(sp, from, y, from + 1, y + 1);
            scene_px_touch(sp, to, y, to + 1, y + 1);
        }
    }
}

static scene_px_layer stars(scene_intro* in, uint32_t first, uint32_t count) {
    return scene_px_call(star_row, in, 0, star_y(first), W, star_y(first + count - 1u) + 1, 0);
}

static const uint8_t GLOW[SCENE_INTRO_GLOWS][2] = {
    {GRID_PURPLE, GRID_BLUE},     {GRID_LIGHT_RED, GRID_RED},     {GRID_YELLOW, GRID_ORANGE},
    {GRID_CYAN, GRID_LIGHT_BLUE}, {GRID_LIGHT_GREEN, GRID_GREEN}, {GRID_ORANGE, GRID_BROWN},
};

static void logo_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    scene_intro* in = (scene_intro*)(uintptr_t)ctx;
    uint8_t px[W];
    grid_bitmap_span(&logo_metal_chrome_bitmap, &in->logo_at[y & 7u], x, y, n, px);
    const uint8_t* glow = GLOW[in->glow_row[(y - SCENE_INTRO_LOGO_Y0) / GRID_CELL_PX]];
    for (uint32_t i = 0; i < n; i++) {
        uint8_t p = px[i];
        if (p == GRID_PURPLE || p == GRID_BLUE) {
            out[i] = glow[p == GRID_BLUE];
        } else if (p != GRID_BLACK) {
            out[i] = p;
        }
    }
}

static void glow_sweep(scene_intro* in, grid* g) {
    if (in->frame % SCENE_INTRO_GLOW_EVERY) {
        return;
    }
    for (uint32_t k = 0; k < SCENE_INTRO_LOGO_ROWS; k++) {
        uint32_t row = (in->glow_next + k) % SCENE_INTRO_LOGO_ROWS;
        if (in->glow_row[row] != in->glow) {
            in->glow_row[row] = in->glow;
            in->glow_next = (uint8_t)((row + 1u) % SCENE_INTRO_LOGO_ROWS);
            for (uint32_t c = 0; c < GRID_COLS; c++) {
                grid_mark(g, c, SCENE_INTRO_LOGO_Y0 / GRID_CELL_PX + row);
            }
            return;
        }
    }
}

void scene_intro_draw(scene_intro* in, scene_px* sp, grid* g) {
    stars_drift(in, sp);
    glow_sweep(in, g);
    scene_px_layer l = stars(in, 0, SCENE_INTRO_STARS_TOP);
    scene_px_set(sp, SCENE_INTRO_SLOT_STARS_TOP, &l);
    l = stars(in, SCENE_INTRO_STARS_TOP, SCENE_INTRO_STARS - SCENE_INTRO_STARS_TOP);
    scene_px_set(sp, SCENE_INTRO_SLOT_STARS_LOW, &l);
    l = scene_px_call(logo_row, in, 0, SCENE_INTRO_LOGO_Y0, W, SCENE_INTRO_LOGO_Y1, 0);
    scene_px_set(sp, SCENE_INTRO_SLOT_LOGO, &l);
}
