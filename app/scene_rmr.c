// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* RED MOON RITE's logic and layers (app/scene_rmr.h). */
#include "scene_rmr.h"

#include <stddef.h>

#include "grid.h"
#include "sine.h"

_Static_assert(SCENE_RMR_REACH == SCENE_RMR_POSES, "reach follows the armed poses");

#define W SCENE_PX_W
#define H SCENE_PX_H
#define FEET ((int32_t)SCENE_RMR_FEET)

static int32_t guard_of(const scene_rmr* r) {
    return r->stance ? SCENE_RMR_GUARD2 : SCENE_RMR_GUARD;
}

/* Fighter k's place. */
static int32_t place(const scene_rmr* r, uint32_t k) {
    return k ? r->x + SCENE_RMR_APART : r->x - SCENE_RMR_APART;
}

static const scene_rmr_flight* flight(const scene_rmr* r) {
    return &scene_rmr_flights[r->unarmed > 0 ? 1 : 0];
}

void scene_rmr_init(scene_rmr* r) {
    static const scene_rmr none;
    *r = none;
    r->x = SCENE_RMR_MID;
    r->hp[0] = r->hp[1] = SCENE_RMR_HP;
    r->unarmed = -1;
    for (uint32_t i = 0; i < SCENE_RMR_SKY_ROWS; i++) {
        r->sky[i] = 1;
    }
    r->sky_row = 0xffu;
}

uint32_t scene_rmr_sky_of(uint32_t cutoff) {
    return cutoff < SCENE_RMR_SKY_DIM ? 0u : cutoff < SCENE_RMR_SKY_MID ? 1u : 2u;
}

void scene_rmr_step(scene_rmr* r, const scene_rmr_beat* e) {
    if (e->bass != SCENE_RMR_BASS_NONE) {
        r->lead = (uint8_t)(e->bass == SCENE_RMR_BASS_HIGH);
    }
    uint32_t L = r->lead, O = 1u - L;
    int32_t U = r->unarmed;
    uint32_t rpb = e->rpb ? e->rpb : 1u;
    if (e->row) {
        /* disarm: once his sword has stuck, he pulls it free on the next beat */
        if (r->sword && r->sword_n >= flight(r)->frames && r->pose[U] != SCENE_RMR_REACH &&
            r->t[U] == 0 && e->s % rpb == 0) {
            r->pose[U] = SCENE_RMR_REACH;
            r->t[U] = 8;
        }
        if (e->bar == 3u && e->s == 3u * rpb && U < 0) {
            r->spin = 4;
            r->spin_who = (uint8_t)L;
            r->pose[L] = SCENE_RMR_SPIN_A;
            r->t[L] = 99;
        }
    }
    if (r->spin) {
        if (!e->row) {
            return; /* the spin counts rows; hits between them do nothing */
        }
        uint32_t S = r->spin_who;
        r->pose[S] = r->spin & 1u ? SCENE_RMR_SPIN_B : SCENE_RMR_SPIN_A;
        if (--r->spin == 0) {
            uint32_t V = 1u - S;
            r->pose[S] = SCENE_RMR_SPIN_B;
            r->t[S] = 6;
            r->pose[V] = SCENE_RMR_HIT;
            r->t[V] = 10;
            r->hp[V] = (uint8_t)(r->hp[V] > 2u ? r->hp[V] - 2u : 0u);
            r->shake = 3;
            /* disarm: his sword flies from his hand; the fighters hold their ground meanwhile */
            if (U < 0) {
                r->unarmed = (int8_t)V;
                r->sword = 1;
                r->sword_n = 0;
                r->sword_x = (int16_t)place(r, V);
            }
        }
        return;
    }
    /* disarm: while one fighter is unarmed, nobody attacks or parries */
    if ((e->hits & SCENE_RMR_KICK) && U < 0) {
        r->pose[L] = SCENE_RMR_RAISE;
        r->t[L] = 3;
        r->next = SCENE_RMR_CHOP + 1u;
    }
    if ((e->hits & SCENE_RMR_SNARE) && U < 0) {
        r->pose[O] = SCENE_RMR_BLOCK;
        r->t[O] = 5;
        r->spark = 3;
        r->hp[L] = (uint8_t)(r->hp[L] ? r->hp[L] - 1u : 0u);
    }
    if (e->hits & SCENE_RMR_HAT) {
        r->stance ^= 1u;
    }
    if (e->row && e->s == 0) {
        for (uint32_t k = 0; k < 2u; k++) {
            if (r->hp[k] == 0) {
                r->hp[k] = SCENE_RMR_HP;
            }
        }
    }
    if (e->hits & SCENE_RMR_ARP) {
        r->flare = 3;
    }
}

