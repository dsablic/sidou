// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The published CC/NRPN map: one table row per parameter, with its CC (7-bit) and NRPN
 * (14-bit), range, curve and default. docs/reference/midi.md is generated from this table
 * by the host tests (host/crates/sidou-tests/tests/app/midi_map.rs), which also fail when the
 * two disagree.
 *
 * - A CC sets the parameter through its curve (midi_map_from_cc): LINEAR spans the range,
 *   value * max / 127 rounded; STEPS divides 0..127 evenly among the values,
 *   value * (max + 1) / 128.
 * - An NRPN carries the parameter's own value, MSB * 128 + LSB, clamped to its range: the
 *   full 11 bits of the cutoff and 12 of the pulse width. For a parameter of more than 128
 *   values (max >= 128) CC 6 applies the MSB at once (the LSB taken as 0) and CC 38 refines
 *   it. For a smaller one (max < 128: the value is all in the LSB) CC 6 only stores the MSB
 *   and CC 38 applies the value, so a sender's 6 = 0, 38 = v never passes through 0 (a chip
 *   model reset, a silent waveform, a cut envelope). CC 6 alone does not change such a
 *   parameter: send CC 38, or the plain CC. midi_map_cc keeps the selection (CC 99/98, RPN
 *   101/100) and data entry state (CC 6/38, increment 96, decrement 97) per channel, for one
 *   input (a midi_map_port each).
 * - Reserved rows (param MIDI_MAP_RESERVED) keep their numbers for parameters that do not
 *   exist yet and do nothing; there are none now (M1c Task 3 gave the character layer's
 *   theirs). A new parameter is a new row.
 *
 * Portable C with no hardware access; it runs in the main loop (flash on the device). */
#ifndef SIDOU_MIDI_MAP_H
#define SIDOU_MIDI_MAP_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_MAP_NO_CC 0xffu
#define MIDI_MAP_NO_NRPN 0xffffu
/* The param of a row whose numbers are kept for a parameter not implemented yet. */
#define MIDI_MAP_RESERVED 0xffu
#define MIDI_MAP_NRPN(msb, lsb) ((uint16_t)((msb) << 7 | (lsb)))
/* NRPN and RPN 127/127: no parameter selected. */
#define MIDI_MAP_NULL 127u

typedef enum {
    MIDI_CURVE_LINEAR = 0, /* value * max / 127, rounded: needs max >= 127 */
    MIDI_CURVE_STEPS = 1   /* value * (max + 1) / 128: needs max < 128 */
} midi_map_curve;

/* The reference's text is only for the host tests that generate docs/reference/midi.md:
 * sidou-sys defines SIDOU_MIDI_MAP_DOC, and the device build leaves the strings out. So the
 * layout differs between the host and the device: emulator tests must not read the device's
 * rows (midi_map.c) through sidou-sys's midi_map_entry. */
typedef struct {
#ifdef SIDOU_MIDI_MAP_DOC
    const char* group;  /* "Live", "SID", "Character", "Distortion", ... */
    const char* name;
    const char* values; /* what the values mean, for the reference */
#endif
    uint16_t nrpn;      /* MIDI_MAP_NRPN(msb, lsb), or MIDI_MAP_NO_NRPN */
    uint16_t max;       /* the range is 0..max */
    uint16_t def;       /* the power-on value (preset 0's where a preset sets it) */
    uint8_t param;      /* a live_param, or MIDI_MAP_RESERVED */
    uint8_t cc;         /* or MIDI_MAP_NO_CC */
    uint8_t curve;      /* a midi_map_curve, for the CC */
} midi_map_entry;

#define MIDI_MAP_ROWS 42u

/* Row i in table order, or NULL past the last (MIDI learn keeps its bindings as row indexes,
 * midi_learn.h). */
const midi_map_entry* midi_map_row(uint32_t i);
/* The row of a parameter, a CC or an NRPN (reserved rows included), or NULL. */
const midi_map_entry* midi_map_for_param(uint32_t param);
const midi_map_entry* midi_map_for_cc(uint8_t cc);
const midi_map_entry* midi_map_for_nrpn(uint16_t nrpn);
/* A CC value 0..127 as the parameter's value, by the row's curve. */
uint32_t midi_map_from_cc(const midi_map_entry* e, uint8_t value);
/* A parameter value as a CC value, round(value * 127 / max) (clamped): midi_map_from_cc
 * gives it back exactly for STEPS, and as near as 7 bits can for LINEAR. */
uint8_t midi_map_to_cc(const midi_map_entry* e, uint32_t value);

/* One channel's running NRPN/RPN state. */
typedef struct {
    uint8_t nrpn_msb; /* the parameter number last selected by CC 99 and 98 */
    uint8_t nrpn_lsb;
    uint8_t rpn_msb; /* by CC 101 and 100 */
    uint8_t rpn_lsb;
    uint8_t rpn;      /* 1 if an RPN was selected last: data entry is not the NRPN's */
    uint8_t data_msb; /* the last CC 6 for this selection, or MIDI_MAP_NO_DATA */
} midi_map_channel;

#define MIDI_MAP_NO_DATA 0xffu

struct midi_learn; /* midi_learn.h */

/* One input's CCs onto a live instrument. */
typedef struct {
    live* l;
    midi_map_channel ch[16];
    /* The bindings MIDI learn made (midi_learn.h), shared by the inputs, or NULL: none. */
    struct midi_learn* learn;
} midi_map_port;

/* Nothing selected on any channel; no learn. */
void midi_map_port_init(midi_map_port* p, live* l);
/* A CC on a channel (0..15): selection, data entry, increment and decrement by the channel's
 * state (RPN 0 the bend range, other RPNs nothing), CC 121 (reset all controllers) deselects
 * and resets live's controllers; everything else through MIDI learn first
 * (midi_learn_route: a CC bound to a parameter sets it through the parameter's row, and a
 * parameter waiting to be learned takes the CC), then live_cc. */
void midi_map_cc(midi_map_port* p, uint8_t channel, uint8_t cc, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
