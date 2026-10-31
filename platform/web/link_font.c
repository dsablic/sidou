// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The Live Link viewer's font (tools/wasm-link): app/font.c's glyphs as they are built, under
 * another name, and font_glyphs, the table app/grid.c draws from, as writable memory that
 * starts as a copy of them, so that a device's GLYPHS frames can replace them. (Writing to
 * font.c's own table would write to an object defined const, which the compiler may drop.) */
#define font_glyphs font_glyphs_built
#include "font.c"
#undef font_glyphs

uint8_t font_glyphs[FONT_LAST + 1u][8] __attribute__((aligned(4)));

/* The table, filled with the built font. */
uint8_t* link_font_init(void);
uint8_t* link_font_init(void) {
    for (uint32_t c = 0; c <= FONT_LAST; c++) {
        for (uint32_t y = 0; y < 8u; y++) {
            font_glyphs[c][y] = font_glyphs_built[c][y];
        }
    }
    return &font_glyphs[0][0];
}
