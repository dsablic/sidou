// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LAB's undo (docs/plans/2026-10-25-lab-design.md "EDIT + LFO"): a ring of steps, each the bytes
 * of the sound (lab_sound.h's image, and the envelope mode after it: LAB's ENVELOPE choice) that
 * the step changed, with their values before it. A step is recorded from the sound as it was
 * before a change and as it is after (lab_edit_push), so whatever made the change (a card, the
 * choice, a revert, a roll, A becoming the edit) is undone the same way: the old bytes written
 * back and the sound applied through its setters (live_sound_apply).
 *
 * The ring holds LAB_EDIT_STEPS steps in LAB_EDIT_ENTRIES entries of (the byte's place, its old
 * value); the place's top bit marks a step's first entry. A step never takes more entries than
 * the image has bytes, so any one step fits; a new step that does not fit drops the oldest whole
 * steps. A run (lab_edit_push's merge) adds to the newest step only the bytes it does not hold
 * yet, so the step keeps the values from before the run's first change.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LAB_EDIT_H
#define SIDOU_LAB_EDIT_H

#include <stdint.h>

#include "lab_sound.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The steps undo keeps, and the entries they share. */
#define LAB_EDIT_STEPS 16u
#define LAB_EDIT_ENTRIES 48u
/* What a step records: the sound's bytes, then the envelope mode. */
#define LAB_EDIT_MODE LAB_SOUND_BYTES
#define LAB_EDIT_BYTES (LAB_SOUND_BYTES + 1u)
/* lab_edit.at: a step's first entry. */
#define LAB_EDIT_FIRST 0x80u

_Static_assert(LAB_EDIT_BYTES <= LAB_EDIT_ENTRIES && LAB_EDIT_BYTES < LAB_EDIT_FIRST,
               "any one step fits the ring, and a place fits beside its mark");

typedef struct {
    uint8_t at[LAB_EDIT_ENTRIES];  /* a byte's place in the image, LAB_EDIT_FIRST on a step's first */
    uint8_t old[LAB_EDIT_ENTRIES]; /* its value before the step */
    uint8_t first;                 /* the oldest entry's index */
    uint8_t n;                     /* the entries held */
    uint8_t steps;                 /* the steps held */
} lab_edit;

/* No steps. */
void lab_edit_clear(lab_edit* e);
/* The sound as a step sees it into out: live_sound_get's image, the envelope mode after it. */
void lab_edit_get(const live* l, uint8_t out[LAB_EDIT_BYTES]);
/* The change from before to after as a step: the bytes that differ, before's values. With merge,
 * into the newest step (a run), the bytes it already holds left as they are. 1 when the newest
 * step is this change's (it changed something, or merged into a step), 0 when it changed nothing
 * and made no step. */
uint32_t lab_edit_push(lab_edit* e, const uint8_t before[LAB_EDIT_BYTES],
                       const uint8_t after[LAB_EDIT_BYTES], int merge);
/* The newest step undone on l (its old bytes over the sound as it is, applied through the
 * setters); 1, or 0 when there is none. */
uint32_t lab_edit_undo(lab_edit* e, live* l);

#ifdef __cplusplus
}
#endif

#endif
