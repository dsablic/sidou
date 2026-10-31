// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_in.h"

void midi_in_init(midi_in* m, midi_msg_fn on_msg, midi_sysex_fn on_sysex, void* ctx) {
    m->on_msg = on_msg;
    m->on_sysex = on_sysex;
    m->ctx = ctx;
    m->status = 0;
    m->count = 0;
    m->data[0] = 0;
    m->data[1] = 0;
    m->in_sysex = 0;
}

static uint8_t data_length(uint8_t status) {
    switch (status & 0xf0u) {
    case 0xc0:
    case 0xd0:
        return 1;
    case 0xf0:
        switch (status) {
        case 0xf1:
        case 0xf3:
            return 1;
        case 0xf2:
            return 2;
        default:
            return 0;
        }
    default: return 2;
    }
}

static void emit(midi_in* m, midi_kind kind, uint8_t channel, uint8_t d1, uint8_t d2,
                 uint16_t bend) {
    midi_msg msg;
    msg.kind = kind;
    msg.channel = channel;
    msg.data1 = d1;
    msg.data2 = d2;
    msg.bend = bend;
    m->on_msg(m->ctx, &msg);
}

static void sysex(midi_in* m, midi_sysex_event event, uint8_t byte) {
    if (m->on_sysex) {
        m->on_sysex(m->ctx, event, byte);
    }
}

static void complete(midi_in* m) {
    uint8_t s = m->status;
    uint8_t ch = s & 0x0fu;
    uint8_t a = m->data[0], b = m->data[1];
    m->count = 0;
    switch (s & 0xf0u) {
    case 0x80: emit(m, MIDI_NOTE_OFF, ch, a, b, 0); break;
    case 0x90: emit(m, b ? MIDI_NOTE_ON : MIDI_NOTE_OFF, ch, a, b, 0); break;
    case 0xa0: emit(m, MIDI_POLY_PRESSURE, ch, a, b, 0); break;
    case 0xb0: emit(m, MIDI_CC, ch, a, b, 0); break;
    case 0xc0: emit(m, MIDI_PROGRAM, ch, a, 0, 0); break;
    case 0xd0: emit(m, MIDI_CHANNEL_PRESSURE, ch, a, 0, 0); break;
    case 0xe0: emit(m, MIDI_PITCH_BEND, ch, 0, 0, (uint16_t)(a | b << 7)); break;
    default:
        m->status = 0;
        if (s == 0xf2) {
            emit(m, MIDI_SONG_POSITION, 0, 0, 0, (uint16_t)(a | b << 7));
        } else if (s == 0xf3) {
            emit(m, MIDI_SONG_SELECT, 0, a, 0, 0);
        }
        break;
    }
}

void midi_in_byte(midi_in* m, uint8_t byte) {
    if (byte >= 0xf8) {
        emit(m, MIDI_REALTIME, 0, byte, 0, 0);
        return;
    }
    if (byte & 0x80u) {
        if (m->in_sysex) {
            sysex(m, byte == 0xf7 ? MIDI_SYSEX_END : MIDI_SYSEX_ABORT, byte);
        }
        m->in_sysex = byte == 0xf0;
        if (m->in_sysex) {
            sysex(m, MIDI_SYSEX_START, byte);
        }
        m->count = 0;
        m->status = byte;
        if (byte >= 0xf0 && data_length(byte) == 0) {
            m->status = 0;
        }
        return;
    }
    if (m->in_sysex) {
        sysex(m, MIDI_SYSEX_DATA, byte);
        return;
    }
    if (!m->status) {
        return;
    }
    m->data[m->count++] = byte;
    if (m->count == data_length(m->status)) {
        complete(m);
    }
}

void midi_in_channel(midi_in* m, uint8_t status, uint8_t d1, uint8_t d2) {
    if (m->in_sysex) {
        sysex(m, MIDI_SYSEX_ABORT, status);
        m->in_sysex = 0;
    }
    m->status = status;
    m->data[0] = d1;
    if (data_length(status) == 2u) {
        m->data[1] = d2;
    }
    complete(m);
}

void midi_in_feed(midi_in* m, const uint8_t* bytes, size_t n) {
    for (size_t i = 0; i < n; ++i) {
        midi_in_byte(m, bytes[i]);
    }
}
