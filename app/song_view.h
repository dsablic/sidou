// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* What the song views share (M2d Task 1; docs/reference/controls.md "Song views"): the
 * transport, the mutes and solos, the jam, the header row and the voice heads, and the canvas
 * they draw into (one shows at a time, so they share it).
 *
 * The song views are STEP, TRACKER, ORDER, INSTR and TABLE. SEQ opens the one that showed last
 * from any view (STEP the first time after power-on; on top of it: HOME goes back to it) and, in
 * one, steps to the next:
 * STEP (app/step_view.h), TRACKER (app/tracker_view.h), ORDER, INSTR and TABLE
 * (app/song_pages.h) and round; a view not made yet (a NULL in song_view.c's list) is skipped.
 *
 * The transport goes through the player's binding (app/song_transport.h: song_ctl on the
 * device; with none bound the transport stays stopped):
 * - PLAY plays the song from the orderlist entry the views show (the cursor's) and stops it;
 *   with EDIT held it goes on from where it stopped. In every view: the panel never sees it.
 * - REC turns record mode on and off; the header says REC in red. With the song playing it
 *   records the keys and the K1/K2 knobs (song_view_record_note, song_view_record_knob below).
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
 * Each view chooses its own themed header and voice strips (app/tracker_view.h). While io
 * streams a save, TRACKER shows SAVING in place of its pattern heading; transport, mutes
 * and jam keep running. The tempo uses the PAL/NTSC frame rate, the play routine's calls a
 * frame, the average funktempo speed and the MIDI setup's rows per beat.
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
#include "sid_transport.h"
#include "song_transport.h"
#include "song_io.h"
#include "song_edit.h"

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
/* Non-song owners of the same canvas; ownership changes only when composing. */
#define SONG_VIEW_CANVAS_NONE 0xfdu
#define SONG_VIEW_CANVAS_CHIP 0xfeu
#define SONG_VIEW_CANVAS_MENU 0xffu
/* The SID views' (app/sid_view.h): the browser, NOW PLAYING; and the song views' SID PLAYING
 * while a .sid tune has the song's memory (song_view_away). */
#define SONG_VIEW_CANVAS_SIDS 0xfcu
#define SONG_VIEW_CANVAS_PLAYING 0xfbu
#define SONG_VIEW_CANVAS_AWAY 0xfau
/* LAB's (app/lab_view.h). */
#define SONG_VIEW_CANVAS_LAB 0xf9u
/* Rows a beat by default (the BPM). */
#define SONG_VIEW_ROWS_A_BEAT 4u
/* No key: song_view.jam_key. */
#define SONG_VIEW_NO_KEY 0xffu
/* REC's quantise setting (song_view.quant; the menu's REC QUANT): the nearest row (the
 * default), the row the key falls in, or the nearest with a bar's count-in. */
#define SONG_VIEW_QUANT_NEAREST 0u
#define SONG_VIEW_QUANT_OFF 1u
#define SONG_VIEW_QUANT_COUNT 2u
#define SONG_VIEW_QUANTS 3u
/* Notes recorded and not yet written (the editor takes one cell write at a time). */
#define SONG_VIEW_REC_QUEUE 4u
/* Room for a view's look in song_view.drawn: the head's, then the view's own. */
#define SONG_VIEW_LOOK 48u

/* Cells, colours and the characters of large text's pieces (app/grid.h grid_canvas). */
typedef grid_canvas song_view_canvas;

typedef struct {
    const song_transport* t; /* NULL: no player */
    song_io* io; /* NULL: no song files */
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
    uint8_t which;     /* the canvas owner: SONG_VIEW_* or SONG_VIEW_CANVAS_* */
    uint8_t built;
    uint8_t edit_pending;
    uint32_t dirty;    /* rows of the canvas the grid may not have yet (bit r: row r) */
    uint32_t held;     /* frames whose copy the budget cut short (diagnostic) */
    uint16_t unbuilt;  /* TRACKER: bit j, window row j is not drawn into the canvas yet */
    uint8_t drawn[SONG_VIEW_LOOK];
    song_view_canvas cv;
    song_editor* editor;
    screen* edit_screen; /* pending-edit feedback only; keys receive their actual route's screen */
    uint8_t step;
    uint8_t field;
    uint8_t clear_pattern, clear_row;
    uint8_t clear_armed;
    uint8_t subtune; /* selected orderlist, not necessarily the currently playing one */
    /* REC while the song plays (song_view_record_*): the quantise setting, the count-in's end
     * (a call of the play's count) while one runs, the running cutoff and resonance a knob
     * records (rec_knobs: 0 until the first move reads them from LIVE), and the writes waiting
     * for the editor: pattern, row, note (SONG_VIEW_REC_OFF: a key off where the row is a rest)
     * and instrument. */
    uint8_t quant;
    uint8_t counting;
    uint8_t was_playing;
    uint8_t shown; /* a song view showed at the last frame: was_playing is current */
    uint8_t rec_knobs;
    uint16_t count_end;
    uint16_t rec_cut;
    uint8_t rec_res;
    uint8_t rec_n;
    uint8_t rec_q[SONG_VIEW_REC_QUEUE][4];
    /* The .sid player (app/sid_transport.h; NULL: none), which GLO's mutes and solos reach while
     * a tune has the song's memory (away: the transport is unbound, and the song views show SID
     * PLAYING: STOP TO EDIT and take no input but the views' global keys). */
    const sid_transport* tune;
    uint8_t away;
} song_view;

/* No player, nothing muted, the cursor at entry 0 row 0 of voice 1, instrument 1. */
void song_view_init(song_view* sv, live* l);
/* Acquire before building, never on enter/exit: a covered view may still compose and a pop
 * does not re-enter the view below. Returns 1 on transfer, invalidating the song build/copy. */
int song_view_acquire_canvas(song_view* sv, uint32_t owner);
/* The player's binding (app/song_transport.h), or NULL: none. While an edit is pending the
 * existing binding is retained; finish it before retrying a rebind or replacing its song. */
void song_view_set_transport(song_view* sv, const song_transport* t);
/* After the loader has drained edits and replaced the model: no history or PLAY/jam
 * request from the previous song may act on the new one. */
void song_view_loaded(song_view* sv);
/* SONG's large-text rows borrow the same canvas: each 2x2 block, aligned to even columns
 * and rows, holds its source character in all four cells. Other rows remain ordinary text.
 * big_rows marks both rows of each large-text line. The copy keeps the ordinary frame budget. */
void song_view_copy_large(song_view* sv, screen* s, grid* g, uint32_t big_rows);
/* 1 if v is a song view. */
int song_view_is(const view* v);
/* SEQ: the song view to show after `top` (the view on top): the first one made for a view that
 * is no song view, the next one made for a song view (itself if it is the only one). */
const view* song_view_next(const view* top);
/* PLAY: stops the song if it plays (or is starting), else plays it from the cursor's entry,
 * or with resume (EDIT held) goes on from where it stopped. */
void song_view_play(song_view* sv, int resume);
/* REC: record mode on or off; shows: a song view shows (a COUNT-IN starts only there, where
 * the keys record). */
void song_view_rec(song_view* sv, int shows);
/* REC while the song plays records (song_view_recording): the note a key plays goes into the
 * cursor's voice at the row it was played in (song_view_record_note), the K1 and K2 moves
 * become cutoff and resonance locks on the row they happened in (song_view_record_knob), both
 * through the editor's cell writes and the lock table, as the views' own edits; "SONG FULL" and
 * the other refusals toast on s. */
#define SONG_VIEW_REC_OFF 0xffu
/* 1: REC is on and the song plays: the keys are for recording (during a count-in too, when they
 * sound and record nothing). song_view_recording: and any count-in has run out. */
int song_view_rec_playing(const song_view* sv);
int song_view_recording(const song_view* sv);
/* The rows after the one a voice plays that a key pressed `elapsed` of a row (256ths of a call,
 * row_q8 the row's length in the same) after its start falls in: SONG_VIEW_QUANT_OFF the row it
 * is in, the others the nearest row (halfway rounds up). */
uint32_t song_view_quantise(uint32_t quant, uint32_t elapsed_q8, uint32_t row_q8);
/* Note 0..95 with instrument instr on the cursor's voice; keyoff 1 also writes a key off on
 * the next row if it is a rest (a staccato lane's). 1: queued (or written), 0: not recorded. */
int song_view_record_note(song_view* sv, screen* s, uint32_t note, uint32_t instr, int keyoff);
/* K1 (knob 0, the cutoff) or K2 (1, the resonance) turned by `steps` detents. 1: the lock
 * is written. */
int song_view_record_knob(song_view* sv, screen* s, uint32_t knob, int32_t steps);
/* The knobs' running value, for their popup: 0 cutoff, 1 resonance. */
uint32_t song_view_rec_value(const song_view* sv, uint32_t knob);
/* The writes waiting for the editor, once a frame (song_view_frame calls it). */
void song_view_rec_drain(song_view* sv);
/* GLO's toggles (panel_take_glo): bit k of the PANEL_GLO_KEYS keys, mutes then solos. */
void song_view_glo(song_view* sv, uint32_t toggles);
/* The voices to keep silent: the unsoloed ones while any is soloed, else the muted ones. */
uint32_t song_view_mute_mask(const song_view* sv);
/* The note keys (views_note_keys), before the panel: keys the jam holds that went up release
 * it; with take (a song view shows, a player is bound, GLO is not held) keys that went down
 * (pressed, or down and in neither `owned` nor the jam's: owned are the keys the panel sounds
 * or keeps quiet) become the jam note. Returns the keys the jam took this call, for the panel
 * to keep quiet (panel_quiet_keys). With REC on, recording_screen is the active TRACKER's
 * screen (also under MIDI learn), or NULL outside TRACKER; canvas ownership is unrelated.
 * While REC records (song_view_recording) each key's note is recorded as well, on live_screen,
 * the screen of the song view on top (TRACKER's or STEP's), in place of TRACKER's cursor entry. */
uint32_t song_view_keys(song_view* sv, uint32_t down, uint32_t pressed, uint32_t owned,
                        int take, screen* recording_screen, screen* live_screen);
/* Once a frame before the screen's: what the player has not taken yet, and with shows (a song
 * view shows) its status; outside the song views PLAY reads the status when pressed. */
void song_view_frame(song_view* sv, int shows);
/* 1 while the song plays or is about to (the header's play mark). */
int song_view_playing(const song_view* sv);
/* The tempo as the header shows it, in whole BPM; SONG_VIEW_NO_BPM with no player or no speed. */
#define SONG_VIEW_NO_BPM 0xFFFFu
uint32_t song_view_shown_bpm(const song_view* sv);
/* The tempo in tenths of a BPM, rounded: speed_sum the calls of two rows (2 x the speed, or
 * funktempo's two), multispeed 0-4 (0: 25 Hz), rows_beat > 0. 0 for no speed. */
uint32_t song_view_bpm10(uint32_t speed_sum, uint32_t multispeed, uint32_t ntsc,
                         uint32_t rows_beat);
/* Voice v's orderlist entry and row as the views show them: the player's (its row events)
 * while the song plays and one said, the cursor's otherwise. */
void song_view_position(const song_view* sv, uint32_t v, uint32_t* entry, uint32_t* row);
/* The canvas onto the grid: under a wipe the rows it reveals whole; otherwise the changed
 * cells of the dirty rows and the rows the screen drew over, top first, within
 * SONG_VIEW_FRAME_CELLS less what the screen's layers take, leaving what is covered this
 * frame and the border under the raster; the rows not finished stay dirty for the next. */
void song_view_copy(song_view* sv, screen* s, grid* g);
/* A .sid tune has the song's memory (on), or the song has it back: GLO's mutes go to the tune
 * (song_view.tune) from now on, and the song views draw SID PLAYING. */
void song_view_away(song_view* sv, int on);
/* The song views while away: their changed, compose and input (K1..K4 go on to the panel, the
 * keys' sound; anything else is a toast and consumed). */
int song_view_away_changed(const song_view* sv);
void song_view_away_compose(song_view* sv, screen* s, grid* g);
int song_view_away_input(screen* s, const screen_event* e);
/* GLO's state for LIVE's header: the mutes in bits 0-2, the solos in bits 3-5. */
uint32_t song_view_mutes(const song_view* sv);
/* An instrument as PRESETS' popup shows it: its number in hex and its name, as far as the
 * popup's width and the name's printable characters go ("03 KICK"); the number alone with no
 * song or no name. */
void song_view_instr_text(const song_view* sv, uint32_t instr, char out[SCREEN_POPUP_TEXT]);

#ifdef __cplusplus
}
#endif

#endif
