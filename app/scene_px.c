// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The tile compositor for pixel scenes (app/scene_px.h). */
#include "scene_px.h"

#include <stddef.h>

#include "font.h"

#define TILE 8

/* An empty slot (SCENE_PX_NONE is 0). */
static const scene_px_layer NONE;

static int32_t lo(int32_t a, int32_t b) {
    return a < b ? a : b;
}

static int32_t hi(int32_t a, int32_t b) {
    return a > b ? a : b;
}

/* The frame rectangle an image's pixels can cover, in stored pixels: its coded rectangle and
 * its base's. */
static void image_rect(const scene_image* s, int32_t* x0, int32_t* y0, int32_t* x1,
                       int32_t* y1) {
    *x0 = s->x;
    *y0 = s->y;
    *x1 = (int32_t)s->x + s->w;
    *y1 = (int32_t)s->y + s->h;
    if (s->base) {
        int32_t a, b, c, d;
        image_rect(s->base, &a, &b, &c, &d);
        *x0 = lo(*x0, a);
        *y0 = lo(*y0, b);
        *x1 = hi(*x1, c);
        *y1 = hi(*y1, d);
    }
}

/* A layer's box on the screen, clipped to it. */
static void layer_box(const scene_px_layer* l, int16_t box[4]) {
    int32_t x0 = 0, y0 = 0, x1 = 0, y1 = 0;
    switch (l->kind) {
    case SCENE_PX_FILL:
    case SCENE_PX_CALL:
        x0 = l->x0, y0 = l->y0, x1 = l->x1, y1 = l->y1;
        break;
    case SCENE_PX_GRADIENT: {
        const scene_gradient* g = l->src;
        x0 = l->x0, x1 = l->x1;
        y0 = hi(l->y0, g->y0), y1 = lo(l->y1, g->y1);
        break;
    }
    case SCENE_PX_IMAGE: {
        const scene_image* s = l->src;
        int32_t a, b, c, d;
        image_rect(s, &a, &b, &c, &d);
        if (l->mirror) {
            int32_t t = s->fw - c;
            c = s->fw - a;
            a = t;
        }
        x0 = hi(l->x + a * s->gx, l->x0);
        x1 = lo(l->x + c * s->gx, l->x1);
        y0 = hi(l->y + b, l->y0);
        y1 = lo(l->y + d, l->y1);
        break;
    }
    case SCENE_PX_MARKS: {
        const scene_px_mark* m = l->src;
        x0 = y0 = INT16_MAX;
        x1 = y1 = INT16_MIN;
        for (uint32_t i = 0; i < l->n; i++) {
            x0 = lo(x0, m[i].x);
            y0 = lo(y0, m[i].y);
            x1 = hi(x1, m[i].x + 1);
            y1 = hi(y1, m[i].y + 1);
        }
        break;
    }
    case SCENE_PX_TEXT:
        x0 = l->x, y0 = l->y;
        x1 = l->x + TILE * l->n, y1 = l->y + TILE;
        break;
    default:
        break;
    }
    x0 = hi(x0, 0), y0 = hi(y0, 0);
    x1 = lo(x1, SCENE_PX_W), y1 = lo(y1, SCENE_PX_H);
    if (x1 <= x0 || y1 <= y0) {
        x0 = y0 = x1 = y1 = 0;
    }
    box[0] = (int16_t)x0, box[1] = (int16_t)y0, box[2] = (int16_t)x1, box[3] = (int16_t)y1;
}

static int same_layer(const scene_px_layer* a, const scene_px_layer* b) {
    return a->kind == b->kind && a->colour == b->colour && a->mirror == b->mirror &&
           a->n == b->n && a->x == b->x && a->y == b->y && a->x0 == b->x0 && a->y0 == b->y0 &&
           a->x1 == b->x1 && a->y1 == b->y1 && a->src == b->src && a->remap == b->remap &&
           a->fn == b->fn && a->param == b->param;
}

static void touch_box(scene_px* sp, const int16_t box[4]) {
    scene_px_touch(sp, box[0], box[1], box[2], box[3]);
}

/* The grid's painter: the tile, and its hash as the panel will show it. */
static void paint(void* ctx, uint32_t col, uint32_t row, uint8_t px[64]) {
    scene_px* sp = ctx;
    scene_px_tile(sp, col, row, px);
    sp->hash[row * GRID_COLS + col] = scene_px_hash(px);
    sp->painted++;
}

