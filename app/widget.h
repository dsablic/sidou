// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Widgets on the cell grid (app/grid.h): a PETSCII frame with a title, numbers, meter bars,
 * and a menu, a list of items that show a value and change it by knob, with a blinking block
 * cursor (docs/art/README.md).
 *
 * A menu is data: a const table of items in flash, each a label and two functions, and a
 * little state (the cursor and the first row shown). The view that owns the menu routes an
 * encoder to menu_move and another to menu_turn, and draws it every frame with menu_draw.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_WIDGET_H
#define SIDOU_WIDGET_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A value's text, terminator included. */
#define MENU_VALUE_CHARS 16u

typedef struct {
    const char* label;
    /* The item's value as text into buf (MENU_VALUE_CHARS); NULL makes the item an action,
     * shown with an arrow. */
    void (*value)(void* ctx, char* buf);
    /* steps detents (+ = clockwise): a value item changes; an action runs on a clockwise
     * turn. */
    void (*turn)(void* ctx, int32_t steps);
} menu_item;

typedef struct {
    const menu_item* items;
    void* ctx; /* handed to the items' functions */
    uint8_t count;
    uint8_t rows; /* rows shown at once */
    uint8_t cursor;
    uint8_t top; /* the first item shown */
} menu;

/* A frame of box glyphs around w x h cells (w, h >= 2) with title (NULL: none) set into its
 * top edge after the corner, between spaces, cut before the other corner. The inside is left
 * alone. */
void widget_frame(grid* g, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t attr,
                  const char* title);
/* The outer ring of cells in ch, and the inside cleared to spaces: a screen's frame. */
void widget_screen(grid* g, uint8_t ch, uint8_t attr);
/* value right-aligned in width cells, with a sign if sign. */
void widget_number(grid* g, uint32_t col, uint32_t row, uint32_t width, int32_t value, int sign,
                   uint8_t attr);
/* A meter of cells bar glyphs (app/font.h FONT_BAR0..8) filled to value / max. */
void widget_bar(grid* g, uint32_t col, uint32_t row, uint32_t cells, uint32_t value, uint32_t max,
                uint8_t attr);

/* The cursor on the first item, scrolled to the top. */
void menu_init(menu* m, const menu_item* items, uint32_t count, uint32_t rows, void* ctx);
/* The cursor steps items down (+) or up, clamped at both ends; the rows shown follow it. */
void menu_move(menu* m, int32_t steps);
/* The item under the cursor turned by steps. */
void menu_turn(menu* m, int32_t steps);
/* The rows shown from (col, row), w cells wide: the cursor cell (the block while cursor_on,
 * on the selected row), the label, and the value right-aligned in the last cells (an
 * action's arrow in the last). */
void menu_draw(const menu* m, grid* g, uint32_t col, uint32_t row, uint32_t w, uint8_t attr,
               int cursor_on);

#ifdef __cplusplus
}
#endif

#endif
