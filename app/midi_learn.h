// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI learn: CCs bound to parameters at run time, over the published map (midi_map.h), and the
 * state machine that binds them (docs/reference/controls.md, "MIDI learn").
 *
 * The gesture (app/learn_view.h draws it): the learn button arms it (KNOB), a knob turn picks
 * the parameter the knob controls (CC), and the next learnable CC from either port binds to
 * it (back to IDLE, LEARNED). The button again before a knob asks to clear every binding
 * (CONFIRM), a third time clears them (CLEARED); the button after the knob instead of a CC
 * forgets that parameter's binding (FORGOT). HOME cancels (CANCELLED), and so does
 * MIDI_LEARN_TIMEOUT_MS without a step (TIMED_OUT). Each step restarts the timeout.
 *
 * The bindings:
 * - omni, like the map: a learned CC means the same on every channel and on TRS and USB;
 * - added to the map: the parameter's own CC still works;
 * - a learned CC wins over the map: if it was another parameter's CC, it now drives the
 *   learned parameter only (that one keeps its NRPN, and gets its CC back when the binding
 *   goes);
 * - one CC per parameter and one parameter per CC: learning a parameter again moves it to the
 *   new CC, and learning a bound CC for another parameter moves the CC;
 * - the value goes through the parameter's curve and range from its map row, as its own CC's;
 * - in RAM: lost at power-off until M2 stores settings.
 * Learnable CCs are 0..119 but data entry, increment and decrement, NRPN and RPN (6, 38,
 * 96..101): midi_map_cc takes those before any binding. 120..127 are channel mode messages.
 *
 * Every lookup is O(1): a CC's binding is one byte of cc_row (its map row), a parameter's one
 * byte of param_cc.
 *
 * Portable C with no hardware access and no allocation; the main loop owns it (MIDI input,
 * the panel's routing and MIDI out all run there). */
#ifndef SIDOU_MIDI_LEARN_H
#define SIDOU_MIDI_LEARN_H

#include <stdint.h>

#include "live.h"
#include "midi_map.h"

#ifdef __cplusplus
extern "C" {
#endif

/* An armed learn with no step for this long is cancelled. */
#define MIDI_LEARN_TIMEOUT_MS 10000u
/* cc_row: no binding. */
#define MIDI_LEARN_NONE 0xffu

typedef enum {
    MIDI_LEARN_IDLE = 0,
    MIDI_LEARN_KNOB = 1,   /* armed: a knob picks the parameter */
    MIDI_LEARN_CC = 2,     /* param picked: the next learnable CC binds to it */
    MIDI_LEARN_CONFIRM = 3 /* the button twice: a third press clears every binding */
} midi_learn_state;

/* How the last learn ended (midi_learn_take). */
typedef enum {
    MIDI_LEARN_NOTHING = 0,
    MIDI_LEARN_LEARNED = 1,   /* result_cc now drives param */
    MIDI_LEARN_FORGOT = 2,    /* param's binding (result_cc, MIDI_MAP_NO_CC if none) is gone */
    MIDI_LEARN_CLEARED = 3,   /* every binding is gone */
    MIDI_LEARN_CANCELLED = 4, /* HOME */
    MIDI_LEARN_TIMED_OUT = 5
} midi_learn_result;

typedef struct midi_learn {
    uint8_t cc_row[128];                /* each CC's bound map row (midi_map_row), or NONE */
    uint8_t param_cc[LIVE_PARAM_COUNT]; /* each parameter's bound CC, or MIDI_MAP_NO_CC */
    uint8_t count;                      /* bindings */
    uint8_t state;                      /* a midi_learn_state */
    uint8_t param;                      /* MIDI_LEARN_CC: the parameter picked */
    uint8_t result;                     /* a midi_learn_result, until midi_learn_take */
    uint8_t result_cc;
    uint32_t since; /* ms of the last step, for the timeout */
} midi_learn;

/* No bindings, idle. */
void midi_learn_init(midi_learn* ml);
/* 1 if cc can be learned (above). */
int midi_learn_learnable(uint32_t cc);
/* 1 if param can be learned: it has a row in the map that is not reserved. */
int midi_learn_param(uint32_t param);

/* The gesture. now: ms, for the timeout. */
/* The learn button: IDLE arms (KNOB), KNOB asks to clear all (CONFIRM), CONFIRM clears every
 * binding (CLEARED), CC forgets the picked parameter's binding (FORGOT). */
void midi_learn_button(midi_learn* ml, uint32_t now);
/* A knob turned while armed (KNOB, CC or CONFIRM): param is picked (CC). 1 if taken; 0 when
 * idle or param cannot be learned (the state stays). */
int midi_learn_knob(midi_learn* ml, uint32_t param, uint32_t now);
/* HOME: an armed learn ends, CANCELLED. */
void midi_learn_cancel(midi_learn* ml);
/* An armed learn with no step since MIDI_LEARN_TIMEOUT_MS ends, TIMED_OUT. 1 if it did. */
int midi_learn_tick(midi_learn* ml, uint32_t now);
/* The result of the learn that ended last, and nothing after it. */
midi_learn_result midi_learn_take(midi_learn* ml);

/* MIDI input, from midi_map_cc for every CC it does not take itself: while a parameter waits
 * (MIDI_LEARN_CC), a learnable cc binds to it (LEARNED). Then cc's bound row, or NULL. */
const midi_map_entry* midi_learn_route(midi_learn* ml, uint8_t cc);
/* cc's bound row, or NULL (no learning). */
const midi_map_entry* midi_learn_for_cc(const midi_learn* ml, uint32_t cc);
/* 1 if midi_learn_route would give cc a row: bound, or bound by it now. */
int midi_learn_takes(const midi_learn* ml, uint32_t cc);
/* param's bound CC, or MIDI_MAP_NO_CC. */
uint8_t midi_learn_cc_of(const midi_learn* ml, uint32_t param);

/* The bindings directly, as the gesture's ends do. cc learnable, param learnable, else 0. */
int midi_learn_bind(midi_learn* ml, uint32_t cc, uint32_t param);
void midi_learn_forget(midi_learn* ml, uint32_t param);
void midi_learn_clear(midi_learn* ml);

#ifdef __cplusplus
}
#endif

#endif
