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
 *   caller looks locally before taking MIDI input and remotely right after it, once for a
 *   whole pass of input when nothing else changes a parameter meanwhile (midi_rx_watch).
 * - midi_out_tick, once per 10 ms tick, takes a local look, then sends what is pending with
 *   each parameter's value at that moment: at most one message per parameter per tick, the
 *   latest value winning, at most MIDI_OUT_TICK_PACKETS packets a tick, round robin over the
 *   parameters so none starves; the mod target first, since it says what CC 1 is (the pulse
 *   width or the wheel), and nothing before it while it waits for room. What does not fit (the budget, or the room the caller has)
 *   stays pending, one flag per parameter: nothing queues beyond that.
 *   Cold expert metadata/restoration halves that tick's packet budget.
 * - A value that encodes as the one last sent (or received) sends nothing: a pulse-width
 *   move within one CC step, or a knob turned there and back within a tick.
 * - Banks (M2e Task 4, app/midi_route.h): a preset's program change is bank 0's; when the last
 *   bank sent was another, CC 0 = 0 and CC 32 = 0 go before it. A song loaded from the SONG
 *   menu (midi_out_song) goes as CC 0 = 0, CC 32 = 1 and its program, before anything else.
 * - A preset change from the panel is a program change, sent before any parameter; the
 *   parameters it loads (live_set_preset: not the volume, the chip model, the effect slots
 *   or a bend range set by RPN 0) go out as the preset's factory values, which the program
 *   change carries (live_preset_params); the parameters of the edits PRESETS brought back
 *   (live_recall_preset) or a knob moved in the same look differ from them and follow the
 *   program change as CCs, in the parameters' order. From MIDI, it is the sender's, like any
 *   other change. The parameters it keeps are diffed as usual, so their pending changes still
 *   go.
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
 * MIDI learn (midi_learn.h, midi_out_learn): a parameter bound to a CC goes out on that CC,
 * whatever the above says (the CC the player's controller sends is the one a DAW should
 * record, and it plays back onto the same parameter); and a parameter whose own CC another
 * parameter learned goes as its NRPN, so what it sends never lands on the other one.
 *
 * Packets (midi_usb.h): the 32-bit word read little-endian, CIN in bits 0..3, cable
 * MIDI_OUT_CABLE in bits 4..7, the status and data bytes above.
 *
 * Portable C with no hardware access; the main loop owns it. */
#ifndef SIDOU_MIDI_OUT_H
#define SIDOU_MIDI_OUT_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "midi_map.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The one cable of the USB-MIDI function (platform/fm1/src/usb.c: one embedded jack each way). */
#define MIDI_OUT_CABLE 0u
/* Packets midi_out_tick writes at most: four NRPNs, or sixteen CCs. */
#define MIDI_OUT_TICK_PACKETS 16u
/* Packets per message: a CC or program change is one, an NRPN four. */
#define MIDI_OUT_NRPN_PACKETS 4u
/* midi_out.row of a parameter with no map row (midi_map_row gives NULL for it). */
#define MIDI_OUT_NO_ROW 0xffu

typedef struct {
    uint16_t seen[LIVE_PARAM_COUNT]; /* each parameter's value at the last look */
    uint16_t wire[LIVE_PARAM_COUNT]; /* its value as last sent, or received from MIDI */
    uint8_t row[LIVE_PARAM_COUNT];   /* each parameter's map row (midi_map_row's index), or
                                      * MIDI_OUT_NO_ROW: a byte, not a pointer, a quarter the RAM */
    uint8_t pending[LIVE_PARAM_COUNT];
    uint8_t preset_seen;
    uint8_t preset_pending; /* a program change to send */
    uint8_t bank_sent;      /* the bank of the last program change sent: 0, or 1 (songs) */
    uint8_t song_pending;   /* a song's program change to send (midi_out_song) */
    uint8_t song_program;
    uint8_t channel;        /* 0..15 */
    uint8_t hires;          /* 1: NRPN for the parameters of more than 128 values */
    uint8_t next;           /* where the next tick's round robin starts */
    uint32_t packets;       /* packets written */
    uint32_t deferred;      /* ticks that left something pending for lack of room or budget */
    uint32_t dropped;       /* pending changes midi_out_drop forgot */
    const midi_learn* learn; /* MIDI learn's bindings (midi_out_learn), or NULL */
    uint32_t expert_seen;   /* live.expert_version at the last expert look */
    uint16_t expert_pending; /* explicit experts that need macro-before-expert ordering */
    uint16_t release_pending; /* macro-follow transitions not yet carried by an emitted message */
} midi_out;

/* Nothing pending: the instrument's state now is what the other side is taken to have.
 * channel 0..15 (masked), hires 0 or 1. No MIDI learn. */
void midi_out_init(midi_out* o, const live* l, uint8_t channel, uint8_t hires);
/* Sends by MIDI learn's bindings from now on ("MIDI learn" above); NULL: the map's alone. */
void midi_out_learn(midi_out* o, const midi_learn* learn);
/* A song loaded from the SONG menu: its program n (0..127) in bank 1, at the next tick. */
void midi_out_song(midi_out* o, uint8_t n);
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
