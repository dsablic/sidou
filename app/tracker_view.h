// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* TRACKER (M2d Task 2; docs/art/README.md "TRACKER"): the song's patterns in GoatTracker's
 * layout, drawn to be read at 23 mm (after the screen-readability prototype's trkNew): one voice
 * at a time at 2 times the font, the other two as activity strips.
 *
 * - rows 0-1, the header: the shown voice "V1" at 2x in its colour (light red, cyan, light
 *   green; grey while the song mutes it, with MUT or SOL small beside the pattern), the shown
 *   voice's pattern "PAT 03" (hex) at 2x, the play mark or a stop square, the tempo in BPM small
 *   and REC in red;
 * - row 2: the other two voices' numbers over their strips;
 * - rows 3-26: 12 pattern rows of two cell rows each: the row's number small at the left (hex;
 *   yellow on the current row), the note at 2x (white; --- a rest and ... in dark grey), the
 *   instrument at 2x in the voice's colour (.. none, dark grey) and the command small
 *   (`308`: the command's number and its data). The current row is a dark grey band across
 *   columns 0-17. A muted voice's rows keep their colours (its header number and its strip go
 *   grey: a whole voice's recolour is what a mute would otherwise send);
 * - columns 26 and 28: the other two voices as strips, a bar a row, tall where a note sounds
 *   on that row of that voice's own pattern and short where none does, in the voice's colour
 *   (dark grey while muted);
 * - row 27: OL and the shown voice's orderlist from its entry (the entry in white on the band,
 *   the two after it in grey: a pattern in hex, Rn a repeat, +n / -n a transpose, RST and the
 *   restart position at the end); row 28: the current row's first command, of the voices
 *   that sound, spelled out (V2 4 22 VIBRATO); row 29: an auditioned instrument's number and
 *   name, and the position (ORD 03 ROW 1A); what the controls do is the help card's.
 *
 * The cursor is a voice (song_view.voice, the shown one) and a row: while the song plays the
 * row its player's row events say for that voice (app/song_transport.h status), stopped the
 * cursor's (the first pattern at or after its entry, its row: song_view.row). With REC on and
 * the song stopped the edit cursor shows; while REC records (the song playing) the rows follow
 * the player as they do with REC off, so the notes are seen landing, and the cursor is kept for
 * when it stops. SELECT steps rows (EDIT held: an 8-row page), ALGORITHM steps the
 * voice 1, 2, 3 and round; the note keys audition on it (app/song_view.h). The
 * other voices' rows are those of their own patterns at the same row number.
 *
 * Two modes (the menu's TRACKER ROWS, song_view.scroll, kept with the settings):
 * - BAR (the default): the rows stand still and the bar (dark grey) moves down them. The window
 *   is the 8-row page the bar's row is in, with 4 rows of context above it, and moves a page
 *   at a time. A row step changes the bar's two rows; a page turn every row, which the copy
 *   spreads over frames, top first.
 * - SCROLL: GoatTracker's: the bar's row stays on the window's row 5 and the rows move under
 *   it. Every row step can change every row: at fast tempos the copy lags
 *   (docs/art/README.md measures where).
 *
 * The row drawing (draw_row in tracker_view.c) takes the cursor row, the voice's colour and a
 * field to highlight (TRACKER_VIEW_FIELD_*): an edit cursor on a note, an instrument or a
 * command is the same drawing with a field named.
 *
 * The view reads the song (the binding's song_transport.song, read only: edits reach it
 * through song_ctl, between frames) and builds into the song views' shared canvas, the
 * header when it changes (the tempo, the voice, REC, a mute), the rows when the window, the
 * patterns, the bar or a mute does; each frame copies the changed cells of the rows built, top
 * first, within SONG_VIEW_FRAME_CELLS less what the screen's layers take (song_view_copy).
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

/* The rows' window: 12 pattern rows of TRACKER_VIEW_ROW_CELLS cell rows each from screen row 3; in
 * SCROLL the bar on the window's row 5; in BAR a page of 8 with 4 rows of context above it. */
#define TRACKER_VIEW_TOP 3u
#define TRACKER_VIEW_ROWS 12u
#define TRACKER_VIEW_ROW_CELLS 2u
/* The window rows a frame builds into the canvas when it builds the whole window: a frame copies at most
 * SONG_VIEW_FRAME_CELLS cells, which about this many rows hold. */
#define TRACKER_VIEW_BUILD_ROWS 6u
#define TRACKER_VIEW_SCROLL_ROW 5u
#define TRACKER_VIEW_PAGE 8u
#define TRACKER_VIEW_CONTEXT 4u
/* A row's columns: its number (2 cells), the note at 2x (3 characters), the instrument at 2x
 * (2) and the command small (3); the bar covers columns 0..TRACKER_VIEW_BAND_W - 1: what a row
 * step repaints is two such bars, so the width is what the step costs. */
#define TRACKER_VIEW_NUM_COL 0u
#define TRACKER_VIEW_NOTE_COL 3u
#define TRACKER_VIEW_INS_COL 10u
#define TRACKER_VIEW_CMD_COL 15u
#define TRACKER_VIEW_BAND_W 18u
/* The other two voices' strips: a cell column each, and their numbers over them on row 2. */
#define TRACKER_VIEW_STRIP_COL 26u
#define TRACKER_VIEW_STRIP_STEP 2u
#define TRACKER_VIEW_TAG_ROW 2u
/* The orderlist's row, the command's and the hint's. */
#define TRACKER_VIEW_ORDER_ROW 27u
#define TRACKER_VIEW_SAY_ROW 28u
#define TRACKER_VIEW_HINT_ROW 29u
/* The field of a row to highlight (an edit cursor, tracker_view.c draw_row). */
#define TRACKER_VIEW_FIELD_NONE 0u
#define TRACKER_VIEW_FIELD_NOTE 1u
#define TRACKER_VIEW_FIELD_INSTRUMENT 2u
#define TRACKER_VIEW_FIELD_COMMAND 3u

extern const view tracker_view_view;

/* The window's first pattern row (negative: blank rows above row 0) for the bar on row `row`,
 * in BAR (scroll 0) or SCROLL. */
int32_t tracker_view_top(uint32_t row, int scroll);
/* A row cell's text as the view shows it, into out (9 bytes: 8 and a terminator). */
void tracker_view_cell(const uint8_t cell[4], char out[9]);
/* An orderlist entry (a pattern, Rn, +n, -n), into out (4 bytes including -10's terminator). */
void tracker_view_order_entry(uint8_t entry, char out[4]);
/* A command's name (GoatTracker's readme, 4.x), "" for none. */
const char* tracker_view_command_name(uint32_t cmd);
void tracker_view_note(screen* s, song_view* sv, uint32_t note);
int tracker_view_popup(const song_view* sv, uint32_t encoder, int edit, char* name, char* value,
                       uint32_t* fill, uint32_t* max);

#ifdef __cplusplus
}
#endif

#endif
