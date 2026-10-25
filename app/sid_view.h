// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The SID views (M3 Task 11; docs/plans/2026-10-19-m3-sid-files.md, "The browser and the
 * transport"; docs/reference/controls.md "SIDS"): the browser of the store's .sid tunes and NOW
 * PLAYING, over the tune's player (app/sid_transport.h).
 *
 * - SIDS, the browser (the menu's SIDS): the store's .sid files by their store names, in name
 *   order (app/sid_io.h sid_io_next), eight a page at 2x as SONG's LOAD lists songs (a name past
 *   14 characters ends in an arrow), the loaded tune marked (a play mark while it plays, a square
 *   while it is paused or stopped). SELECT moves (and pages); ALGORITHM clockwise, or SAVE (the
 *   page's OK), loads the highlighted tune and opens NOW PLAYING; on the loaded tune it opens NOW
 *   PLAYING as it is. An empty store says NO TUNES: SEE MANUAL. HOME goes back; with the tune
 *   stopped (or failed) it also ends it for good: the song comes back from the store.
 * - The question: with no tune loaded and the song changed since its last save, a tune asks
 *   first (SONG NOT SAVED, the song's name): SAVE saves the song (a named one in place; an
 *   unnamed one opens SAVE AS, and the question waits for it) and the tune loads once it is
 *   saved; ALGORITHM clockwise again discards the song's edits and loads the tune; HOME cancels.
 * - NOW PLAYING: the header's title at 2x on two lines, its author and released at 1x, SONG n/m
 *   (ALGORITHM turned steps the song, restarting it from init; SELECT does nothing), the time
 *   since the song started and the state, PAL or NTSC, each chip's model, a 2SID mark (and a
 *   third SID's, 3SID: 3RD SILENT), the three voice lanes with GLO's mutes and solos (as
 *   TRACKER's header: the voice's colour, dark grey while silent, reversed while soloed; MUT and
 *   SOL) and chip A's envelopes, and the overruns and late writes when not zero.
 * - PLAY (global, sid_view_play) while a tune is loaded: plays the loaded song from init when it
 *   is stopped, stops it when it plays or is paused; EDIT held + PLAY pauses and resumes (on a
 *   stopped tune it plays). In a view other than these two, PLAY on a stopped tune ends it for
 *   good, so the song is back for the next. A paused or stopped tune keeps its place.
 * - GLO's mutes and solos reach the tune (app/song_view.h song_view.tune): voice v of both chips
 *   for a 2SID tune.
 * - STATUS's 6502 rows (sid_view_status_rows) while a tune is loaded.
 *
 * Both views draw into the song views' canvas (app/song_view.h song_view_acquire_canvas: one
 * shows at a time), build what changed and copy it within SONG_VIEW_FRAME_CELLS a frame, as the
 * song views do. An idle NOW PLAYING composes what its time, envelopes and counters move.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SID_VIEW_H
#define SIDOU_SID_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "screen.h"
#include "sid_transport.h"
#include "song_view.h"
#include "store.h"
#include "theme.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Files a page of the browser, and the characters of a name at 2x (the rest an arrow). */
#define SID_VIEW_FILES 8u
#define SID_VIEW_NAME_CELLS 14u
/* What NOW PLAYING showed at its last build (sid_view.drawn), and STATUS's rows. */
#define SID_VIEW_LOOK 28u
#define SID_VIEW_STATUS_LOOK 16u
/* The envelope bars: 2x characters of 8 pixels' fill each. */
#define SID_VIEW_BAR_CHARS 7u
/* sid_view_save: an unnamed song to save first (the views open SAVE AS). */
#define SID_VIEW_SAVE_AS 2

typedef struct {
    const sid_transport* t; /* NULL: no player */
    song_view* sv;          /* the canvas, GLO's mutes and the song files (sv->io) */
    const store* st;        /* the store the browser lists; NULL: none */
    uint8_t cursor, page, count, scanning, skip, built, drawn_theme;
    uint8_t asking;  /* the question shows */
    uint8_t saving;  /* SAVE answered it: the tune loads once the song is saved */
    uint8_t mark;    /* the browser's loaded-tune mark as built (its line and state) */
    uint8_t flags;   /* and the question's and the song files' state */
    uint8_t zones;   /* NOW PLAYING's zones built (sid_view.c ZONE_*) */
    char after[STORE_NAME_MAX + 1u];
    char files[SID_VIEW_FILES][STORE_NAME_MAX + 1u];
    char chosen[STORE_NAME_MAX + 1u]; /* the question's tune */
    uint8_t drawn[SID_VIEW_LOOK];
} sid_view;

extern const view sid_view_browser;
extern const view sid_view_playing;

/* No player, no store, the browser at its first page. */
void sid_view_init(sid_view* sd, song_view* sv);
/* The tune's player (also GLO's, song_view.tune), or NULL. */
void sid_view_set_transport(sid_view* sd, const sid_transport* t);
/* The browser on top of s (the menu's SIDS): its page read again, the cursor where it was. */
void sid_view_open(sid_view* sd, screen* s);
/* 1 if v is one of the SID views. */
int sid_view_is(const view* v);
/* The tune's state (SID_T_*; SID_T_NONE with no player). */
uint32_t sid_view_state(const sid_view* sd);
/* PLAY (EDIT held: edit), in a SID view (here) or another: 1 when a tune is loaded and took it. */
int sid_view_play(sid_view* sd, int edit, int here);
/* HOME let go on the browser: 1 when it cancelled the question (HOME goes nowhere else); 0 to go
 * back a view, the tune ended for good first if it is stopped. */
int sid_view_home(sid_view* sd);
/* SAVE on the browser: the question's save (SID_VIEW_SAVE_AS for an unnamed song, which the
 * views open SAVE AS for), else the page's OK; 1. */
int sid_view_save(sid_view* sd, screen* s);
/* Once a frame: the question's save waited for (the tune loads once the song is saved). */
void sid_view_frame(sid_view* sd, screen* s);
/* STATUS's rows while a tune is loaded: 1 when they changed since `drawn` (SID_VIEW_STATUS_LOOK,
 * then updated). sid_view_status_draw draws them as STATUS's first page: STATUS_LINES lines at 2x,
 * a line every STATUS_LINE_ROWS rows from `row` (state, CPU, T/KC, LATE, OVERRUNS, CYC/S, SHADOW,
 * 3RD SID). */
#define STATUS_LINES 8u
#define STATUS_LINE_ROWS 3u
int sid_view_status_look(const sid_view* sd, uint8_t drawn[SID_VIEW_STATUS_LOOK], int update);
void sid_view_status_draw(const sid_view* sd, grid* g, const theme* th, uint32_t row);

#ifdef __cplusplus
}
#endif

#endif
