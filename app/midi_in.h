// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_IN_H
#define SIDOU_MIDI_IN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    MIDI_NOTE_OFF = 0,
    MIDI_NOTE_ON = 1,
    MIDI_POLY_PRESSURE = 2,
    MIDI_CC = 3,
    MIDI_PROGRAM = 4,
    MIDI_CHANNEL_PRESSURE = 5,
    MIDI_PITCH_BEND = 6,
    MIDI_REALTIME = 7,
    MIDI_SONG_POSITION = 8,
    MIDI_SONG_SELECT = 9
} midi_kind;

typedef struct {
    midi_kind kind;
    uint8_t channel;
    uint8_t data1;
    uint8_t data2;
    uint16_t bend;
} midi_msg;

typedef enum {
    MIDI_SYSEX_START = 0,
    MIDI_SYSEX_DATA = 1,
    MIDI_SYSEX_END = 2,
    MIDI_SYSEX_ABORT = 3
} midi_sysex_event;

typedef void (*midi_msg_fn)(void* ctx, const midi_msg* msg);
typedef void (*midi_sysex_fn)(void* ctx, midi_sysex_event event, uint8_t byte);

typedef struct {
    midi_msg_fn on_msg;
    midi_sysex_fn on_sysex;
    void* ctx;
    uint8_t status;
    uint8_t count;
    uint8_t data[2];
    uint8_t in_sysex;
} midi_in;

void midi_in_init(midi_in* m, midi_msg_fn on_msg, midi_sysex_fn on_sysex, void* ctx);
void midi_in_byte(midi_in* m, uint8_t byte);
void midi_in_feed(midi_in* m, const uint8_t* bytes, size_t n);
void midi_in_channel(midi_in* m, uint8_t status, uint8_t d1, uint8_t d2);

#ifdef __cplusplus
}
#endif

#endif