/* The same pixels for another reader (the Live Link mirror, grid.h region_peek): no hash. */
static void peek(void* ctx, uint32_t col, uint32_t row, uint8_t px[64]) {
    scene_px_tile(ctx, col, row, px);
}

void scene_px_init(scene_px* sp, grid* g) {
    for (uint32_t i = 0; i < SCENE_PX_LAYERS; i++) {
        sp->layer[i] = NONE;
        for (uint32_t k = 0; k < 4u; k++) {
            sp->box[i][k] = 0;
        }
    }
    for (uint32_t t = 0; t < GRID_CELLS; t++) {
        sp->hash[t] = 0;
    }
    for (uint32_t k = 0; k < sizeof sp->cand; k++) {
        sp->cand[k] = 0;
    }
    sp->nlive = 0;
    sp->ncand = 0;
    sp->next = 0;
    sp->sweep = 0;
    sp->scanned = 0;
    sp->painted = 0;
    sp->marked = 0;
    sp->fresh = 0;
    /* Every cell goes out with the region; each takes its hash as it is sent. */
    grid_region_painter(g, 0, 0, GRID_COLS, GRID_ROWS, paint, sp, SCENE_PX_RUN_CELLS);
    g->region_peek = peek;
    grid_invalidate(g); /* the region is the whole screen: every cell marked */
}

void scene_px_set(scene_px* sp, uint32_t i, const scene_px_layer* l) {
    if (i >= SCENE_PX_LAYERS) {
        return;
    }
    if (!l) {
        l = &NONE;
    }
    if (same_layer(&sp->layer[i], l)) {
        return;
    }
    if (!sp->fresh) {
        touch_box(sp, sp->box[i]);
    }
    sp->layer[i] = *l;
    layer_box(&sp->layer[i], sp->box[i]);
    if (!sp->fresh) {
        touch_box(sp, sp->box[i]);
    }
    /* The slots with something on the screen, in order: all a tile looks at. */
    sp->nlive = 0;
    for (uint32_t k = 0; k < SCENE_PX_LAYERS; k++) {
        if (sp->box[k][0] < sp->box[k][2]) {
            sp->live[sp->nlive++] = (uint8_t)k;
        }
    }
}

void scene_px_touch(scene_px* sp, int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    x0 = hi(x0, 0), y0 = hi(y0, 0);
    x1 = lo(x1, SCENE_PX_W), y1 = lo(y1, SCENE_PX_H);
    if (x1 <= x0 || y1 <= y0) {
        return;
    }
    for (int32_t r = y0 / TILE; r <= (y1 - 1) / TILE; r++) {
        for (int32_t c = x0 / TILE; c <= (x1 - 1) / TILE; c++) {
            uint32_t t = (uint32_t)r * GRID_COLS + (uint32_t)c;
            uint8_t bit = (uint8_t)(1u << (t & 7u));
            if (!(sp->cand[t >> 3] & bit)) {
                sp->cand[t >> 3] |= bit;
                sp->ncand++;
            }
        }
    }
}

uint32_t scene_px_scan(scene_px* sp, grid* g, uint32_t most) {
    uint32_t t = sp->next;
    for (uint32_t seen = 0; sp->ncand && seen < GRID_CELLS && most; seen++, t++) {
        if (t >= GRID_CELLS) {
            t = 0;
        }
        uint8_t bit = (uint8_t)(1u << (t & 7u));
        if (!(sp->cand[t >> 3] & bit)) {
            continue;
        }
        sp->cand[t >> 3] &= (uint8_t)~bit;
        sp->ncand--;
        uint32_t col = t % GRID_COLS, row = t / GRID_COLS;
        if (grid_marked(g, col, row)) {
            continue; /* it goes out as it will be then, and takes its hash */
        }
        uint8_t px[64];
        scene_px_tile(sp, col, row, px);
        sp->scanned++;
        most--;
        if (scene_px_hash(px) != sp->hash[t]) {
            grid_mark(g, col, row);
            sp->marked++;
        }
    }
    sp->next = (uint16_t)(t >= GRID_CELLS ? 0 : t);
    return sp->ncand;
}

