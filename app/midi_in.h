// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* A MIDI 1.0 byte-stream parser: a pure state machine, one byte at a time.
 *
 * - Running status: channel messages may omit their status byte.
 * - Real-time bytes (0xF8..0xFF) are reported where they occur, even inside another
 *   message or a SysEx, and change no state.
 * - SysEx (0xF0 .. 0xF7) is passed byte by byte to the SysEx callback and is otherwise
 *   ignored: MIDI_SYSEX_START (F0), MIDI_SYSEX_DATA for each data byte, then exactly one
 *   of MIDI_SYSEX_END (F7) or MIDI_SYSEX_ABORT (the status byte that cut it short; an F0
 *   inside a SysEx aborts it and starts a new one). Real-time bytes inside it are not
 *   part of it. A consumer discards what it collected on ABORT.
 * - System common messages (0xF1..0xF7) cancel running status. Song position (F2) and song
 *   select (F3) are reported; MTC quarter frames' (F1) data is skipped. A real-time byte
 *   inside them is reported where it occurs, as inside any message.
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
    MIDI_REALTIME = 7,          /* data1 the status byte, 0xF8..0xFF; channel 0 */
    MIDI_SONG_POSITION = 8,     /* bend the position in 16ths, 0..16383; channel 0 */
    MIDI_SONG_SELECT = 9        /* data1 the song, 0..127; channel 0 */
} midi_kind;

typedef struct {
    midi_kind kind;
    uint8_t channel; /* 0..15 */
    uint8_t data1;
    uint8_t data2;
    uint16_t bend; /* pitch bend, or the song position */
} midi_msg;

typedef enum {
    MIDI_SYSEX_START = 0, /* byte is F0 */
    MIDI_SYSEX_DATA = 1,  /* byte is 0x00..0x7F */
    MIDI_SYSEX_END = 2,   /* byte is F7 */
    MIDI_SYSEX_ABORT = 3  /* byte is the status byte (0x80..0xF6) that ended it */
} midi_sysex_event;

typedef void (*midi_msg_fn)(void* ctx, const midi_msg* msg);
typedef void (*midi_sysex_fn)(void* ctx, midi_sysex_event event, uint8_t byte);

typedef struct {
    midi_msg_fn on_msg;     /* must not be NULL */
    midi_sysex_fn on_sysex; /* may be NULL: SysEx is then skipped */
    void* ctx;
    uint8_t status; /* the message being assembled; 0 if none */
    uint8_t count;  /* its data bytes so far */
    uint8_t data[2];
    uint8_t in_sysex;
} midi_in;

/* on_msg must not be NULL; on_sysex may be. */
void midi_in_init(midi_in* m, midi_msg_fn on_msg, midi_sysex_fn on_sysex, void* ctx);
void midi_in_byte(midi_in* m, uint8_t byte);
void midi_in_feed(midi_in* m, const uint8_t* bytes, size_t n);
/* As midi_in_feed of a channel message whole: `status` 0x80..0xEF, then its data bytes (d1,
 * and d2 for the two-byte kinds), each below 0x80. The same messages and state as the bytes
 * one at a time, without the parse (a USB packet's channel message, midi_rx.c). */
void midi_in_channel(midi_in* m, uint8_t status, uint8_t d1, uint8_t d2);

#ifdef __cplusplus
}
#endif

#endif
