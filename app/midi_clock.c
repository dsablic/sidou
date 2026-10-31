// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_clock.h"

void midi_clock_init(midi_clock* c, uint32_t sample_rate) {
    *c = (midi_clock){0};
    c->sample_rate = sample_rate;
    c->lost_after = sample_rate * MIDI_CLOCK_LOST_MS / 1000u;
}

static int32_t since(uint32_t a, uint32_t b) {
    return (int32_t)(b - a);
}

static int32_t round_i32(float x) {
    return (int32_t)(x < 0.0f ? x - 0.5f : x + 0.5f);
}

static void drop_fit(midi_clock* c) {
    c->count = 0;
    c->head = 0;
    c->period = 0.0f;
    c->at_ref = 0.0f;
}

static void fit(midi_clock* c) {
    unsigned n = c->count;
    float sx = 0.0f, sy = 0.0f;
    for (unsigned i = 0; i < n; ++i) {
        unsigned k = (c->head + i) % MIDI_CLOCK_WINDOW;
        sx += (float)(int32_t)(c->number[k] - c->ref_number);
        sy += (float)since(c->ref_stamp, c->stamp[k]);
    }
    float mx = sx / (float)n, my = sy / (float)n;
    float sxx = 0.0f, sxy = 0.0f;
    for (unsigned i = 0; i < n; ++i) {
        unsigned k = (c->head + i) % MIDI_CLOCK_WINDOW;
        float dx = (float)(int32_t)(c->number[k] - c->ref_number) - mx;
        float dy = (float)since(c->ref_stamp, c->stamp[k]) - my;
        sxx += dx * dx;
        sxy += dx * dy;
    }
    float b = sxx > 0.0f ? sxy / sxx : 0.0f;
    c->at_ref = my - b * mx;
    c->period = n >= MIDI_CLOCK_LOCK && b > 0.0f ? b : 0.0f;
}

static void add(midi_clock* c, uint32_t number, uint32_t stamp) {
    if (c->count == MIDI_CLOCK_WINDOW) {
        c->head = (uint8_t)((c->head + 1u) % MIDI_CLOCK_WINDOW);
        --c->count;
    }
    unsigned k = (c->head + c->count) % MIDI_CLOCK_WINDOW;
    c->number[k] = number;
    c->stamp[k] = stamp;
    ++c->count;
    c->ref_number = number;
    c->ref_stamp = stamp;
    fit(c);
}

static int in_hold(midi_clock* c, uint32_t stamp) {
    if (!c->has_hold || since(c->hold_start, stamp) < 0) {
        return 0;
    }
    if ((float)since(c->hold_end, stamp) <= c->period) {
        return 1;
    }
    c->has_hold = 0;
    return 0;
}

static midi_clock_event on_clock(midi_clock* c, uint32_t stamp) {
    midi_clock_event ev = MIDI_CLOCK_EV_NONE;
    if (c->active && since(c->last, stamp) > (int32_t)c->lost_after) {
        drop_fit(c);
        ev = MIDI_CLOCK_EV_LOST;
    }
    c->active = 1;
    c->last = stamp;
    uint32_t number = c->clocks++;
    if (c->running) {
        c->base_number = number;
        c->base_position = c->position++;
        c->based = 1;
    }
    if (!in_hold(c, stamp)) {
        add(c, number, stamp);
    }
    return ev;
}

midi_clock_event midi_clock_msg(midi_clock* c, const midi_msg* msg, uint32_t stamp) {
    if (msg->kind == MIDI_SONG_POSITION) {
        c->position = (uint32_t)msg->bend * MIDI_CLOCK_PER_16TH;
        c->based = 0;
        return MIDI_CLOCK_EV_POSITION;
    }
    if (msg->kind != MIDI_REALTIME) {
        return MIDI_CLOCK_EV_NONE;
    }
    switch (msg->data1) {
    case 0xf8: return on_clock(c, stamp);
    case 0xfa:
        c->position = 0;
        c->running = 1;
        c->based = 0;
        return MIDI_CLOCK_EV_START;
    case 0xfb:
        c->running = 1;
        c->based = 0;
        return MIDI_CLOCK_EV_CONTINUE;
    case 0xfc:
        c->running = 0;
        c->based = 0;
        return MIDI_CLOCK_EV_STOP;
    default: return MIDI_CLOCK_EV_NONE;
    }
}

void midi_clock_hold(midi_clock* c, uint32_t start, uint32_t end) {
    c->hold_start = start;
    c->hold_end = end;
    c->has_hold = 1;
}

midi_clock_event midi_clock_poll(midi_clock* c, uint32_t now) {
    if (c->active && since(c->last, now) > (int32_t)c->lost_after) {
        c->active = 0;
        drop_fit(c);
        return MIDI_CLOCK_EV_LOST;
    }
    return MIDI_CLOCK_EV_NONE;
}

uint32_t midi_clock_period_q16(const midi_clock* c) {
    float q16 = c->period * 65536.0f;
    return q16 >= 4294967040.0f ? 0xffffffffu : (uint32_t)(q16 + 0.5f);
}

int midi_clock_time_of(const midi_clock* c, uint32_t position, uint32_t* stamp) {
    if (!c->running || !c->based || c->period == 0.0f) {
        return 0;
    }
    int32_t clocks = (int32_t)(position - c->base_position) +
                     (int32_t)(c->base_number - c->ref_number);
    *stamp = c->ref_stamp + (uint32_t)round_i32(c->at_ref + c->period * (float)clocks);
    return 1;
}

uint32_t midi_clock_frame_q16(const midi_clock* c, uint8_t tempo0, uint8_t tempo1,
                              uint8_t rows_per_beat, int32_t late) {
    if (c->period == 0.0f || !tempo0 || !tempo1 || !rows_per_beat) {
        return 0;
    }
    float beat = c->period * (float)MIDI_CLOCK_PER_BEAT;
    float most = beat / (float)MIDI_CLOCK_CORRECTION;
    float shift = (float)late;
    shift = shift > most ? most : shift < -most ? -most : shift;
    float frames = (float)(tempo0 + tempo1) * 0.5f * (float)rows_per_beat;
    float q16 = (beat - shift) / frames * 65536.0f;
    return q16 >= 4294967040.0f ? 0xffffffffu : (uint32_t)(q16 + 0.5f);
}