void scene_px_refresh(scene_px* sp, grid* g, uint32_t n) {
    uint32_t t = sp->sweep;
    for (; n; n--) {
        grid_mark(g, t % GRID_COLS, t / GRID_COLS);
        t = t + 1u < GRID_CELLS ? t + 1u : 0u;
    }
    sp->sweep = (uint16_t)t;
}

uint32_t scene_px_hash(const uint8_t px[64]) {
    /* FNV-1a, 32 bits. */
    uint32_t h = 2166136261u;
    for (uint32_t i = 0; i < 64u; i++) {
        h = (h ^ px[i]) * 16777619u;
    }
    return h;
}

/* Draws layer l over the tile at (tx, ty) where its box [x0, x1) x [y0, y1) (clipped to
 * the tile) covers it. */
static void draw(const scene_px_layer* l, int32_t tx, int32_t ty, int32_t x0, int32_t y0,
                 int32_t x1, int32_t y1, uint8_t out[64]) {
    switch (l->kind) {
    case SCENE_PX_FILL:
        for (int32_t y = y0; y < y1; y++) {
            for (int32_t x = x0; x < x1; x++) {
                out[(y - ty) * TILE + (x - tx)] = l->colour;
            }
        }
        break;
    case SCENE_PX_GRADIENT:
        for (int32_t y = y0; y < y1; y++) {
            scene_gradient_row(l->src, (uint32_t)x0, (uint32_t)y, (uint32_t)(x1 - x0),
                               out + (y - ty) * TILE + (x0 - tx));
        }
        break;
    case SCENE_PX_IMAGE:
        /* The box is within the frame: x0 - l->x and y - l->y are >= 0. */
        scene_image_blend(l->src, (uint32_t)(x0 - l->x), (uint32_t)(y0 - l->y),
                          (uint32_t)(x1 - x0), (uint32_t)(y1 - y0), TILE, l->mirror, l->remap,
                          out + (y0 - ty) * TILE + (x0 - tx));
        break;
    case SCENE_PX_MARKS: {
        const scene_px_mark* m = l->src;
        for (uint32_t i = 0; i < l->n; i++) {
            if (m[i].x >= x0 && m[i].x < x1 && m[i].y >= y0 && m[i].y < y1) {
                out[(m[i].y - ty) * TILE + (m[i].x - tx)] = m[i].colour;
            }
        }
        break;
    }
    case SCENE_PX_TEXT: {
        const uint8_t* text = l->src;
        for (int32_t y = y0; y < y1; y++) {
            for (int32_t x = x0; x < x1; x++) {
                uint32_t gx = (uint32_t)(x - l->x);
                uint8_t bits = font_glyph(text[gx / TILE])[y - l->y];
                if (bits & (0x80u >> (gx % TILE))) {
                    out[(y - ty) * TILE + (x - tx)] = l->colour;
                }
            }
        }
        break;
    }
    case SCENE_PX_CALL:
        for (int32_t y = y0; y < y1; y++) {
            l->fn(l->src, (uint32_t)x0, (uint32_t)y, (uint32_t)(x1 - x0),
                  out + (y - ty) * TILE + (x0 - tx));
        }
        break;
    default:
        break;
    }
}

void scene_px_tile(const scene_px* sp, uint32_t col, uint32_t row, uint8_t out[64]) {
    int32_t tx = (int32_t)col * TILE, ty = (int32_t)row * TILE;
    int blank = 1; /* out is still to be cleared to black */
    for (uint32_t k = 0; k < sp->nlive; k++) {
        uint32_t i = sp->live[k];
        const int16_t* b = sp->box[i];
        int32_t x0 = hi(b[0], tx), y0 = hi(b[1], ty);
        int32_t x1 = lo(b[2], tx + TILE), y1 = lo(b[3], ty + TILE);
        if (x0 < x1 && y0 < y1) {
            const scene_px_layer* l = &sp->layer[i];
            /* A fill or a gradient over the whole tile hides the black under it. */
            if (blank && !((l->kind == SCENE_PX_FILL || l->kind == SCENE_PX_GRADIENT) &&
                           x1 - x0 == TILE && y1 - y0 == TILE)) {
                for (uint32_t p = 0; p < 64u; p++) {
                    out[p] = GRID_BLACK;
                }
            }
            blank = 0;
            draw(l, tx, ty, x0, y0, x1, y1, out);
        }
    }
    if (blank) {
        for (uint32_t p = 0; p < 64u; p++) {
            out[p] = GRID_BLACK;
        }
    }
}
