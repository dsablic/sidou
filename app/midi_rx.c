// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_rx.h"

#include "midi_usb.h"

void midi_rx_live(midi_map_port* port, const midi_msg* msg) {
    live* l = port->l;
    switch (msg->kind) {
    case MIDI_NOTE_ON: live_note_on(l, msg->data1, msg->data2); break;
    case MIDI_NOTE_OFF: live_note_off(l, msg->data1); break;
    case MIDI_CC: midi_map_cc(port, msg->channel, msg->data1, msg->data2); break;
    case MIDI_PROGRAM:
        if (msg->data1 < LIVE_PRESET_COUNT) {
            live_set_preset(l, msg->data1);
        }
        break;
    default: break; /* pressure, pitch bend, real time: not mapped yet */
    }
}

static void on_msg(void* ctx, const midi_msg* msg) {
    midi_rx_live(ctx, msg);
}

/* SysEx is skipped (no midi_sysex_fn) on both sources. The update protocol takes its frames
 * from USB inside the vendored USB stack (platform/fm1/update.h): over USB only, because it
 * needs replies and the TRS jack is MIDI IN only. */
static void parsers_init(midi_rx* rx) {
    midi_map_port_init(&rx->trs_map, rx->l);
    midi_map_port_init(&rx->usb_map, rx->l);
    midi_in_init(&rx->trs, on_msg, 0, &rx->trs_map);
    midi_in_init(&rx->usb, on_msg, 0, &rx->usb_map);
}

static void restart(midi_rx* rx, uint8_t epoch) {
    rx->epoch = epoch;
    rx->restarts++;
    parsers_init(rx);
    live_all_notes_off(rx->l);
}

void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch) {
    rx->l = l;
    rx->epoch = epoch;
    rx->restarts = 0;
    parsers_init(rx);
}

void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e) {
    if (e->epoch != rx->epoch) {
        restart(rx, e->epoch);
    }
    if (e->source == MIDI_SRC_TRS) {
        midi_in_byte(&rx->trs, (uint8_t)e->data);
    } else if (e->source == MIDI_SRC_USB) {
        uint8_t b[3];
        uint32_t n = midi_usb_bytes(e->data, b);
        midi_in_feed(&rx->usb, b, n);
    }
}

int midi_rx_check_lost(midi_rx* rx, uint8_t epoch) {
    if ((int8_t)(uint8_t)(epoch - rx->epoch) <= 0) {
        return 0; /* not newer: an entry taken since the caller read it may be ahead already */
    }
    restart(rx, epoch);
    return 1;
}
