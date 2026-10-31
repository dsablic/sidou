// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_OUT_H
#define SIDOU_MIDI_OUT_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "midi_map.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_OUT_CABLE 0u
#define MIDI_OUT_TICK_PACKETS 16u
#define MIDI_OUT_NRPN_PACKETS 4u
#define MIDI_OUT_NO_ROW 0xffu

typedef struct {
    uint16_t seen[LIVE_PARAM_COUNT];
    uint16_t wire[LIVE_PARAM_COUNT];
    uint8_t row[LIVE_PARAM_COUNT];
    uint8_t pending[LIVE_PARAM_COUNT];
    uint8_t preset_seen;
    uint8_t preset_pending;
    uint8_t bank_sent;
    uint8_t song_pending;
    uint8_t song_program;
    uint8_t channel;
    uint8_t hires;
    uint8_t next;
    uint32_t packets;
    uint32_t deferred;
    uint32_t dropped;
    const midi_learn* learn;
    uint32_t expert_seen;
    uint16_t expert_pending;
    uint16_t release_pending;
    uint16_t pwm_step;
} midi_out;

void midi_out_init(midi_out* o, const live* l, uint8_t channel, uint8_t hires);
void midi_out_learn(midi_out* o, const midi_learn* learn);
void midi_out_song(midi_out* o, uint8_t n);
void midi_out_local(midi_out* o, const live* l);
void midi_out_remote(midi_out* o, const live* l);
uint32_t midi_out_tick(midi_out* o, const live* l, uint32_t room,
                       uint32_t out[MIDI_OUT_TICK_PACKETS]);
void midi_out_drop(midi_out* o, const live* l);

#ifdef __cplusplus
}
#endif

#endif
