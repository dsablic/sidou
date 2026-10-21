// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_rx.h"

#include <stddef.h>

#include "midi_route.h"
#include "midi_sync.h"
#include "midi_usb.h"

/* With no router, every channel message plays the instrument. */
static const uint8_t UNROUTED[16] = {
    0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu,
    0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu, 0x7fu,
};

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
    rx->inputs += (uint32_t)(msg->kind != MIDI_REALTIME || msg->data1 == 0xFFu);
    /* the instrument's note or CC goes straight on (a CC flood pays one look a message) */
    if (!((rx->fast[msg->channel] >> msg->kind) & 1u)) {
        if (rx->sync && msg->kind >= MIDI_REALTIME) {
            /* and on: the ARP's clock is the router's */
            midi_sync_msg(rx->sync, msg, rx->entry ? rx->entry->stamp : 0u);
        }
        midi_route* r = rx->route;
        if (r) {
            midi_route_to to = midi_route_classify(r, msg);
            if (to == MIDI_ROUTE_TAKE) {
                if (rx->out && !rx->looked && midi_route_edits_live(r, msg)) {
                    midi_out_local(rx->out, rx->l); /* a preset or a profile: MIDI's */
                    rx->looked = 1;
                }
                midi_route_take(r, msg);
            }
            if (to != MIDI_ROUTE_LIVE) {
                return;
            }
        }
    }
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

/* A source's parser and NRPN state, anew. Its SysEx callback stays: the caller's (the platform
 * sets the parsers' on_sysex after midi_rx_init; none, NULL, skips SysEx). */
static void port_init(midi_rx* rx, midi_in* in, midi_map_port* map, midi_msg_fn on_msg) {
    midi_sysex_fn on_sysex = in->on_sysex;
    midi_map_port_init(map, rx->l);
    map->learn = rx->learn;
    map->banks = rx->route ? rx->route->bank : NULL;
    midi_in_init(in, on_msg, on_sysex, rx);
}

/* A gap: the sources it lost (MIDI_SRC_* bits; 0, unknown: both) restart their parsers, so
 * a message or a SysEx frame cut by it is dropped, while the other source's go on (a SysEx
 * transfer on it included). Every note is released and the controllers reset either way. */
static void restart(midi_rx* rx, uint8_t epoch, uint8_t sources) {
    rx->epoch = epoch;
    rx->restarts++;
    sources = sources & (MIDI_SRC_USB | MIDI_SRC_TRS) ? sources : MIDI_SRC_USB | MIDI_SRC_TRS;
    if (sources & MIDI_SRC_TRS) {
        port_init(rx, &rx->trs, &rx->trs_map, on_trs);
    }
    if (sources & MIDI_SRC_USB) {
        port_init(rx, &rx->usb, &rx->usb_map, on_usb);
    }
    live_all_notes_off(rx->l);
    live_reset_controllers(rx->l); /* a bend back to the centre may be among the lost */
    if (rx->route) {
        midi_route_reset_input(rx->route);
    }
}

void midi_rx_init(midi_rx* rx, live* l, uint8_t epoch) {
    rx->l = l;
    rx->epoch = epoch;
    rx->restarts = 0;
    rx->out = 0;
    rx->looked = 0;
    rx->learn = NULL;
    rx->inputs = 0;
    rx->route = NULL;
    rx->fast = UNROUTED;
    rx->sync = NULL;
    rx->entry = NULL;
    rx->trs.on_sysex = NULL; /* SysEx is skipped until the platform sets a callback */
    rx->usb.on_sysex = NULL;
    port_init(rx, &rx->trs, &rx->trs_map, on_trs);
    port_init(rx, &rx->usb, &rx->usb_map, on_usb);
}

void midi_rx_sync(midi_rx* rx, struct midi_sync* sync) {
    rx->sync = sync;
}

void midi_rx_route(midi_rx* rx, struct midi_route* route) {
    rx->route = route;
    rx->fast = route ? route->fast : UNROUTED;
    rx->trs_map.banks = route ? route->bank : NULL;
    rx->usb_map.banks = rx->trs_map.banks;
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
        restart(rx, e->epoch, e->gap);
    }
    rx->entry = e;
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

int midi_rx_check_lost(midi_rx* rx, uint8_t epoch, uint8_t sources) {
    if ((int8_t)(uint8_t)(epoch - rx->epoch) <= 0) {
        return 0; /* not newer: an entry taken since the caller read it may be ahead already */
    }
    restart(rx, epoch, sources);
    return 1;
}
