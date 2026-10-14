// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI input for the live instrument (M1a): ring entries (midi_ring.h) through a parser per
 * source (midi_in.h) onto live, with the default mapping:
 * - notes on every channel (omni) play the instrument; the octave buttons do not apply;
 * - CC 74 cutoff (0..127 scaled to 0..2047), CC 71 resonance, CC 1 pulse width, CC 7 volume,
 *   CC 120 and 123 release every note (live_cc);
 * - program change n selects preset n; programs past the last preset are ignored;
 * - everything else (pitch bend, pressure, real time, system common, SysEx) is ignored.
 * The full CC map and MIDI learn come in M1d.
 *
 * One parser per source, because the sources interleave: a TRS message arrives a byte per
 * 320 us, so USB messages can come between its bytes, and a parser shared by both would take
 * a USB status byte as the end of the TRS message. USB packets go through midi_usb_bytes
 * first, so both sources share the one parser implementation with its running status,
 * real-time and SysEx rules.
 *
 * Portable C with no hardware access; the main loop owns it. */
#ifndef SIDOU_MIDI_RX_H
#define SIDOU_MIDI_RX_H

#include <stdint.h>

#include "live.h"
#include "midi_in.h"
#include "midi_ring.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    live* l;
    midi_in trs;
    midi_in usb;
    uint32_t lost_seen; /* midi_ring_lost when last checked */
    uint32_t restarts;  /* times lost input restarted the parsers */
} midi_rx;

/* lost: the ring's midi_ring_lost now, so input lost before this is not held against it. */
void midi_rx_init(midi_rx* rx, live* l, uint32_t lost);
/* One entry, through its source's parser; entries of an unknown source are ignored. */
void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e);
/* lost: the ring's midi_ring_lost. If it moved, input was dropped and a note-off may be among
 * it: both parsers restart (a message cut short is not completed with later bytes) and every
 * note is released. Call before taking the entries that came after the loss. Returns 1 if it
 * restarted. */
int midi_rx_check_lost(midi_rx* rx, uint32_t lost);
/* The mapping alone: one parsed message onto live. */
void midi_rx_live(live* l, const midi_msg* msg);

#ifdef __cplusplus
}
#endif

#endif
