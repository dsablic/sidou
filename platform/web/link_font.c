// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#define font_glyphs font_glyphs_built
#include "font.c"
#undef font_glyphs

uint8_t font_glyphs[FONT_LAST + 1u][8] __attribute__((aligned(4)));

uint8_t* link_font_init(void);
uint8_t* link_font_init(void) {
    for (uint32_t c = 0; c <= FONT_LAST; c++) {
        for (uint32_t y = 0; y < 8u; y++) {
            font_glyphs[c][y] = font_glyphs_built[c][y];
        }
    }
    return &font_glyphs[0][0];
}
