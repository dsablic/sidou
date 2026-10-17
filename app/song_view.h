// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* What the song views share (M2d Task 1; docs/reference/controls.md "Song views"): the
 * transport, the mutes and solos, the jam, the header row and the voice heads, and the canvas
 * they draw into (one shows at a time, so they share it).
 *
 * The song views are STEP, TRACKER, ORDER, INSTR and TABLE. SEQ opens the first of them from
 * any view (on top of it: HOME goes back to it) and, in one, steps to the next:
 * STEP, TRACKER, ORDER, INSTR, TABLE and round. A view not made yet is skipped (TRACKER,
 * app/tracker_view.h, is the first made).
 *
 * The transport goes through the player's binding (app/song_transport.h: song_ctl on the
 * device; with none bound the transport stays stopped):
 * - PLAY plays the song from the orderlist entry the views show (the cursor's) and stops it;
 *   with EDIT held it goes on from where it stopped. In every view: the panel never sees it.
 * - REC turns record mode on and off; the header says REC in red. (What it records is M2d
 *   Task 6's.)
 * - GLO held and one of the three leftmost note keys mutes that voice, or lets it sound
 *   again; GLO held and the next three keys solo voice 1, 2 or 3, or unsolo it. While any
 *   voice is soloed, the soloed ones sound and the rest are silent; otherwise the muted ones
 *   are silent. In every view; the keys sound nothing while GLO is held (app/panel.h). The
 *   state shows in every song view's heads and in LIVE's header.
 * - In a song view, with a player bound, a note key auditions the current instrument on the
 *   cursor's voice through the player (a jam, at the panel's octave) and its release lets it
 *   go; one note at a time, the last key wins. Elsewhere, and with no player, the keys play
 *   the instrument.
 * Each command the binding cannot take now (0) is asked again at the next frame
 * (song_view_frame): PLAY's, the mutes, a jam's release; a jam note that cannot go is
 * dropped, late notes being worse than none.
 *
 * Row 0, the header (reverse video): the view's name (columns 0-6), a play mark or a stop
 * square (7), the tempo in BPM, rounded (8-11; "---" with no player), the position as
 * "ORD 03 ROW 1A" in hex (13-25: the player's while it plays, the cursor's otherwise) and REC
 * in red while recording (27-29). The tempo: the frame rate (PAL 985,248 / 19,656 Hz, NTSC
 * 1,022,727 / 17,095), times the play routine's calls a frame (gt2reloc's CIA period for
 * multispeed: 0x4cc7 / N + 1 cycles PAL, 0x42c6 / N + 1 NTSC, decision record 0007), over the
 * calls a row (funktempo: their average) and the rows a beat (4; the MIDI chunk's once M2e
 * lands).
 *
 * Row 1, the voice heads, from columns 3, 12 and 21: V1, V2 and V3 in their waveform's colour
 * (pulse cyan, saw yellow, triangle light green, noise light grey, combined white; grey
 * while silent), the waveform's name (TRI, SAW, PUL, NOI, MIX, ---), MUT in light red when
 * muted or SOL in white when soloed instead, and a one-cell envelope meter in the waveform's
 * colour on dark grey.
 *
 * The views compose when what they show changed, built into the shared canvas and copied onto
 * the grid within SONG_VIEW_FRAME_CELLS cells a frame less what the toast, the raster and a
 * cover take, as CHIP does (app/chip_view.h). An idle view (the transport stopped, nothing
 * changing) composes nothing.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SONG_VIEW_H
#define SIDOU_SONG_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "panel.h"
#include "screen.h"
#include "song_transport.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The cells a frame of a song view sends at most (as CHIP's: about 17.5 ms of SPI). */
#define SONG_VIEW_FRAME_CELLS 200u
/* The song views, in SEQ's order. */
#define SONG_VIEW_STEP 0u
#define SONG_VIEW_TRACKER 1u
#define SONG_VIEW_ORDER 2u
#define SONG_VIEW_INSTR 3u
#define SONG_VIEW_TABLE 4u
#define SONG_VIEW_COUNT 5u
/* The header's columns, and the heads' (voice v's from SONG_VIEW_HEAD_COL + 9 v). */
#define SONG_VIEW_NAME_CHARS 7u
#define SONG_VIEW_HEAD_COL 3u
#define SONG_VIEW_HEAD_W 9u
/* Rows a beat by default (the BPM). */
#define SONG_VIEW_ROWS_A_BEAT 4u
/* No key: song_view.jam_key. */
#define SONG_VIEW_NO_KEY 0xffu
/* What the header and the heads show, as bytes (song_view_head_look): the state, BPM 2,
 * position 2, REC, and each voice's head (colour, name, meter). */
#define SONG_VIEW_HEAD_LOOK (6u + 3u * SONG_T_VOICES)
/* Room for a view's look in song_view.drawn: the head's, then the view's own. */
#define SONG_VIEW_LOOK 48u

typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} song_view_canvas;

