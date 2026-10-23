// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI input for the live instrument: ring entries (midi_ring.h) through a parser per
 * source (midi_in.h) onto live, with the mapping:
 * - notes on every channel (omni) play the instrument, or on the channels MIDI's mode gives it
 *   (midi_rx_route); the octave buttons do not apply;
 * - CCs and NRPNs through the published map (midi_map.h, docs/reference/midi.md), with the
 *   NRPN state per channel and per source; CC 120 and 123 release every note;
 * - program change n selects preset n; programs past the last preset are ignored (with MIDI's
 *   mode, midi_rx_route: what the channel's bank select says, app/midi_route.h);
 * - pitch bend, channel and polyphonic aftertouch go to live (live_bend, live_pressure,
 *   live_poly_pressure), which applies them at its next tick;
 * - the MIDI clock and start (real time) go to the chord arpeggio (live_arp_realtime), and with
 *   midi_rx_sync to the clock follower;
 * - everything else (the rest of real time, system common, SysEx) is ignored.
 *
 * One parser per source, because the sources interleave: a TRS message arrives a byte per
 * 320 us, so USB messages can come between its bytes, and a parser shared by both would take
 * a USB status byte as the end of the TRS message. USB packets go through midi_usb_bytes
 * first, so both sources share the one parser implementation with its running status,
 * real-time and SysEx rules.
 *
 * Lost input (midi_ring.h): the parsers of the sources it lost restart, every note is released
 * and the controllers reset (bend centred, wheel and pressure 0) exactly where the gap is, so
 * what came before it is taken whole, what was cut by it is dropped (a message is never
 * completed with bytes from after the gap), and a note-off or a bend back to the centre that
 * may be among the lost cannot leave a note hanging or bent. The other source's parser goes on,
 * so a SysEx frame on it is not cut by the gap. midi_rx_entry finds a gap from the epochs of the
 * entries; midi_rx_check_lost finds one at the end of the ring (no entry after it yet).
 *
 * SysEx: each parser's on_sysex (midi_in.h) is the platform's to set after midi_rx_init (NULL
 * skips SysEx); a restart keeps it.
 *
 * Portable C with no hardware access; the main loop owns it. */
#ifndef SIDOU_MIDI_RX_H
#define SIDOU_MIDI_RX_H

#include <stdint.h>

#include "live.h"
#include "midi_in.h"
#include "midi_learn.h"
#include "midi_map.h"
#include "midi_out.h"
#include "midi_ring.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    live* l;
    /* Hot per-pass flags stay directly addressable, ahead of the large port maps. */
    uint8_t epoch;     /* the ring's epoch the parsers are in */
    uint8_t looked;    /* midi_out_local ran this pass: midi_rx_settle owes midi_out_remote */
    midi_in trs;
    midi_in usb;
    midi_map_port trs_map; /* each source's NRPN state */
    midi_map_port usb_map;
    uint32_t restarts; /* times lost input restarted the parsers */
    midi_out* out;     /* told which changes are MIDI's (midi_rx_watch), or NULL */
    midi_learn* learn; /* MIDI learn's bindings, both ports' (midi_rx_learn), or NULL */
    /* Messages taken other than real-time ones (F8 clock, FA-FC transport, FE active sensing):
     * the store's quiet detector watches it (platform/fm1/store_fm1.c), so a DAW's steady
     * clock and sensing do not count as playing. */
    uint32_t inputs;
    struct midi_route* route; /* MIDI's mode (midi_rx_route), or NULL: OMNI, as in M1 */
    const uint8_t* fast; /* the router's per-channel fast kinds, or the no-router defaults */
    struct midi_sync* sync;   /* clock and transport (midi_rx_sync), or NULL: ignored */
    const midi_ring_entry* entry; /* the entry being parsed (its stamp) */
} midi_rx;

/* epoch: the ring's midi_ring_epoch now, so losses before this are not held against it.
 * No midi_out watches, no MIDI learn. */
void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch);
/* MIDI out learns which changes are MIDI input's, so it never echoes them. Once a pass, not
 * once a message: before the first message of a pass that can change a parameter (a program
 * change, data entry, a CC the map gives a parameter, CC 1 while it is the pulse width's, or
 * a CC MIDI learn takes) the parsers call midi_out_local, and midi_rx_settle, at the end of
 * the pass, calls midi_out_remote. That needs nothing but MIDI input to change a parameter
 * between the two (the caller's to keep): every change in the pass is taken as MIDI's. So a
 * burst costs MIDI out two looks however long it is, and notes, bend and aftertouch cost it
 * nothing. out may be NULL: none. */
void midi_rx_watch(midi_rx* rx, midi_out* out);
/* Both ports' CCs go through MIDI learn's bindings (midi_learn.h; midi_map_port), and a
 * parameter waiting to be learned takes the next learnable CC from either. A restart after
 * lost input keeps them. learn may be NULL: none. */
void midi_rx_learn(midi_rx* rx, midi_learn* learn);
/* Both ports' messages through MIDI's mode (app/midi_route.h): the instrument's channel to
 * live as above, chip A's channels to the router, the rest dropped; a restart after lost input
 * releases the router's notes too. route may be NULL: every channel plays the instrument. */
void midi_rx_route(midi_rx* rx, struct midi_route* route);
/* Real time, song position and song select from both ports to the clock follower and the
 * transport (app/midi_sync.h), each with its entry's stamp. sync may be NULL: none. */
void midi_rx_sync(midi_rx* rx, struct midi_sync* sync);
/* One entry, through its source's parser; entries of an unknown source are ignored. An entry
 * from a later epoch than the parsers' first restarts them (a gap lies before it). */
void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e);
/* The end of a pass of entries: MIDI out takes the changes since the pass's first edit as
 * MIDI's (midi_rx_watch). Call it before anything but MIDI input can change a parameter
 * again, and before midi_out_tick. Nothing to do if no message of the pass could edit. */
void midi_rx_settle(midi_rx* rx);
/* epoch: the ring's midi_ring_epoch, read once the consumer has taken every entry (the ring
 * was empty). If it is ahead of the parsers' (by 1..127, wrapping), input was lost after the
 * last entry: restart now. An epoch that is not ahead changes nothing: an entry taken after
 * the caller read it may already have moved the parsers on. (With entries left in the ring,
 * the next one's epoch says it instead.) Returns 1 if it restarted. `sources`: the ring's
 * in_gap, read after the epoch (0, unknown: both).
 * A restart also forgets the NRPN selections of the sources it lost: the gap may have changed
 * them. */
int midi_rx_check_lost(midi_rx* rx, uint8_t epoch, uint8_t sources);
/* The mapping alone: one parsed message onto the port's live, with its NRPN state. */
void midi_rx_live(midi_map_port* port, const midi_msg* msg);

#ifdef __cplusplus
}
#endif

#endif
