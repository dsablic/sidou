// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SCENE's pixel art decoders (app/scene_art.h). */
#include "scene_art.h"

#include <stddef.h>

/* The 4 x 4 Bayer order the art dithers with (docs/art/README.md), on wide pixels. */
static const uint8_t BAYER[16] = {0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5};

/* The run of coded row r that holds frame pixel `from` (s->x <= from < s->x + s->w), from the
 * checkpoint before it; *at is the frame pixel the run starts at. */
static const uint8_t* seek_run(const scene_image* s, uint32_t r, uint32_t from, uint32_t* at) {
    const uint8_t* p = s->data + s->rows[r];
    uint32_t a = s->x;
    uint32_t j = (from - a) / SCENE_ART_SEEK;
    if (j && s->seek) {
        uint32_t k = ((uint32_t)s->w - 1u) / SCENE_ART_SEEK;
        p += s->seek[r * k + j - 1u];
        a += j * SCENE_ART_SEEK;
    }
    uint32_t len = (uint32_t)(*p >> 4) + 1u;
    while (a + len <= from) {
        a += len;
        len = (uint32_t)(*++p >> 4) + 1u;
    }
    *at = a;
    return p;
}

void scene_image_row(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, int mirror,
                     uint8_t* out) {
    if (!n) {
        return;
    }
    /* The frame's pixels [a, a + n), left to right; reversed at the end when mirrored. */
    uint32_t a = mirror ? (uint32_t)s->fw - x - n : x;
    if (s->base) {
        scene_image_row(s->base, a, y, n, 0, out);
    } else {
        for (uint32_t i = 0; i < n; i++) {
            out[i] = SCENE_ART_CLEAR;
        }
    }
    uint32_t r0 = s->x, r1 = (uint32_t)s->x + s->w;
    if (y >= s->y && y < (uint32_t)s->y + s->h && a < r1 && a + n > r0) {
        uint32_t from = a > r0 ? a : r0;
        uint32_t to = a + n < r1 ? a + n : r1;
        uint32_t at;
        const uint8_t* p = seek_run(s, y - s->y, from, &at);
        uint32_t len = (uint32_t)(*p >> 4) + 1u;
        for (uint32_t px = from; px < to;) {
            uint32_t end = at + len < to ? at + len : to;
            uint8_t c = (uint8_t)(*p & 15u);
            uint8_t v = c == s->clear ? SCENE_ART_CLEAR : c;
            while (px < end) {
                out[px++ - a] = v;
            }
            at += len;
            if (px < to) {
                len = (uint32_t)(*++p >> 4) + 1u;
            }
        }
    }
    if (mirror) {
        for (uint32_t i = 0, j = n - 1u; i < j; i++, j--) {
            uint8_t t = out[i];
            out[i] = out[j];
            out[j] = t;
        }
    }
}

/* What scene_image_blend draws into: screen pixels [sx, ex) of the image as drawn, sx at out,
 * each row stride after the last. */
typedef struct {
    uint8_t* out;
    uint32_t stride;
    uint32_t sx, ex;
    uint32_t fw;
    uint32_t sh; /* log2 of gx */
    int mirror;
    const uint8_t* remap;
} blend_span;

/* Draws the runs of coded row r of s (no patch) over frame pixels [from, to), within its
 * rectangle, into the row at out: p the run that holds pixel from, starting at pixel at. */
static inline void blend_runs(const scene_image* s, const uint8_t* p, uint32_t at, uint32_t from,
                              uint32_t to, const blend_span* b, uint8_t* out) {
    /* Locals: a store through out could alias b's fields. */
    uint32_t clear = s->clear, sh = b->sh, sx = b->sx, ex = b->ex, fw = b->fw;
    int mirror = b->mirror;
    const uint8_t* remap = b->remap;
    uint32_t end = at + (uint32_t)(*p >> 4) + 1u;
    while (end <= from) {
        at = end;
        end += (uint32_t)(*++p >> 4) + 1u;
    }
    for (;;) {
        uint32_t c = *p & 15u;
        if (c != clear) {
            uint32_t a = at > from ? at : from;
            uint32_t e = end < to ? end : to;
            uint32_t q0, q1; /* the screen pixels the stored pixels [a, e) show at */
            if (mirror) {
                q0 = (fw - e) << sh;
                q1 = (fw - a) << sh;
            } else {
                q0 = a << sh;
                q1 = e << sh;
            }
            q0 = q0 > sx ? q0 : sx;
            q1 = q1 < ex ? q1 : ex;
            uint8_t v = (uint8_t)(remap ? remap[c] : c);
            for (uint8_t *o = out + (q0 - sx), *oe = out + (q1 - sx); o < oe;) {
                *o++ = v;
            }
        }
        if (end >= to) {
            return;
        }
        at = end;
        end += (uint32_t)(*++p >> 4) + 1u;
    }
}

