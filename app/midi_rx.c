// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_rx.h"

#include <stddef.h>

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
    case MIDI_PITCH_BEND: live_bend(l, msg->bend); break;
    case MIDI_CHANNEL_PRESSURE: live_pressure(l, msg->data1); break;
    case MIDI_POLY_PRESSURE: live_poly_pressure(l, msg->data1, msg->data2); break;
    default: break; /* real time, song position and select: not the instrument's */
    }
}

/* 0: the message cannot change a parameter (live_param_get), so MIDI out need not look
 * around it: the NRPN and RPN selection, the notes off, the controllers' reset (bend, wheel,
 * pressure), the wheel on a target other than the pulse width and any other CC the map does
 * not give a parameter (CC 64, a reserved row's), unless MIDI learn takes it (bound, or
 * binding now). Data entry (6, 38, 96, 97) can. */
static int edits(const midi_rx* rx, const midi_msg* msg) {
    const live* l = rx->l;
    if (msg->kind == MIDI_PROGRAM) {
        return msg->data1 < LIVE_PRESET_COUNT;
    }
    if (msg->kind != MIDI_CC) {
        return 0; /* notes, bend and aftertouch */
    }
    switch (msg->data1) {
    case 98:
    case 99:
    case 100:
    case 101:
    case LIVE_CC_ALL_SOUND_OFF:
    case LIVE_CC_RESET_ALL:
    case LIVE_CC_ALL_NOTES_OFF: return 0;
    case 6:
    case 38:
    case 96:
    case 97: return 1;
    case LIVE_CC_MOD_WHEEL:
        if (l->mod_target == LIVE_MOD_PULSE_WIDTH) {
            return 1;
        }
        break;
    default: {
        const midi_map_entry* e = midi_map_for_cc(msg->data1);
        if (e && e->param != MIDI_MAP_RESERVED) {
            return 1;
        }
        break;
    }
    }
    return rx->learn != NULL && midi_learn_takes(rx->learn, msg->data1);
}

/* One message from a source's parser. Before the first that can change a parameter in a
 * pass, MIDI out takes what changed until then as the panel's; midi_rx_settle takes the rest
 * as MIDI's. */
static void take(midi_rx* rx, midi_map_port* port, const midi_msg* msg) {
    if (rx->out && !rx->looked && edits(rx, msg)) {
        midi_out_local(rx->out, rx->l);
        rx->looked = 1;
    }
    midi_rx_live(port, msg);
}

static void on_trs(void* ctx, const midi_msg* msg) {
    midi_rx* rx = ctx;
    take(rx, &rx->trs_map, msg);
}

static void on_usb(void* ctx, const midi_msg* msg) {
    midi_rx* rx = ctx;
    take(rx, &rx->usb_map, msg);
}

/* SysEx is skipped (no midi_sysex_fn) on both sources. The update protocol takes its frames
 * from USB inside the vendored USB stack (platform/fm1/update.h): over USB only, because it
 * needs replies and the TRS jack is MIDI IN only. */
static void parsers_init(midi_rx* rx) {
    midi_map_port_init(&rx->trs_map, rx->l);
    midi_map_port_init(&rx->usb_map, rx->l);
    rx->trs_map.learn = rx->learn;
    rx->usb_map.learn = rx->learn;
    midi_in_init(&rx->trs, on_trs, 0, rx);
    midi_in_init(&rx->usb, on_usb, 0, rx);
}

static void restart(midi_rx* rx, uint8_t epoch) {
    rx->epoch = epoch;
    rx->restarts++;
    parsers_init(rx);
    live_all_notes_off(rx->l);
    live_reset_controllers(rx->l); /* a bend back to the centre may be among the lost */
}

void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch) {
    rx->l = l;
    rx->epoch = epoch;
    rx->restarts = 0;
    rx->out = 0;
    rx->looked = 0;
    rx->learn = NULL;
    parsers_init(rx);
}

void midi_rx_learn(midi_rx* rx, midi_learn* learn) {
    rx->learn = learn;
    rx->trs_map.learn = learn;
    rx->usb_map.learn = learn;
}

void midi_rx_watch(midi_rx* rx, midi_out* out) {
    rx->out = out;
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

void midi_rx_settle(midi_rx* rx) {
    if (rx->looked) {
        midi_out_remote(rx->out, rx->l);
        rx->looked = 0;
    }
}

int midi_rx_check_lost(midi_rx* rx, uint8_t epoch) {
    if ((int8_t)(uint8_t)(epoch - rx->epoch) <= 0) {
        return 0; /* not newer: an entry taken since the caller read it may be ahead already */
    }
    restart(rx, epoch);
    return 1;
}