void scene_rmr_frame(scene_rmr* r, uint32_t cutoff, int sounding) {
    /* The sky: the first row not in the cutoff's palette takes it. */
    uint8_t want = (uint8_t)scene_rmr_sky_of(cutoff);
    r->sky_row = 0xffu;
    for (uint32_t i = 0; i < SCENE_RMR_SKY_ROWS; i++) {
        if (r->sky[i] != want) {
            r->sky[i] = want;
            r->sky_row = (uint8_t)(SCENE_RMR_SKY_ROW0 + i);
            break;
        }
    }
    r->frame++;
    if (sounding) {
        r->quiet = 0;
    } else if (r->quiet < SCENE_RMR_CALM_FRAMES) {
        r->quiet++;
    }
    if (r->quiet < SCENE_RMR_CALM_FRAMES) {
        r->anim++;
    }
    /* The torches: flared while the flare lasts, which ends two frames a step. */
    r->flame_big = (uint8_t)(r->flare > 0);
    if (r->flare && (r->frame & 1u)) {
        r->flare--;
    }
    /* The fighters' poses. */
    for (uint32_t k = 0; k < 2u; k++) {
        if (r->t[k] && --r->t[k] == 0) {
            if (r->pose[k] == SCENE_RMR_RAISE && r->next) {
                r->pose[k] = (uint8_t)(r->next - 1u);
                r->t[k] = 5;
                r->next = 0;
            } else {
                if (r->pose[k] == SCENE_RMR_REACH) { /* disarm: back in his hand */
                    r->unarmed = -1;
                    r->sword = 0;
                }
                r->pose[k] = (uint8_t)guard_of(r);
            }
        }
    }
    for (uint32_t k = 0; k < 2u; k++) {
        if (r->t[k] == 0) {
            r->pose[k] = (uint8_t)guard_of(r);
        }
    }
    /* The drift toward the one giving ground, held while a sword is out of its hand. */
    if (!r->sword) {
        int32_t tx = SCENE_RMR_MID + (r->lead ? -SCENE_RMR_DRIFT : SCENE_RMR_DRIFT);
        r->x = (int16_t)(r->x + (tx > r->x) - (tx < r->x));
    }
    r->shake_dx = (int8_t)(r->shake ? (r->shake & 1u ? 2 : -2) : 0);
    if (r->shake) {
        r->shake--;
    }
    if (r->sword && r->sword_n < flight(r)->frames) {
        r->sword_n++;
    }
    r->spark_shown = r->spark;
    if (r->spark) {
        r->spark--;
    }
    uint32_t S = r->spin_who;
    r->arc = (uint8_t)(r->spin > 0 || (r->pose[S] == SCENE_RMR_SPIN_B && r->t[S] > 3u));
}

int scene_rmr_sword_at(const scene_rmr* r, int32_t* gx, int32_t* gy, uint32_t* k) {
    if (!r->sword) {
        return 0;
    }
    const scene_rmr_flight* f = flight(r);
    if (r->sword_n >= f->frames) {
        *gx = r->sword_x + f->stuck.dx;
        *gy = f->stuck.y;
        *k = SCENE_RMR_SWORD_FRAMES;
        return 1;
    }
    /* frames 1 .. frames - 1: frame 0 is the step that sends it, before any is drawn */
    const scene_rmr_grip* g = &f->path[r->sword_n ? r->sword_n - 1u : 0u];
    *gx = r->sword_x + g->dx;
    *gy = g->y;
    *k = g->k;
    return 1;
}

/* ---- the layers ---- */

static const scene_px_layer NONE;

/* The layers' makers, out of line: the draw makes some twenty a frame (flash, not time). */
__attribute__((noinline, unused)) static scene_px_layer screen_layer(uint8_t kind) {
    scene_px_layer l = NONE;
    l.kind = kind;
    l.x1 = W;
    l.y1 = H;
    return l;
}

