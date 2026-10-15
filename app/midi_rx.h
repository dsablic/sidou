// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI input for the live instrument: ring entries (midi_ring.h) through a parser per
 * source (midi_in.h) onto live, with the mapping:
 * - notes on every channel (omni) play the instrument; the octave buttons do not apply;
 * - CCs and NRPNs through the published map (midi_map.h, docs/reference/midi.md), with the
 *   NRPN state per channel and per source; CC 120 and 123 release every note;
 * - program change n selects preset n; programs past the last preset are ignored;
 * - everything else (pitch bend, pressure, real time, system common, SysEx) is ignored.
 *
 * One parser per source, because the sources interleave: a TRS message arrives a byte per
 * 320 us, so USB messages can come between its bytes, and a parser shared by both would take
 * a USB status byte as the end of the TRS message. USB packets go through midi_usb_bytes
 * first, so both sources share the one parser implementation with its running status,
 * real-time and SysEx rules.
 *
 * Lost input (midi_ring.h): the parsers restart and every note is released exactly where the
 * gap is, so what came before it is taken whole, what was cut by it is dropped (a message is
 * never completed with bytes from after the gap), and a note-off that may be among the lost
 * cannot leave a note hanging. midi_rx_entry finds a gap from the epochs of the entries;
 * midi_rx_check_lost finds one at the end of the ring (no entry after it yet).
 *
 * Portable C with no hardware access; the main loop owns it. */
#ifndef SIDOU_MIDI_RX_H
#define SIDOU_MIDI_RX_H

#include <stdint.h>

#include "live.h"
#include "midi_in.h"
#include "midi_map.h"
#include "midi_ring.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    live* l;
    midi_in trs;
    midi_in usb;
    midi_map_port trs_map; /* each source's NRPN state */
    midi_map_port usb_map;
    uint8_t epoch;     /* the ring's epoch the parsers are in */
    uint32_t restarts; /* times lost input restarted the parsers */
} midi_rx;

/* epoch: the ring's midi_ring_epoch now, so losses before this are not held against it. */
void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch);
/* One entry, through its source's parser; entries of an unknown source are ignored. An entry
 * from a later epoch than the parsers' first restarts them (a gap lies before it). */
void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e);
/* epoch: the ring's midi_ring_epoch, read once the consumer has taken every entry (the ring
 * was empty). If it is ahead of the parsers' (by 1..127, wrapping), input was lost after the
 * last entry: restart now. An epoch that is not ahead changes nothing: an entry taken after
 * the caller read it may already have moved the parsers on. (With entries left in the ring,
 * the next one's epoch says it instead.) Returns 1 if it restarted.
 * A restart also forgets every NRPN selection: the gap may have changed it. */
int midi_rx_check_lost(midi_rx* rx, uint8_t epoch);
/* The mapping alone: one parsed message onto the port's live, with its NRPN state. */
void midi_rx_live(midi_map_port* port, const midi_msg* msg);

#ifdef __cplusplus
}
#endif

#endif
