// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The run dictionary and row packets (docs/art/bitmap-codec.md). Cursors count nibbles. */
#ifndef SIDOU_BITMAP_CODEC_H
#define SIDOU_BITMAP_CODEC_H

#include <stdint.h>

__attribute__((always_inline)) static inline uint32_t bitmap_row_mode(
    const uint8_t* data, uint32_t row, uint32_t width, uint32_t x, uint32_t* pixel, int raw) {
    uint32_t shift = data[15] & 15u;
    uint32_t j = x >> shift;
    uint32_t at = (row + ((width - 1u) >> shift)) << 1;
    if (raw) {
        if (j) {
            at += (uint32_t)data[row + j - 1u] << 1;
        }
    } else {
        for (uint32_t i = 0; i < j; i++) {
            at += data[row + i];
        }
    }
    *pixel = j << shift;
    return at;
}

__attribute__((always_inline)) static inline uint32_t bitmap_row(
    const uint8_t* data, uint32_t row, uint32_t width, uint32_t x, uint32_t* pixel) {
    return bitmap_row_mode(data, row, width, x, pixel, data[15] & 128u);
}

__attribute__((always_inline)) static inline uint32_t bitmap_code_run(const uint8_t* data, uint32_t* at) {
    uint32_t i = *at;
    uint32_t code = (data[i >> 1] >> ((i & 1u) << 2)) & 15u;
    i++;
    if (code != 15u) {
        *at = i;
        return data[code];
    }
    uint32_t run = data[i >> 1] >> ((i & 1u) << 2);
    if (i & 1u) {
        run |= (uint32_t)data[(i >> 1) + 1u] << 4;
    }
    *at = i + 2u;
    return run & 255u;
}

__attribute__((always_inline)) static inline uint32_t bitmap_run_mode(
    const uint8_t* data, uint32_t* at, int raw) {
    if (raw) {
        uint32_t run = data[*at >> 1];
        *at += 2u;
        return run;
    }
    return bitmap_code_run(data, at);
}

__attribute__((always_inline)) static inline uint32_t bitmap_run(const uint8_t* data, uint32_t* at) {
    return bitmap_run_mode(data, at, data[15] & 128u);
}

#endif
