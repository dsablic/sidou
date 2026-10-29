// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* STEP (M2d Task 5; docs/art/README.md "STEP"): the 16 leftmost note keys are 16 steps of the
 * selected voice's pattern, and a held step plus a knob is a parameter lock. Drawn to be read
 * at 23 mm in TRACKER's style (2x text for what is read, the theme's roles).
 *
 * - rows 0-1, the header (as TRACKER's): the voice "V1" at 2x in its colour, its pattern
 *   "PAT 03" at 2x, MUT/SOL, the play mark, the tempo and REC;
 * - row 2 the page selector (PAGE 1 2 3 4, the shown one reversed) and the pattern's length
 *   (64 STEPS); rows 3-4 the step ruler at 2x (the number of each group's first step);
 * - rows 5-16 three lanes of four rows: V1, V2, V3. A lane's
 *   16 steps stand in columns 5-23 in groups of four, each a pip (an empty step a dark square,
 *   a note a ball in the voice's colour, the playing step white) over a notch row: a yellow
 *   notch is a LOCK on that step, a cyan one a SET (the waveform command), the held step a
 *   yellow block. Under them, at 2x, the lane's default note, its instrument and its gate (STAC or
 *   LEG); an arrow marks the selected lane;
 * - rows 17-22 the HELD STEP frame: K1 CUTOFF, K2 RESONANCE, K3 PULSE WIDTH and K4 WAVEFORM
 *   with their values and bars, LOCK in yellow on a locked one and SET in cyan on the waveform
 *   command (what EDIT turns them into, the character, is the help card's);
 * - rows 24-28 the 16 keys as two rows of eight pads in beats of four, numbered by beat at 2x
 *   (1 2 3 4  1 2 3 4), the second and fourth beats shaded and each beat's first number
 *   brighter, the pad of a step with a note in the lane's colour; what the controls do is the
 *   help card's (app/help_view.h).
 *
 * Controls (docs/reference/controls.md "STEP"): a step key pressed and let go with no knob
 * between toggles a note on the selected lane at that step (the lane's default note and
 * instrument; off writes a rest); held with a knob turned it locks that knob's parameter on
 * that step. SELECT selects the lane, ALGORITHM pages (while the song plays the page follows
 * the playing step), PRESETS sets the lane's instrument or with a step held that step's note,
 * ARP switches the lane between staccato (a key off on the next empty row, the default) and
 * legato, LFO with a step held clears its locks and its waveform command. Held with a knob,
 * ALGORITHM is the step's chance to play (a lock; the player drops the note of a pass that
 * loses its roll). EDIT + ARP is MUTATE (a few small changes to the lane's pattern), EDIT +
 * SCL is DICE (a new pattern for the lane, notes of a scale on a rhythm grid): both are plain
 * cell writes through the editor.
 *
 * Two kinds, one meaning each: LOCK (K1 cutoff, K2 resonance, K3 pulse width; with EDIT held
 * K1-K4 the character, song_ext.h SONG_LOCK_*) holds for that row only and lives in the LOCK
 * chunk; SET (K4's waveform) is GoatTracker's own `7XY` command on that row, which holds until
 * the next waveform change, and is refused where the row's command column is taken.
 *
 * Every pattern change is an edit through the song's editor (app/song_edit.h): a note, a rest
 * and a key off are cell writes the player applies between blocks; a lock is the main loop's
 * write of the LOCK chunk (song_ctl resolves it at the row's start). A step that needs two
 * rows written (the note and the key off after it) sends the second once the first is applied.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_STEP_VIEW_H
#define SIDOU_STEP_VIEW_H

#include <stdint.h>

#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The steps a page holds: the 16 leftmost note keys. */
#define STEP_VIEW_STEPS 16u
/* No step held. */
#define STEP_VIEW_NONE 0xffu
/* The grid's rows: the page selector, the ruler, the lanes (three rows each), the HELD STEP
 * frame and the pads. */
#define STEP_VIEW_PAGE_ROW 2u
#define STEP_VIEW_RULER_ROW 3u
#define STEP_VIEW_LANES_ROW 5u
#define STEP_VIEW_LANE_ROWS 4u
#define STEP_VIEW_HELD_ROW 17u
#define STEP_VIEW_PADS_ROW 24u
/* The pads' first column: a pad is two cells and a cell apart, a beat two cells more. */
#define STEP_VIEW_PAD_COL 2u
/* A lane's first step column, and the lanes the view draws (V1-V3). */
#define STEP_VIEW_STEP_COL 5u
#define STEP_VIEW_LANES 3u
/* The bytes of what the view drew (three zones' looks), in step_view.drawn. */
#define STEP_VIEW_LOOK 52u

/* The waveform SET writes: its choices (0 is none: no command). */
#define STEP_VIEW_WAVES 5u

/* An action waiting for the editor (step_view.c): what, on which step, and the detents it
 * has gathered. */
typedef struct {
    uint8_t kind, voice, pattern, row;
    int16_t steps;
} step_act;
#define STEP_VIEW_QUEUE 4u

typedef struct {
    song_view* sv;
    uint8_t page;      /* the page ALGORITHM chose (the song plays: the playing step's) */
    uint8_t held;      /* the held step, STEP_VIEW_NONE: none */
    uint8_t gate;      /* bit v: voice v's lane is legato */
    uint8_t rev;       /* locks written: the view draws again */
    uint8_t zones;     /* the zones built into the canvas (step_view.c) */
    uint8_t note[3];   /* each lane's default note, 0..95 (0 is C-0) */
    uint8_t instr[3];  /* and its instrument */
    uint16_t down;     /* the step keys the view knows are down (bit k) */
    uint16_t touched;  /* those a knob was turned on: they do not toggle at their release */
    /* A second row of one step's two (the key off after a note), sent once the first is
     * applied. */
    uint8_t follow;
    uint8_t follow_pattern, follow_row;
    uint8_t follow_cell[4];
    uint8_t qn;
    step_act q[STEP_VIEW_QUEUE];
    /* MUTATE or DICE at work (step_view.c, "generative play"): the run's kind (0: none), where,
     * how far, whether the lane is the song's drums, and the seed its random is made from. */
    uint8_t run, gen_pattern, gen_voice, gen_row, gen_drums;
    /* DICE's density, scale, rhythm and range for the run (read from the song's DICE chunk as it
     * starts). */
    uint8_t gen_dens, gen_scale, gen_rhythm, gen_range;
    /* SCL is down with EDIT (STEP_VIEW_SCL_CHORD): its release rolls DICE unless K1 or K3 was
     * turned (or another button pressed) meanwhile; the song's density and scale show while it
     * is down. EDIT + SCL is the chord whichever lands first: SCL alone (STEP_VIEW_SCL_ALONE)
     * waits for EDIT, and selects the next effect slot as everywhere at its release, or at once
     * when another control is used first. */
    uint8_t scl_down, scl_spoiled;
    uint32_t seed;
    uint8_t drawn[STEP_VIEW_LOOK];
} step_view;

enum { STEP_VIEW_SCL_CHORD = 1, STEP_VIEW_SCL_ALONE = 2 };

extern const view step_view_view;

/* No step held, page 0, every lane at C-3 with instrument 1, staccato. */
void step_view_init(step_view* st, song_view* sv);
/* SCL's hold, once a pass while STEP shows (views_buttons, with the buttons held): its release
 * rolls DICE if it was not spoiled. */
void step_view_buttons(step_view* st, screen* s, uint32_t held);
/* The step keys, once a pass while STEP shows (views_note_keys): down are the keys down,
 * pressed those that went down since the last call, take 0 while GLO is held (its layer
 * takes the keys). A key that goes up with no knob turned on it toggles its step. Returns the
 * keys it took, for the panel to keep quiet (panel_quiet_keys); keys above the 16 steps are
 * the jam's (song_view_keys). */
uint32_t step_view_keys(step_view* st, screen* s, uint32_t down, uint32_t pressed, int take);
/* The page's steps for the LEDs (panel_led_in): set gets bit i for a note at step i of the
 * selected lane's page, rows the steps that exist (the last page of a pattern may be short, none
 * with no pattern), head the step the song plays on the page, STEP_VIEW_NONE if it plays
 * elsewhere or stopped. */
void step_view_leds(const step_view* st, uint32_t* set, uint32_t* rows, uint32_t* head);
/* The popup of an encoder (app/screen.h screen_popup_fn's text): PRESETS, ALGORITHM, and with
 * a step held the knobs. 0: nothing to show (the knob acts as in LIVE). */
int step_view_popup(const step_view* st, uint32_t encoder, int edit, char name[SCREEN_POPUP_TEXT],
                    char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max);

#ifdef __cplusplus
}
#endif

#endif
