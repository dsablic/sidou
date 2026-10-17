// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* TRACKER (M2d Task 2; docs/art/README.md "TRACKER"): the song's patterns in GoatTracker's
 * layout on the 30 x 30 grid, after the prototype (assets/prototypes/sidou-screens.html).
 *
 * - row 0, the header, and row 1, the voice heads (app/song_view.h);
 * - rows 2-26: 25 pattern rows. Columns 0-1 the row number in hex (white on the bar's row,
 *   yellow every 16, light grey every 4, grey otherwise); then each voice's row of its
 *   pattern from columns 3, 12 and 21, eight cells as C-401308: the note (white; --- for a
 *   rest, === a key off, +++ a key on), the instrument (light blue, .. none), the command
 *   (yellow) and its data (light green), ... for none. A silent voice (muted, or another
 *   soloed) is drawn in dark grey;
 * - column 29: where the bar's row is in its pattern, a pip on a rule;
 * - row 27: a rule; row 28: each voice's orderlist from its playing entry (OL, then the entry
 *   in white on blue, the two after it: a pattern in hex, Rn a repeat, +n / -n a transpose,
 *   RST and the restart position at the end); row 29: the bar row's first command spelled
 *   out in yellow (V2 4 22 VIBRATO), from the voices that sound.
 *
 * Each voice shows the pattern of its orderlist entry: while the song plays the entry and the
 * row the player's row events say (app/song_transport.h status), stopped the cursor's (the
 * first pattern at or after its entry, its row). The bar is on the cursor's voice's row.
 *
 * Two modes (the menu's TRACKER ROWS, song_view.scroll, currently kept in RAM):
 * - BAR (the default): the rows stand still and the bar (light blue on blue) moves down them.
 *   The window is the 16-row page the bar's row is in, with 4 rows of context above it, and
 *   moves a page at a time. A row step changes the bar's two rows; a page turn every row,
 *   which the copy spreads over frames, top first.
 * - SCROLL: GoatTracker's: the bar's row stays on the window's row 12 (screen row 14) and the
 *   rows move under it. Every row step can change every row: at fast tempos the copy lags
 *   (docs/art/README.md measures where).
 *
 * The view reads the song (the binding's song_transport.song, read only: edits reach it
 * through song_ctl, between frames) and builds into the song views' shared canvas, the
 * header and heads when they change, the rows when the window, the patterns, the bar or the
 * mutes do; each frame copies the changed cells of the rows built, top first, within
 * SONG_VIEW_FRAME_CELLS less what the screen's layers take (song_view_copy).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_TRACKER_VIEW_H
#define SIDOU_TRACKER_VIEW_H

#include <stdint.h>

#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The rows' window: 25 rows from screen row 2; in SCROLL the bar on the window's row 12. */
#define TRACKER_VIEW_TOP 2u
#define TRACKER_VIEW_ROWS 25u
#define TRACKER_VIEW_SCROLL_ROW 12u
#define TRACKER_VIEW_PAGE 16u
#define TRACKER_VIEW_CONTEXT 4u
/* Each voice's eight cells from column 3 + 9 v. */
#define TRACKER_VIEW_COL 3u
#define TRACKER_VIEW_W 9u

extern const view tracker_view_view;

/* The window's first pattern row (negative: blank rows above row 0) for the bar on row `row`,
 * in BAR (scroll 0) or SCROLL. */
int32_t tracker_view_top(uint32_t row, int scroll);
/* A row cell's text as the view shows it, into out (9 bytes: 8 and a terminator). */
void tracker_view_cell(const uint8_t cell[4], char out[9]);
/* An orderlist entry's two cells (a pattern, Rn, +n, -n), into out (3 bytes). */
void tracker_view_order_entry(uint8_t entry, char out[3]);
/* A command's name (GoatTracker's readme, 4.x), "" for none. */
const char* tracker_view_command_name(uint32_t cmd);

#ifdef __cplusplus
}
#endif

#endif
