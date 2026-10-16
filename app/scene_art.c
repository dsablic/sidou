// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SCENE's pixel art decoders (app/scene_art.h). */
#include "scene_art.h"

/* The 4 x 4 Bayer order the art dithers with (docs/art/README.md), on wide pixels. */
static const uint8_t BAYER[16] = {0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5};

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
        const uint8_t* p = s->data + s->rows[y - s->y];
        uint32_t at = r0; /* the frame pixel the run at p starts at */
        uint32_t len = (uint32_t)(*p >> 4) + 1u;
        while (at + len <= from) {
            at += len;
            len = (uint32_t)(*++p >> 4) + 1u;
        }
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
