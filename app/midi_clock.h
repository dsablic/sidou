// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_CLOCK_H
#define SIDOU_MIDI_CLOCK_H

#include <stdint.h>

#include "midi_in.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_CLOCK_WINDOW 96u
#define MIDI_CLOCK_LOCK 24u
#define MIDI_CLOCK_LOST_MS 300u
#define MIDI_CLOCK_PER_BEAT 24u
#define MIDI_CLOCK_PER_16TH 6u
#define MIDI_CLOCK_CORRECTION 100u

typedef enum {
    MIDI_CLOCK_EV_NONE = 0,
    MIDI_CLOCK_EV_START = 1,
    MIDI_CLOCK_EV_CONTINUE = 2,
    MIDI_CLOCK_EV_STOP = 3,
    MIDI_CLOCK_EV_POSITION = 4,
    MIDI_CLOCK_EV_LOST = 5
} midi_clock_event;

typedef struct {
    uint32_t stamp[MIDI_CLOCK_WINDOW];
    uint32_t number[MIDI_CLOCK_WINDOW];
    uint32_t sample_rate;
    uint32_t lost_after;
    uint32_t clocks;
    uint32_t last;
    float period;
    float at_ref;
    uint32_t ref_stamp, ref_number;
    uint32_t hold_start, hold_end;
    uint32_t position;
    uint32_t base_number, base_position;
    uint8_t head, count;
    uint8_t active;
    uint8_t running;
    uint8_t has_hold;
    uint8_t based;
} midi_clock;

void midi_clock_init(midi_clock* c, uint32_t sample_rate);

midi_clock_event midi_clock_msg(midi_clock* c, const midi_msg* msg, uint32_t stamp);

void midi_clock_hold(midi_clock* c, uint32_t start, uint32_t end);

midi_clock_event midi_clock_poll(midi_clock* c, uint32_t now);

uint32_t midi_clock_period_q16(const midi_clock* c);

int midi_clock_time_of(const midi_clock* c, uint32_t position, uint32_t* stamp);

uint32_t midi_clock_frame_q16(const midi_clock* c, uint8_t tempo0, uint8_t tempo1,
                              uint8_t rows_per_beat, int32_t late);

#ifdef __cplusplus
}
#endif

#endif
