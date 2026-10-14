// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_rx.h"

#include "midi_usb.h"

void midi_rx_live(live* l, const midi_msg* msg) {
    switch (msg->kind) {
    case MIDI_NOTE_ON: live_note_on(l, msg->data1, msg->data2); break;
    case MIDI_NOTE_OFF: live_note_off(l, msg->data1); break;
    case MIDI_CC: live_cc(l, msg->data1, msg->data2); break;
    case MIDI_PROGRAM:
        if (msg->data1 < LIVE_PRESET_COUNT) {
            live_set_preset(l, msg->data1);
        }
        break;
    default: break; /* pressure, pitch bend, real time: not in M1a's mapping */
    }
}

static void on_msg(void* ctx, const midi_msg* msg) {
    midi_rx_live(ctx, msg);
}

/* SysEx is skipped (no midi_sysex_fn) on both sources for now.
 * TODO(Task 6): the update protocol's step 1 (identity request, upgrade request). Its USB
 * frames are assembled in the vendored USB stack (platform/fm1/usb/usb.c, FELUCCA_OTA, off
 * in M1a); TRS SysEx would arrive here, through a midi_sysex_fn on rx->trs. */
static void parsers_init(midi_rx* rx) {
    midi_in_init(&rx->trs, on_msg, 0, rx->l);
    midi_in_init(&rx->usb, on_msg, 0, rx->l);
}

void midi_rx_init(midi_rx* rx, live* l, uint32_t lost) {
    rx->l = l;
    rx->lost_seen = lost;
    rx->restarts = 0;
    parsers_init(rx);
}

void midi_rx_entry(midi_rx* rx, const midi_ring_entry* e) {
    if (e->source == MIDI_SRC_TRS) {
        midi_in_byte(&rx->trs, (uint8_t)e->data);
    } else if (e->source == MIDI_SRC_USB) {
        uint8_t b[3];
        uint32_t n = midi_usb_bytes(e->data, b);
        midi_in_feed(&rx->usb, b, n);
    }
}

int midi_rx_check_lost(midi_rx* rx, uint32_t lost) {
    if (lost == rx->lost_seen) {
        return 0;
    }
    rx->lost_seen = lost;
    rx->restarts++;
    parsers_init(rx);
    live_all_notes_off(rx->l);
    return 1;
}