__attribute__((noinline, unused)) static scene_px_layer image(const scene_image* s, int32_t x,
                                                             int32_t y, int mirror) {
    scene_px_layer l = screen_layer(SCENE_PX_IMAGE);
    l.src = s;
    l.x = (int16_t)x;
    l.y = (int16_t)y;
    l.mirror = (uint8_t)(mirror != 0);
    return l;
}

__attribute__((noinline, unused)) static scene_px_layer call(scene_px_fn fn, const void* ctx,
                                                            int32_t x0, int32_t y0, int32_t x1,
                                                            int32_t y1, uint32_t param) {
    scene_px_layer l = screen_layer(SCENE_PX_CALL);
    l.fn = fn;
    l.src = ctx;
    l.x0 = (int16_t)x0, l.y0 = (int16_t)y0, l.x1 = (int16_t)x1, l.y1 = (int16_t)y1;
    l.param = param;
    return l;
}

/* The sky: each cell row in its palette, the gradient and the moon, wisps and hills over it;
 * rows from 17 on take row 16's. */
static void sky_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const scene_rmr* r = ctx;
    uint32_t row = y / GRID_CELL_PX;
    uint32_t i = (row < 16u ? row : 16u) - SCENE_RMR_SKY_ROW0;
    uint32_t p = r->sky[i];
    scene_gradient_row(&scene_rmr_sky_gradient[p], x, y, n, out);
    scene_image_blend(&scene_rmr_sky[p], x, y - SCENE_RMR_SKY_Y, n, 1, n, 0, NULL, out);
}

/* The torches' flames: six colours cycling up through the flame, which wobbles; flared, taller
 * and with sparks over it. The prototype's cycle stage, in fixed point (Q12) with the screen's
 * sine (app/sine.h): sin(y * 0.9 + frame * 0.7) as 256ths of a turn. */
static const uint8_t FLAME[6] = {GRID_YELLOW, GRID_WHITE, GRID_ORANGE,
                                 GRID_YELLOW, GRID_RED,   GRID_ORANGE};

static void flame_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const scene_rmr* r = ctx;
    /* ctx is the scene_rmr; which torch: the half of the screen x is in */
    const uint8_t* m = scene_rmr_flames[x >= W / 2u];
    int32_t tx = m[0] + (int32_t)m[2] / 2, ty = m[1] + (int32_t)m[3] + 2;
    int32_t f = (int32_t)(r->anim & 0xffffu);
    int32_t iy = (int32_t)y;
    if (iy >= ty - 14 && iy < ty - 2) {
        int32_t d = r->flame_big ? 12 : 9;
        int32_t dy = (ty - 2 - iy) * 4096 / d;
        if (dy <= 4096) {
            uint32_t phase = (uint32_t)(iy * 9387 + f * 7301) >> 8;
            int32_t wob = sine_scaled(phase, 614);
            int32_t edge = 4096 - dy * 9 / 10;
            uint8_t c = FLAME[((iy - f) % 6 + 6) % 6];
            if (c == GRID_WHITE && dy > 2048) {
                c = GRID_YELLOW;
            }
            for (uint32_t i = 0; i < n; i++) {
                int32_t ix = (int32_t)(x + i);
                if (ix < tx - 5 || ix >= tx + 5) {
                    continue;
                }
                int32_t dx = (2 * (ix - tx) + 1) * 4096 / 10 + wob;
                if ((dx < 0 ? -dx : dx) <= edge) {
                    out[i] = c;
                }
            }
        }
    }
    if (r->flame_big && iy >= ty - 24 && iy < ty - 13) {
        for (uint32_t i = 0; i < n; i++) {
            int32_t ix = (int32_t)(x + i);
            if (ix >= tx - 3 && ix < tx + 3 && ((ix + iy + f) & 3) == 0) {
                out[i] = (iy & 1) ? GRID_YELLOW : GRID_ORANGE;
            }
        }
    }
}

/* A torch's box: its flame, and the sparks above it when flared. */
static scene_px_layer flame(const scene_rmr* r, uint32_t side) {
    const uint8_t* m = scene_rmr_flames[side];
    int32_t ty = m[1] + (int32_t)m[3] + 2;
    int32_t y0 = r->flame_big ? ty - 24 : m[1];
    return call(flame_row, r, m[0], y0, m[0] + m[2], m[1] + m[3],
                (r->anim & 0x7fffffffu) << 1 | r->flame_big);
}

