// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
#include "song_gen.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_VIEW_FRAME_CELLS 200u
#define SONG_VIEW_STEP 0u
#define SONG_VIEW_TRACKER 1u
#define SONG_VIEW_ORDER 2u
#define SONG_VIEW_INSTR 3u
#define SONG_VIEW_TABLE 4u
#define SONG_VIEW_COUNT 5u
#define SONG_VIEW_CANVAS_NONE 0xfdu
#define SONG_VIEW_CANVAS_CHIP 0xfeu
#define SONG_VIEW_CANVAS_MENU 0xffu
#define SONG_VIEW_CANVAS_SIDS 0xfcu
#define SONG_VIEW_CANVAS_PLAYING 0xfbu
#define SONG_VIEW_CANVAS_AWAY 0xfau
#define SONG_VIEW_CANVAS_LAB 0xf9u
#define SONG_VIEW_ROWS_A_BEAT 4u
#define SONG_VIEW_NO_KEY 0xffu
#define SONG_VIEW_QUANT_NEAREST 0u
#define SONG_VIEW_QUANT_OFF 1u
#define SONG_VIEW_QUANT_COUNT 2u
#define SONG_VIEW_QUANTS 3u
#define SONG_VIEW_REC_QUEUE 4u
#define SONG_VIEW_LOOK 48u

typedef grid_canvas song_view_canvas;

typedef struct song_view {
    const song_transport* t;
    song_io* io;
    live* l;
    song_status st;
    uint8_t mute;
    uint8_t solo;
    uint8_t sent;
    uint8_t rec;
    uint8_t want;
    uint8_t voice;
    uint8_t instr;
    uint8_t order_pos;
    uint8_t row;
    uint8_t rows_beat;
    uint8_t jam_key;
    uint8_t jam_voice;
    uint8_t jam_off;
    uint32_t jam_keys;
    const gt2_song* song;
    uint32_t song_edits;
    uint8_t scroll;
    uint8_t which;
    uint8_t built;
    uint8_t edit_pending;
    uint32_t dirty;
    uint32_t held;
    uint16_t unbuilt;
    uint8_t drawn[SONG_VIEW_LOOK];
    song_view_canvas cv;
    song_editor* editor;
    screen* edit_screen;
    uint8_t step;
    uint8_t field;
    uint8_t clear_pattern, clear_row;
    uint8_t clear_armed;
    uint8_t subtune;
    uint8_t quant;
    uint8_t counting;
    uint8_t was_playing;
    uint8_t shown;
    uint8_t rec_knobs;
    uint16_t count_end;
    uint16_t rec_cut;
    uint8_t rec_res;
    uint8_t rec_n;
    uint8_t rec_q[SONG_VIEW_REC_QUEUE][4];
    const sid_transport* tune;
    uint8_t away;
    song_gen gen;
} song_view;

void song_view_init(song_view* sv, live* l);
int song_view_acquire_canvas(song_view* sv, uint32_t owner);
void song_view_set_transport(song_view* sv, const song_transport* t);
void song_view_loaded(song_view* sv);
void song_view_copy_large(song_view* sv, screen* s, grid* g, uint32_t big_rows);
int song_view_is(const view* v);
const view* song_view_next(const view* top);
void song_view_play(song_view* sv, int resume);
void song_view_rec(song_view* sv, int shows);
#define SONG_VIEW_REC_OFF 0xffu
int song_view_rec_playing(const song_view* sv);
int song_view_recording(const song_view* sv);
uint32_t song_view_quantise(uint32_t quant, uint32_t elapsed_q8, uint32_t row_q8);
int song_view_record_note(song_view* sv, screen* s, uint32_t note, uint32_t instr, int keyoff);
int song_view_record_knob(song_view* sv, screen* s, uint32_t knob, int32_t steps);
uint32_t song_view_rec_value(const song_view* sv, uint32_t knob);
void song_view_rec_drain(song_view* sv);
void song_view_glo(song_view* sv, uint32_t toggles);
uint32_t song_view_mute_mask(const song_view* sv);
uint32_t song_view_keys(song_view* sv, uint32_t down, uint32_t pressed, uint32_t owned,
                        int take, screen* recording_screen, screen* live_screen);
void song_view_frame(song_view* sv, int shows);
int song_view_playing(const song_view* sv);
#define SONG_VIEW_NO_BPM 0xFFFFu
uint32_t song_view_shown_bpm(const song_view* sv);
uint32_t song_view_bpm10(uint32_t speed_sum, uint32_t multispeed, uint32_t ntsc,
                         uint32_t rows_beat);
void song_view_position(const song_view* sv, uint32_t v, uint32_t* entry, uint32_t* row);
void song_view_copy(song_view* sv, screen* s, grid* g);
void song_view_away(song_view* sv, int on);
int song_view_away_changed(const song_view* sv);
void song_view_away_compose(song_view* sv, screen* s, grid* g);
int song_view_away_input(screen* s, const screen_event* e);
uint32_t song_view_mutes(const song_view* sv);
void song_view_instr_text(const song_view* sv, uint32_t instr, char out[SCREEN_POPUP_TEXT]);

#ifdef __cplusplus
}
#endif

#endif
