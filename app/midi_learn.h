// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_LEARN_H
#define SIDOU_MIDI_LEARN_H

#include <stdint.h>

#include "live.h"
#include "midi_map.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_LEARN_TIMEOUT_MS 10000u
#define MIDI_LEARN_NONE 0xffu

typedef enum {
    MIDI_LEARN_IDLE = 0,
    MIDI_LEARN_CC = 2,
    MIDI_LEARN_CONFIRM = 3
} midi_learn_state;

typedef enum {
    MIDI_LEARN_NOTHING = 0,
    MIDI_LEARN_LEARNED = 1,
    MIDI_LEARN_FORGOT = 2,
    MIDI_LEARN_CLEARED = 3,
    MIDI_LEARN_CANCELLED = 4,
    MIDI_LEARN_TIMED_OUT = 5
} midi_learn_result;

typedef struct midi_learn {
    uint8_t cc_row[128];
    uint8_t param_cc[LIVE_PARAM_COUNT];
    uint8_t count;
    uint8_t state;
    uint8_t param;
    uint8_t result;
    uint8_t result_cc;
    uint8_t settings_dirty;
    uint8_t* settings_wake;
    uint32_t since;
} midi_learn;

void midi_learn_init(midi_learn* ml);
int midi_learn_learnable(uint32_t cc);
int midi_learn_param(uint32_t param);

int midi_learn_open(midi_learn* ml, uint32_t param, uint32_t now);
int midi_learn_forget_picked(midi_learn* ml, uint32_t now);
void midi_learn_ask_clear(midi_learn* ml, uint32_t now);
void midi_learn_confirm(midi_learn* ml);
void midi_learn_cancel(midi_learn* ml);
int midi_learn_tick(midi_learn* ml, uint32_t now);
midi_learn_result midi_learn_take(midi_learn* ml);

const midi_map_entry* midi_learn_route(midi_learn* ml, uint8_t cc);
const midi_map_entry* midi_learn_for_cc(const midi_learn* ml, uint32_t cc);
int midi_learn_takes(const midi_learn* ml, uint32_t cc);
uint8_t midi_learn_cc_of(const midi_learn* ml, uint32_t param);

int midi_learn_bind(midi_learn* ml, uint32_t cc, uint32_t param);
void midi_learn_forget(midi_learn* ml, uint32_t param);
void midi_learn_clear(midi_learn* ml);

#ifdef __cplusplus
}
#endif

#endif