/* The parry's spark at its stage, around (x, FEET - SCENE_RMR_SPARK_UP). */
static void spark_row(const void* ctx, uint32_t x, uint32_t y, uint32_t n, uint8_t* out) {
    const scene_rmr* r = ctx;
    const scene_rmr_spark* s = &scene_rmr_sparks[r->spark_shown - 1u];
    int32_t row = (int32_t)y - (FEET - SCENE_RMR_SPARK_UP) - s->dy0;
    if (row < 0 || row >= s->rows) {
        return;
    }
    for (uint32_t k = s->at[row]; k < s->at[row + 1]; k++) {
        int32_t px = r->x + s->dx[k] - (int32_t)x;
        if (px >= 0 && px < (int32_t)n) {
            out[px] = s->colour[k];
        }
    }
}

static scene_px_layer spark(const scene_rmr* r) {
    if (!r->spark_shown) {
        return NONE;
    }
    const scene_rmr_spark* s = &scene_rmr_sparks[r->spark_shown - 1u];
    int32_t reach = 4 * r->spark_shown + 4; /* the rays' length */
    int32_t cy = FEET - SCENE_RMR_SPARK_UP;
    return call(spark_row, r, r->x - reach, cy + s->dy0, r->x + reach + 1,
                cy + s->dy0 + s->rows, r->spark_shown);
}

/* The spin's arc: 14 pixels on an ellipse round the spinning fighter, turning. */
static scene_px_layer arc(scene_rmr* r) {
    if (!r->arc) {
        return NONE;
    }
    uint32_t S = r->spin_who;
    int32_t cx = place(r, S) + (S ? -10 : 10), cy = FEET - SCENE_RMR_ARC_UP;
    for (uint32_t a = 0; a < SCENE_RMR_ARC_MARKS; a++) {
        /* a / 14 x 1.4 pi + anim x 0.9 rad, in 65536ths of a turn */
        uint32_t phase = (a * 3277u + r->anim * 9387u) >> 8;
        int32_t c = sine_scaled(phase + SINE_TURN / 4u, 44);
        r->arc_marks[a].x = (int16_t)(cx + (S ? -c : c));
        r->arc_marks[a].y = (int16_t)(cy + sine_scaled(phase, 12));
        r->arc_marks[a].colour = a & 1u ? GRID_LIGHT_GREY : GRID_WHITE;
    }
    scene_px_layer l = screen_layer(SCENE_PX_MARKS);
    l.src = r->arc_marks;
    l.n = SCENE_RMR_ARC_MARKS;
    l.param = r->anim;
    return l;
}

/* The sword stuck in the floor, or in flight. */
static scene_px_layer sword(const scene_rmr* r, int stuck) {
    int32_t gx, gy;
    uint32_t k;
    if (!scene_rmr_sword_at(r, &gx, &gy, &k) || (k == SCENE_RMR_SWORD_FRAMES) != stuck) {
        return NONE;
    }
    const scene_image* s = stuck ? &scene_rmr_sword_stuck : &scene_rmr_sword[k];
    return image(s, gx - s->ax, gy - s->ay, 0);
}

static const scene_image* fighter_image(const scene_rmr* r, uint32_t k) {
    uint32_t pose = r->pose[k];
    if (r->unarmed != (int8_t)k) {
        return &scene_rmr_barb[k][pose < SCENE_RMR_POSES ? pose : SCENE_RMR_GUARD];
    }
    /* unarmed: guard, guard2, hit and reach are drawn (no other pose comes while unarmed) */
    uint32_t u = pose == SCENE_RMR_GUARD2 ? SCENE_RMR_UNARMED_GUARD2
                 : pose == SCENE_RMR_HIT ? SCENE_RMR_UNARMED_HIT
                 : pose == SCENE_RMR_REACH ? SCENE_RMR_UNARMED_REACH
                                           : SCENE_RMR_UNARMED_GUARD;
    return &scene_rmr_barb_unarmed[k][u];
}

