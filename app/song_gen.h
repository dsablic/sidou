// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Generative play in the song views (STEP and TRACKER; docs/reference/controls.md "STEP",
 * "TRACKER"): MUTATE (EDIT + ARP), DICE (EDIT + SCL) and the filter sweep (EDIT + ENV) on one
 * lane's pattern, DICE's settings (the song's DICE chunk, core/song/song_ext.h) and the frame that
 * shows them while EDIT + SCL is held, and each lane's root.
 *
 * Each run is ordinary cell writes through the song's editor (app/song_edit.h), one a turn of
 * song_gen_drain, so each is an edit as any other: the pattern stays plain GT2 data and undo
 * steps back a cell at a time. The notes come from the lane's scale on its root (C-3 until
 * PRESETS with a step held in STEP, or a note key with EDIT + SCL held, sets it), over the
 * range's octaves, on the rhythm's grid scaled by the density; a lane the SCNE chunk calls
 * DRUMS keeps its one note (the root) and varies only where it hits. What "random" means is
 * song_mix of the seed and the row: the same seed makes the same pattern.
 *
 * EDIT + SCL is a chord whichever lands first: SCL alone waits for EDIT, and selects the next
 * effect slot (as everywhere) at its release or as soon as another control is used. While the
 * chord is held K1-K4 set the density, the range, the selected lane's scale and the rhythm, a
 * note key the lane's root; a knob turned or another button pressed spoils the roll at SCL's
 * release. EDIT + ARP is MUTATE whichever lands first too: a view holds ARP's lone action back
 * (song_gen.arp_alone) while EDIT may still arrive; TRACKER holds LFO's back the same way
 * (song_gen.lfo_alone), as EDIT + LFO is undo.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SONG_GEN_H
#define SIDOU_SONG_GEN_H

#include <stdint.h>

#include "screen.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

struct song_view;

/* A run's kind (song_gen.run; 0: none). */
enum { SONG_GEN_MUTATE = 1, SONG_GEN_DICE, SONG_GEN_SWEEP };
/* SCL's hold (song_gen.scl_down): with EDIT, or alone waiting for it. */
enum { SONG_GEN_SCL_CHORD = 1, SONG_GEN_SCL_ALONE = 2 };
/* The lanes, a root each. */
#define SONG_GEN_LANES 3u
/* The settings frame's rows (song_gen_frame), and the bytes of its look (song_gen_look). */
#define SONG_GEN_FRAME_ROWS 12u
#define SONG_GEN_LOOK 6u
/* The filter sweep: a C command every SONG_GEN_SWEEP_EVERY rows from SWEEP_LOW to SWEEP_HIGH or
 * back. */
#define SONG_GEN_SWEEP_EVERY 2u
#define SONG_GEN_SWEEP_LOW 0x10u
#define SONG_GEN_SWEEP_HIGH 0xF0u

typedef struct {
    uint8_t root[SONG_GEN_LANES]; /* each lane's note, 0..95 (C-3, 36, by default) */
    const uint8_t* gate;          /* STEP's lanes' gate (bit v legato), NULL: staccato */
    uint8_t scl_down, scl_spoiled;
    /* ARP pressed alone and still down, its lone action held back (STEP's gate, TRACKER's key
     * off with REC on): EDIT pressed now is MUTATE, whichever finger lands first. The view does
     * the lone action at ARP's release or as soon as another control is used. */
    uint8_t arp_alone;
    /* The same for LFO (TRACKER's rest with REC on): EDIT + LFO is undo, whichever lands first. */
    uint8_t lfo_alone;
    /* The run: its kind, its pattern and rows, the lane, the next row (or MUTATE's count), the
     * lane's instrument and gate, whether it is the song's drums, DICE's settings as it started,
     * the sweep's direction, the seed its random is made from. */
    uint8_t run, pattern, rows, voice, row, instr, legato, drums;
    uint8_t dens, scale, rhythm, range, up;
    /* The second row of a note MUTATE toggles (the key off after it), written at the next turn. */
    uint8_t follow, follow_row;
    uint8_t follow_cell[4];
    uint32_t seed;
} song_gen;

/* No run, no chord, every root C-3; gate: STEP's lanes' gate (NULL: staccato). */
void song_gen_init(song_gen* g, const uint8_t* gate);

/* The song's DICE settings, the defaults with no song or no chunk. */
void song_gen_dice(const struct song_view* sv, song_dice* d);
/* 1: lane `voice` is the song's drums (its SCNE chunk). */
int song_gen_drums(const struct song_view* sv, uint32_t voice);
/* The note of a random value h on the run's lane (song_gen_start's), and whether row hits. */
uint32_t song_gen_note(const song_gen* g, uint32_t h);
int song_gen_hit(const song_gen* g, uint32_t row);

/* A run (SONG_GEN_*) on `pattern` of lane `voice` with `instr`: a toast says which, and the first
 * write goes at once. Refused with no song, no editor or a run at work (WAITING). */
void song_gen_start(struct song_view* sv, screen* s, uint32_t kind, uint32_t pattern,
                    uint32_t voice, uint32_t instr);
/* 1 while a run or its last row waits. */
int song_gen_busy(const struct song_view* sv);
/* The run's next write, once the editor is free: one a call. */
void song_gen_drain(struct song_view* sv, screen* s);
/* The run and the chords stop (the view is left). */
void song_gen_stop(struct song_view* sv);

/* A view's button press: the chord's part. free: the view may roll now (no step held, not
 * recording, an editor). 1: taken. */
int song_gen_press(struct song_view* sv, uint32_t id, int edit, int free);
/* The buttons held, once a pass while SCL is down: 1 when the chord's release is a roll. */
int song_gen_release(struct song_view* sv, uint32_t held);
/* SCL that was down alone: the next effect slot, and no chord now. */
void song_gen_scl_alone(struct song_view* sv);
/* A view's knob turn: K1-K4 while the chord is held set DICE's settings. 1: taken. */
int song_gen_turn(struct song_view* sv, screen* s, uint32_t enc, int32_t steps);
/* The popup of K1-K4 while the chord is held (screen_popup_fn's text). 0: not the chord's. */
int song_gen_popup(const struct song_view* sv, uint32_t enc, char name[SCREEN_POPUP_TEXT],
                   char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max);
/* The note keys while the chord is held: a key pressed sets the selected lane's root (the last
 * one wins). Returns the keys it takes (every key down: none sounds or toggles), 0 with no
 * chord. */
uint32_t song_gen_keys(struct song_view* sv, uint32_t down, uint32_t pressed);
/* The settings frame, rows top..top + SONG_GEN_FRAME_ROWS - 1 of the song views' canvas. */
void song_gen_frame(struct song_view* sv, uint32_t top);
/* What the frame shows (out[0] 0: no chord, the rest 0), for a view's look. 1: the chord. */
int song_gen_look(const struct song_view* sv, uint8_t out[SONG_GEN_LOOK]);

#ifdef __cplusplus
}
#endif

#endif