typedef struct {
    const song_transport* t; /* NULL: no player */
    live* l;
    song_status st;    /* the player's, as the last frame read it (song_view_frame) */
    uint8_t mute;      /* GLO's mutes, bit v: voice v */
    uint8_t solo;      /* GLO's solos */
    uint8_t sent;      /* the mask the player has (song_view_mute_mask), 0xFF: none yet */
    uint8_t rec;       /* 1: record mode */
    uint8_t want;      /* a PLAY the player has not taken yet (song_view.c WANT_*) */
    uint8_t voice;     /* the cursor's voice, 0..2 */
    uint8_t instr;     /* the current instrument, 1..63 */
    uint8_t order_pos; /* the cursor's orderlist entry */
    uint8_t row;       /* and row */
    uint8_t rows_beat; /* rows a beat, for the BPM */
    uint8_t jam_key;   /* the note key the jam sounds, SONG_VIEW_NO_KEY: none */
    uint8_t jam_voice; /* the voice it sounds on */
    uint8_t jam_off;   /* a jam's release the player has not taken yet */
    uint32_t jam_keys; /* the keys the jam took, until they go up (bit k: key k) */
    const gt2_song* song; /* the player's song (song_transport.song), NULL: none */
    uint32_t song_edits;  /* edits sent: a view reading the song reads it again */
    uint8_t scroll;       /* TRACKER's rows: 0 the moving bar, 1 scrolling (the menu's) */
    /* The shared canvas and its copy (song_view_copy). */
    uint8_t which;     /* the song view it holds, SONG_VIEW_*; 0xFF none */
    uint8_t built;
    uint32_t dirty;    /* rows of the canvas the grid may not have yet (bit r: row r) */
    uint32_t held;     /* frames whose copy the budget cut short (diagnostic) */
    uint8_t drawn[SONG_VIEW_LOOK];
    song_view_canvas cv;
} song_view;

/* No player, nothing muted, the cursor at entry 0 row 0 of voice 1, instrument 1. */
void song_view_init(song_view* sv, live* l);
/* The player's binding (app/song_transport.h), or NULL: none. */
void song_view_set_transport(song_view* sv, const song_transport* t);
/* 1 if v is a song view. */
int song_view_is(const view* v);
/* SEQ: the song view to show after `top` (the view on top): the first one made for a view that
 * is no song view, the next one made for a song view (itself if it is the only one). */
const view* song_view_next(const view* top);
/* PLAY: stops the song if it plays (or is starting), else plays it from the cursor's entry,
 * or with resume (EDIT held) goes on from where it stopped. */
void song_view_play(song_view* sv, int resume);
/* REC: record mode on or off. */
void song_view_rec(song_view* sv);
/* GLO's toggles (panel_take_glo): bit k of the PANEL_GLO_KEYS keys, mutes then solos. */
void song_view_glo(song_view* sv, uint32_t toggles);
/* The voices to keep silent: the unsoloed ones while any is soloed, else the muted ones. */
uint32_t song_view_mute_mask(const song_view* sv);
/* The note keys (views_note_keys), before the panel: keys the jam holds that went up release
 * it; with take (a song view shows, a player is bound, GLO is not held) keys that went down
 * (pressed, or down and in neither `owned` nor the jam's: owned are the keys the panel sounds
 * or keeps quiet) become the jam note. Returns the keys the jam took this call, for the panel
 * to keep quiet (panel_quiet_keys). */
uint32_t song_view_keys(song_view* sv, uint32_t down, uint32_t pressed, uint32_t owned,
                        int take);
/* Once a frame before the screen's: what the player has not taken yet, and with shows (a song
 * view shows) its status; outside the song views PLAY reads the status when pressed. */
void song_view_frame(song_view* sv, int shows);
/* The tempo in tenths of a BPM, rounded: speed_sum the calls of two rows (2 x the speed, or
 * funktempo's two), multispeed 0-4 (0: 25 Hz), rows_beat > 0. 0 for no speed. */
uint32_t song_view_bpm10(uint32_t speed_sum, uint32_t multispeed, uint32_t ntsc,
                         uint32_t rows_beat);
/* Row 0 for the view named name (at most SONG_VIEW_NAME_CHARS) into ch and attr. */
void song_view_header(const song_view* sv, const char* name, uint8_t ch[GRID_COLS],
                      uint8_t attr[GRID_COLS]);
/* Row 1, the heads, on background bg, into ch and attr. */
void song_view_heads(const song_view* sv, uint8_t bg, uint8_t ch[GRID_COLS],
                     uint8_t attr[GRID_COLS]);
/* The header's and the heads' look (SONG_VIEW_HEAD_LOOK bytes): they are drawn again when it
 * changes. */
void song_view_head_look(const song_view* sv, uint8_t out[SONG_VIEW_HEAD_LOOK]);
/* Voice v's orderlist entry and row as the views show them: the player's (its row events)
 * while the song plays and one said, the cursor's otherwise. */
void song_view_position(const song_view* sv, uint32_t v, uint32_t* entry, uint32_t* row);
/* The canvas onto the grid: under a wipe the rows it reveals whole; otherwise the changed
 * cells of the dirty rows and the rows the screen drew over, top first, within
 * SONG_VIEW_FRAME_CELLS less what the screen's layers take, leaving what is covered this
 * frame and the border under the raster; the rows not finished stay dirty for the next. */
void song_view_copy(song_view* sv, screen* s, grid* g);
/* GLO's state for LIVE's header: the mutes in bits 0-2, the solos in bits 3-5. */
uint32_t song_view_mutes(const song_view* sv);

#ifdef __cplusplus
}
#endif

#endif