static scene_px_layer fighter(const scene_rmr* r, uint32_t k) {
    const scene_image* s = fighter_image(r, k);
    int32_t ax = k ? s->fw * s->gx - s->ax : s->ax;
    return image(s, place(r, k) - ax + r->shake_dx, FEET - s->ay, (int)k);
}

static scene_px_layer shadow(const scene_rmr* r, uint32_t k) {
    scene_px_layer l = screen_layer(SCENE_PX_FILL);
    int32_t x = place(r, k);
    l.colour = GRID_BLACK;
    l.x0 = (int16_t)(x - 18), l.x1 = (int16_t)(x + 18);
    l.y0 = (int16_t)(FEET + 2), l.y1 = (int16_t)(FEET + 4);
    return l;
}

/* A side's meter: empty, full up to the stamina left, and the tip where it ends. */
static void meter(const scene_rmr* r, scene_px* sp, uint32_t side) {
    int32_t end = SCENE_RMR_BLADE_X + SCENE_RMR_BLADE_N * (int32_t)r->hp[side];
    uint32_t slot = side ? SCENE_RMR_SLOT_EMPTY1 : SCENE_RMR_SLOT_EMPTY0;
    scene_px_layer l = image(&scene_rmr_blade_empty, 0, 0, (int)side);
    scene_px_set(sp, slot, &l);
    l = image(&scene_rmr_blade_full, 0, 0, (int)side);
    if (side) {
        l.x0 = (int16_t)(W - end);
    } else {
        l.x1 = (int16_t)end;
    }
    scene_px_set(sp, slot + 1u, &l);
    const scene_image* tip = &scene_rmr_blade_tip;
    l = r->hp[side] ? image(tip, side ? W - end - tip->fw : end, SCENE_RMR_BLADE_Y, (int)side)
                    : NONE;
    scene_px_set(sp, slot + 2u, &l);
}

void scene_rmr_draw(scene_rmr* r, scene_px* sp) {
    scene_px_layer l = image(&scene_rmr_hud, 0, 0, 0);
    scene_px_set(sp, SCENE_RMR_SLOT_HUD, &l);
    meter(r, sp, 0);
    meter(r, sp, 1);
    l = call(sky_row, r, 0, SCENE_RMR_SKY_Y, W, scene_rmr_sky_gradient[0].y1, 0);
    scene_px_set(sp, SCENE_RMR_SLOT_SKY, &l);
    if (r->sky_row != 0xffu) {
        /* a row's palette changed under the same layer; row 16's shows on row 17 too */
        int32_t y0 = (int32_t)r->sky_row * (int32_t)GRID_CELL_PX;
        int32_t rows = r->sky_row == 16u ? 2 : 1;
        scene_px_touch(sp, 0, y0, W, y0 + rows * (int32_t)GRID_CELL_PX);
        r->sky_row = 0xffu;
    }
    l = image(&scene_rmr_arena, 0, 0, 0);
    scene_px_set(sp, SCENE_RMR_SLOT_ARENA, &l);
    l = flame(r, 0);
    scene_px_set(sp, SCENE_RMR_SLOT_FLAME0, &l);
    l = flame(r, 1);
    scene_px_set(sp, SCENE_RMR_SLOT_FLAME1, &l);
    /* the sword in the floor is behind the fighters, and over the hand that pulls it free */
    int reach = r->unarmed >= 0 && r->pose[r->unarmed] == SCENE_RMR_REACH;
    l = reach ? NONE : sword(r, 1);
    scene_px_set(sp, SCENE_RMR_SLOT_STUCK_BEHIND, &l);
    for (uint32_t k = 0; k < 2u; k++) {
        l = shadow(r, k);
        scene_px_set(sp, SCENE_RMR_SLOT_SHADOW0 + 2u * k, &l);
        l = fighter(r, k);
        scene_px_set(sp, SCENE_RMR_SLOT_FIGHTER0 + 2u * k, &l);
    }
    l = spark(r);
    scene_px_set(sp, SCENE_RMR_SLOT_SPARK, &l);
    l = arc(r);
    scene_px_set(sp, SCENE_RMR_SLOT_ARC, &l);
    l = reach ? sword(r, 1) : NONE;
    scene_px_set(sp, SCENE_RMR_SLOT_STUCK_OVER, &l);
    l = sword(r, 0);
    scene_px_set(sp, SCENE_RMR_SLOT_SWORD, &l);
}