/* Draws frame pixels [u0, u1) of rows [y, y + h) of s, and of its base where the base shows.
 * Row y goes to out. */
static void blend_rows(const scene_image* s, uint32_t u0, uint32_t u1, uint32_t y, uint32_t h,
                       const blend_span* b, uint8_t* out) {
    uint32_t r0 = s->x, r1 = r0 + s->w;
    if (s->base) {
        /* A patch: its own rows over its rectangle, the base's round it. */
        for (uint32_t i = 0; i < h; i++, out += b->stride) {
            uint32_t r = y + i - s->y; /* unsigned: rows above wrap past h */
            if (r >= s->h || u1 <= r0 || u0 >= r1) {
                blend_rows(s->base, u0, u1, y + i, 1, b, out);
                continue;
            }
            if (u0 < r0) {
                blend_rows(s->base, u0, r0, y + i, 1, b, out);
            }
            if (u1 > r1) {
                blend_rows(s->base, r1, u1, y + i, 1, b, out);
            }
            uint32_t from = u0 > r0 ? u0 : r0, to = u1 < r1 ? u1 : r1, at;
            const uint8_t* p = seek_run(s, r, from, &at);
            blend_runs(s, p, at, from, to, b, out);
        }
        return;
    }
    uint32_t from = u0 > r0 ? u0 : r0, to = u1 < r1 ? u1 : r1;
    uint32_t ya = y > s->y ? y : s->y;
    uint32_t yb = y + h < (uint32_t)s->y + s->h ? y + h : (uint32_t)s->y + s->h;
    if (from >= to || ya >= yb) {
        return;
    }
    /* The same checkpoint on every row. */
    uint32_t j = (from - r0) / SCENE_ART_SEEK;
    const uint8_t* seek = j && s->seek ? s->seek + j - 1u : NULL;
    uint32_t k = ((uint32_t)s->w - 1u) / SCENE_ART_SEEK;
    uint32_t start = seek ? r0 + j * SCENE_ART_SEEK : r0;
    out += (ya - y) * b->stride;
    for (uint32_t r = ya - s->y; r < yb - s->y; r++, out += b->stride) {
        const uint8_t* p = s->data + s->rows[r];
        if (seek) {
            p += seek[r * k];
        }
        blend_runs(s, p, start, from, to, b, out);
    }
}

void scene_image_blend(const scene_image* s, uint32_t x, uint32_t y, uint32_t n, uint32_t h,
                       uint32_t stride, int mirror, const uint8_t* remap, uint8_t* out) {
    if (!n || !h) {
        return;
    }
    blend_span b = {out, stride, x, x + n, s->fw, s->gx >> 1, mirror, remap};
    /* The stored pixels under screen pixels [x, x + n). */
    uint32_t u0 = x >> b.sh, u1 = ((x + n - 1u) >> b.sh) + 1u;
    if (mirror) {
        uint32_t t = b.fw - u1;
        u1 = b.fw - u0;
        u0 = t;
    }
    blend_rows(s, u0, u1, y, h, &b, out);
}

void scene_gradient_row(const scene_gradient* g, uint32_t x, uint32_t y, uint32_t n,
                        uint8_t* out) {
    uint32_t i = 0;
    while (i + 1u < g->n && y >= g->stops[i + 1u].y) {
        i++;
    }
    uint8_t c = g->stops[i].colour;
    if (i + 1u >= g->n || y + g->seam < g->stops[i + 1u].y) {
        for (uint32_t k = 0; k < n; k++) {
            out[k] = c;
        }
        return;
    }
    /* In the seam: the next colour where the Bayer level on wide pixels is under
     * (d + 0.5) / seam of the way, d the row's place in the seam (in whole numbers:
     * 2 * level * seam < (2d + 1) * 16). */
    uint8_t next = g->stops[i + 1u].colour;
    uint32_t d = y + g->seam - g->stops[i + 1u].y;
    uint32_t lim = (2u * d + 1u) * 16u;
    const uint8_t* bay = BAYER + (y & 3u) * 4u;
    for (uint32_t k = 0; k < n; k++) {
        uint32_t px = x + k;
        out[k] = 2u * bay[(px >> 1) & 3u] * g->seam < lim ? next : c;
    }
}
