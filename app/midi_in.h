// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* A MIDI 1.0 byte-stream parser: a pure state machine, one byte at a time.
 *
 * - Running status: channel messages may omit their status byte.
 * - Real-time bytes (0xF8..0xFF) are reported where they occur, even inside another
 *   message or a SysEx, and change no state.
 * - SysEx (0xF0 .. 0xF7) is passed byte by byte, F0 and F7 included, to the SysEx
 *   callback and is otherwise ignored. A SysEx ended by any other status byte ends
 *   without an F7.
 * - System common messages (0xF1..0xF7) cancel running status; their data is skipped.
 * - An incomplete message is dropped when a new status byte arrives.
 * - Note-on with velocity 0 is reported as note-off. */
#ifndef SIDOU_MIDI_IN_H
#define SIDOU_MIDI_IN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    MIDI_NOTE_OFF = 0,          /* data1 note, data2 velocity */
    MIDI_NOTE_ON = 1,           /* data1 note, data2 velocity (1..127) */
    MIDI_POLY_PRESSURE = 2,     /* data1 note, data2 pressure */
    MIDI_CC = 3,                /* data1 controller, data2 value */
    MIDI_PROGRAM = 4,           /* data1 program */
    MIDI_CHANNEL_PRESSURE = 5,  /* data1 pressure */
    MIDI_PITCH_BEND = 6,        /* bend 0..16383, centre 8192 */
    MIDI_REALTIME = 7           /* data1 the status byte, 0xF8..0xFF; channel 0 */
} midi_kind;

typedef struct {
    midi_kind kind;
    uint8_t channel; /* 0..15 */
    uint8_t data1;
    uint8_t data2;
    uint16_t bend;
} midi_msg;

typedef void (*midi_msg_fn)(void* ctx, const midi_msg* msg);
typedef void (*midi_sysex_fn)(void* ctx, uint8_t byte);

typedef struct {
    midi_msg_fn on_msg;
    midi_sysex_fn on_sysex; /* may be NULL */
    void* ctx;
    uint8_t status; /* the message being assembled; 0 if none */
    uint8_t count;  /* its data bytes so far */
    uint8_t data[2];
    uint8_t in_sysex;
} midi_in;

void midi_in_init(midi_in* m, midi_msg_fn on_msg, midi_sysex_fn on_sysex, void* ctx);
void midi_in_byte(midi_in* m, uint8_t byte);
void midi_in_feed(midi_in* m, const uint8_t* bytes, size_t n);

#ifdef __cplusplus
}
#endif

#endif
