// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_GEN_H
#define SIDOU_SONG_GEN_H

#include <stdint.h>

#include "screen.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

struct song_view;
struct song_pages;

enum { SONG_GEN_MUTATE = 1, SONG_GEN_DICE, SONG_GEN_SWEEP };
#define SONG_GEN_LANES 3u
#define SONG_GEN_FRAME_ROWS 12u
#define SONG_GEN_LOOK 6u
#define SONG_GEN_ENC_ROLE 6u
enum { SONG_GEN_MAJ, SONG_GEN_MIN, SONG_GEN_SUS4, SONG_GEN_7, SONG_GEN_CHORDS };
#define SONG_GEN_CHORD_ROWS 5u
#define SONG_GEN_SWEEP_EVERY 2u
#define SONG_GEN_SWEEP_LOW 0x10u
#define SONG_GEN_SWEEP_HIGH 0xF0u

typedef struct {
    uint8_t root[SONG_GEN_LANES];
    const uint8_t* gate;
    uint8_t scl_down, scl_spoiled;
    screen_lone lone;
    uint8_t run, pattern, rows, voice, row, instr, legato, drums;
    uint8_t dens, scale, rhythm, range, up;
    uint8_t follow, follow_row;
    uint8_t follow_cell[4];
    uint32_t seed;
    uint8_t role, kick, snare, wait;
    uint8_t chord[SONG_GEN_CHORDS];
    struct song_pages* pages;
} song_gen;

void song_gen_init(song_gen* g, const uint8_t* gate);

void song_gen_dice(const struct song_view* sv, song_dice* d);
int song_gen_drums(const struct song_view* sv, uint32_t voice);
uint32_t song_gen_role(const struct song_view* sv, uint32_t voice);
uint32_t song_gen_chord_rows(uint32_t c, uint32_t first, uint8_t left[SONG_GEN_CHORD_ROWS],
                             uint8_t right[SONG_GEN_CHORD_ROWS]);
uint32_t song_gen_chord_at(const gt2_song* s, uint32_t ptr);
uint32_t song_gen_chord_find(const gt2_song* s, uint32_t c);
const char* song_gen_chord_name(uint32_t c);
uint32_t song_gen_note(const song_gen* g, uint32_t h);
int song_gen_hit(const song_gen* g, uint32_t row);

void song_gen_start(struct song_view* sv, screen* s, uint32_t kind, uint32_t pattern,
                    uint32_t voice, uint32_t instr);
int song_gen_busy(const struct song_view* sv);
void song_gen_drain(struct song_view* sv, screen* s);
void song_gen_stop(struct song_view* sv);

int song_gen_press(struct song_view* sv, uint32_t id, int edit, int free);
int song_gen_release(struct song_view* sv, uint32_t held);
uint32_t song_gen_lone_chord(struct song_view* sv, const screen_event* e);
int song_gen_turn(struct song_view* sv, screen* s, uint32_t enc, int32_t steps);
int song_gen_popup(const struct song_view* sv, uint32_t enc, char name[SCREEN_POPUP_TEXT],
                   char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max);
uint32_t song_gen_keys(struct song_view* sv, uint32_t down, uint32_t pressed);
void song_gen_frame(struct song_view* sv, uint32_t top);
int song_gen_look(const struct song_view* sv, uint8_t out[SONG_GEN_LOOK]);

#ifdef __cplusplus
}
#endif

#endif
