// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI out: the instrument's parameter changes as USB-MIDI event packets, so a DAW can record
 * the panel as automation (docs/reference/midi.md, "Output").
 *
 * Diff-based: midi_out keeps every parameter's value as it last saw it (live_param_get, so the
 * chip model is the one asked for: the FX button's change shows at once) and compares.
 * - midi_out_local takes the changes since the last look as the panel's (anything but MIDI
 *   input): they are pending. midi_out_remote takes them as MIDI input's: the sender has the
 *   value already, so they are never echoed, on whichever port they came in (TRS or USB), and
 *   a pending panel change of the same parameter is dropped (the last change was MIDI's). The
 *   caller looks locally before taking MIDI input and remotely right after it.
 * - midi_out_tick, once per 10 ms tick, takes a local look, then sends what is pending with
 *   each parameter's value at that moment: at most one message per parameter per tick, the
 *   latest value winning, at most MIDI_OUT_TICK_PACKETS packets a tick, round robin over the
 *   parameters so none starves; the mod target first, since it says what CC 1 is (the pulse
 *   width or the wheel), and nothing before it while it waits for room. What does not fit (the budget, or the room the caller has)
 *   stays pending, one flag per parameter: nothing queues beyond that.
 * - A value that encodes as the one last sent (or received) sends nothing: a pulse-width
 *   move within one CC step, or a knob turned there and back within a tick.
 * - A preset change from the panel is a program change, sent before any parameter; the
 *   parameters it loads (live_set_preset: not the volume, the chip model, the effect slots
 *   or a bend range set by RPN 0) are not sent, the program change carries them, and their
 *   pending changes are dropped. From MIDI, it is the sender's, like any other change. The
 *   parameters it keeps are diffed as usual, so their pending changes still go. A known
 *   limit of diffing: a parameter the preset loads that the panel also changed in the same
 *   look counts as loaded and is not sent.
 *
 * Encoding (the map, midi_map.h): a parameter's CC, value midi_map_to_cc. A parameter of more
 * than 128 values (cutoff, pulse width, volume) goes as its NRPN instead, at full resolution,
 * when the high-resolution option is on. A parameter with no CC (the bend range, the
 * expression targets) always goes as its NRPN, and so does the pulse width while the mod wheel
 * drives another target (CC 1 is then the wheel's). An NRPN is four CCs: 99 the MSB and 98 the
 * LSB of its number, 6 the value's MSB and 38 its LSB (value >> 7, value & 127), the order
 * midi_map_cc applies at full resolution and, for a small parameter, at the LSB. No null NRPN
 * follows.
 *
 * Packets (midi_usb.h): the 32-bit word read little-endian, CIN in bits 0..3, cable
 * MIDI_OUT_CABLE in bits 4..7, the status and data bytes above.
 *
 * Portable C with no hardware access; the main loop owns it. */
#ifndef SIDOU_MIDI_OUT_H
#define SIDOU_MIDI_OUT_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The one cable of the USB-MIDI function (platform/fm1/usb/usb.c: one embedded jack each way). */
#define MIDI_OUT_CABLE 0u
/* Packets midi_out_tick writes at most: four NRPNs, or sixteen CCs. */
#define MIDI_OUT_TICK_PACKETS 16u
/* Packets per message: a CC or program change is one, an NRPN four. */
#define MIDI_OUT_NRPN_PACKETS 4u

typedef struct {
    uint16_t seen[LIVE_PARAM_COUNT]; /* each parameter's value at the last look */
    uint16_t wire[LIVE_PARAM_COUNT]; /* its value as last sent, or received from MIDI */
    uint8_t pending[LIVE_PARAM_COUNT];
    uint8_t preset_seen;
    uint8_t preset_pending; /* a program change to send */
    uint8_t channel;        /* 0..15 */
    uint8_t hires;          /* 1: NRPN for the parameters of more than 128 values */
    uint8_t next;           /* where the next tick's round robin starts */
    uint32_t packets;       /* packets written */
    uint32_t deferred;      /* ticks that left something pending for lack of room or budget */
    uint32_t dropped;       /* pending changes midi_out_drop forgot */
} midi_out;

/* Nothing pending: the instrument's state now is what the other side is taken to have.
 * channel 0..15 (masked), hires 0 or 1. */
void midi_out_init(midi_out* o, const live* l, uint8_t channel, uint8_t hires);
/* The changes since the last look are the panel's: pending. */
void midi_out_local(midi_out* o, const live* l);
/* The changes since the last look came from MIDI input: never echoed. */
void midi_out_remote(midi_out* o, const live* l);
/* Once per tick: a local look, then up to min(room, MIDI_OUT_TICK_PACKETS) packets into out,
 * whole messages only. Returns how many. */
uint32_t midi_out_tick(midi_out* o, const live* l, uint32_t room,
                       uint32_t out[MIDI_OUT_TICK_PACKETS]);
/* No host to send to: a local look, then everything pending is forgotten (counted in
 * dropped), so a host that comes later gets no stale changes. */
void midi_out_drop(midi_out* o, const live* l);

#ifdef __cplusplus
}
#endif

#endif
