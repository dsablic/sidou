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
/* Silent, as LAB's rolls mean it: a held C4 at the panel's velocity, this long from its note-on
 * (the chip's envelopes at 0 before it, as a played instrument has them), whose output on the
 * chip alone, through a DC blocker (a first-order high-pass at about 35 Hz: the chip's DC level
 * moves with the envelope, a thump that is not the note), has an RMS under LAB_SILENT_DB dBFS. */
#define LAB_SILENT_MS 500u
#define LAB_SILENT_DB (-40)

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
/* 1 when sound s would be heard on a chip of model (a sid_model): not silent as LAB_SILENT_MS
 * and LAB_SILENT_DB mean it, with room to spare. An estimate in whole dB, each term the worst of
 * the chip's levels the host tests measured (host/crates/sidou-tests/tests/app/lab_edit.rs):
 * - the waveform's level on the model: none, noise with another waveform, the 6581's
 *   combinations and the 8580's with the pulse refused (they fall away from a 50 % pulse);
 * - the pulse's narrowness, the PWM taking half its swing off (a side under 16 steps refused);
 * - what the envelope lets through in the window: the attack's ramp, the decay to the sustain
 *   level; an arpeggio 3 dB more (it plays the note again and again);
 * - SYNC LEAD's one-voice sync: 6 dB, more below a ratio of 1 (the voice never ends a cycle),
 *   and the note at the ratio's pitch;
 * - with a voice routed to the filter (the note may be on it): the mode's loss at the cutoff's
 *   distance from the note in half octaves, on that model's filter (routed to no mode:
 *   refused); a narrow pulse under a low-pass near the note counts its fundamental alone; a
 *   combination or the ring 6 dB more (their partials are not where the curves say).
 * The estimate must reach LAB_AUDIBLE_DB. The resonance's gain, the chip's profile, the effect
 * slots and the controllers are left out, each only louder or not the chip's. lab_edit.rs proves
 * it against renders: no sound it passes is silent. */
#define LAB_AUDIBLE_DB (-38)
uint32_t lab_audible(const uint8_t s[LAB_SOUND_BYTES], uint32_t model);
/* A sound's 16-bit hash (FNV-1a folded): the rolls' memory of the last result. */
uint32_t lab_edit_hash(const uint8_t s[LAB_SOUND_BYTES]);
/* The newest step undone on l (its old bytes over the sound as it is, applied through the
 * setters); 1, or 0 when there is none. */
uint32_t lab_edit_undo(lab_edit* e, live* l);

#ifdef __cplusplus
}
#endif

#endif
