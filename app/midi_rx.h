// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
    uint8_t epoch;
    uint8_t looked;
    midi_in trs;
    midi_in usb;
    midi_map_port trs_map;
    midi_map_port usb_map;
    uint32_t restarts;
    midi_out* out;
    midi_learn* learn;
    uint32_t inputs;
    struct midi_route* route;
    const uint8_t* fast;
    struct midi_sync* sync;
    const midi_ring_entry* entry;
} midi_rx;

void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch);
void midi_rx_watch(midi_rx* rx, midi_out* out);
void midi_rx_learn(midi_rx* rx, midi_learn* learn);
void midi_rx_route(midi_rx* rx, struct midi_route* route);
void midi_rx_sync(midi_rx* rx, struct midi_sync* sync);
void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e);
void midi_rx_settle(midi_rx* rx);
int midi_rx_check_lost(midi_rx* rx, uint8_t epoch, uint8_t sources);
void midi_rx_live(midi_map_port* port, const midi_msg* msg);

#ifdef __cplusplus
}
#endif

#endif
