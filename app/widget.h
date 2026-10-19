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
/* Rows a menu shows at most (menu_init clamps), and the cells of a line at scale 2 at most. */
#define MENU_MAX_ROWS 8u
#define MENU_LINE_CELLS 60u
/* force_rows uses only the grid's 30 rows; its high bit budgets forced restoration too. */
#define MENU_BUDGET_FORCED (1u << 31)

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
    /* 1 (menu_init): the font's size; 2: large text, an item to a line of w / 2 characters and
     * two cell rows, a blank row between, and the selected item's whole line in sel_attr (0: attr, as the others; no block cursor, so a menu that
     * does not move draws nothing, cursor_on does not blink it). */
    uint8_t scale;
    uint8_t sel_attr;
    /* At scale 2: the colour of the values (the unselected lines' right-aligned text), 0: as the
     * line's. A second colour for what the lines say, as the menu's THEME gives it. */
    uint8_t value_attr;
    /* At scale 2 (a line is up to 60 cells, a scroll changes them all): menu_draw draws the
     * lines whose text changed since they were drawn, the selected one first, while what the
     * grid has pending (grid_pending) leaves room for another line within `budget` cells (0: no
     * limit); the first line makes progress even while a view switch has old cells pending.
     * Rows in force_rows (what something drew over, or all after a view switch) are drawn
     * regardless of budget unless force_rows includes MENU_BUDGET_FORCED. Deferred forced rows
     * are invalidated
     * so the next call restores them. `pending`
     * says lines were left for the next call. The caller sets budget and
     * force_rows before each call. */
    uint16_t budget;
    uint8_t pending;
    uint8_t valid; /* bit k: line k's source text and colours can be compared in the grid */
    uint32_t force_rows;
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

/* Empty 2x quadrants as pixel-identical blanks; their source stays in grid.src. */
void widget_blank_pieces(grid* g, uint32_t col, uint32_t row, uint32_t w);

/* The cursor on the first item, scrolled to the top. */
void menu_init(menu* m, const menu_item* items, uint32_t count, uint32_t rows, void* ctx);
/* The cursor steps items down (+) or up, clamped at both ends; the rows shown follow it. */
void menu_move(menu* m, int32_t steps);
/* The item under the cursor turned by steps. */
void menu_turn(menu* m, int32_t steps);
/* The rows shown from (col, row), w cells wide: the cursor cell (the block while cursor_on,
 * on the selected row), the label, and the value right-aligned in the last cells (an
 * action's arrow in the last). At scale 2: one item every 3 rows from `row`, its text (w - 2) / 2
 * characters wide inside a cell of margin either side; a line that cannot hold its label and
 * value shows the value cut. The caller says what is out of sight (menu.top, count and rows). */
void menu_draw(menu* m, grid* g, uint32_t col, uint32_t row, uint32_t w, uint8_t attr,
               int cursor_on);

#ifdef __cplusplus
}
#endif

#endif
