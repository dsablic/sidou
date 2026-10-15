// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_out.h"

#include "midi_map.h"

/* Code Index Numbers (midi_usb.h). */
#define CIN_CC 0x0Bu
#define CIN_PROGRAM 0x0Cu
#define STATUS_CC 0xB0u
#define STATUS_PROGRAM 0xC0u
#define CC_DATA_MSB 6u
#define CC_DATA_LSB 38u
#define CC_NRPN_LSB 98u
#define CC_NRPN_MSB 99u

_Static_assert(LIVE_PARAM_COUNT <= 255u, "midi_out.next is a uint8_t");
_Static_assert(MIDI_OUT_TICK_PACKETS >= MIDI_OUT_NRPN_PACKETS, "an NRPN must fit a tick");

static uint32_t packet(uint32_t cin, uint32_t status, uint32_t d1, uint32_t d2) {
    return MIDI_OUT_CABLE << 4 | cin | status << 8 | d1 << 16 | d2 << 24;
}

/* 1: the parameter goes as its NRPN (midi_out.h, "Encoding"). */
static int as_nrpn(const midi_out* o, const live* l, const midi_map_entry* e) {
    if (e->cc == MIDI_MAP_NO_CC) {
        return 1;
    }
    if (e->cc == LIVE_CC_MOD_WHEEL && l->mod_target != LIVE_MOD_PULSE_WIDTH) {
        return 1; /* CC 1 moves the wheel, not the pulse width */
    }
    return o->hires && e->max > 127u;
}

static void resync(midi_out* o, const live* l) {
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        uint16_t v = (uint16_t)live_param_get(l, p);
        o->seen[p] = v;
        o->wire[p] = v;
        o->pending[p] = 0;
    }
}

/* What changed since the last look: the panel's (remote 0) or MIDI input's (remote 1). */
static void look(midi_out* o, const live* l, int remote) {
    if (l->preset != o->preset_seen) {
        /* A preset change: the program change carries every parameter it loaded. */
        o->preset_seen = l->preset;
        o->preset_pending = remote ? 0u : 1u;
        resync(o, l);
        return;
    }
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        uint16_t v = (uint16_t)live_param_get(l, p);
        if (v == o->seen[p]) {
            continue;
        }
        o->seen[p] = v;
        if (remote) {
            o->wire[p] = v; /* the sender has it */
            o->pending[p] = 0;
        } else {
            o->pending[p] = 1;
        }
    }
}

void midi_out_init(midi_out* o, const live* l, uint8_t channel, uint8_t hires) {
    resync(o, l);
    o->preset_seen = l->preset;
    o->preset_pending = 0;
    o->channel = channel & 0x0fu;
    o->hires = hires ? 1u : 0u;
    o->next = 0;
    o->packets = 0;
    o->deferred = 0;
    o->dropped = 0;
}

void midi_out_local(midi_out* o, const live* l) {
    look(o, l, 0);
}

void midi_out_remote(midi_out* o, const live* l) {
    look(o, l, 1);
}

uint32_t midi_out_tick(midi_out* o, const live* l, uint32_t room,
                       uint32_t out[MIDI_OUT_TICK_PACKETS]) {
    look(o, l, 0);
    uint32_t budget = room < MIDI_OUT_TICK_PACKETS ? room : MIDI_OUT_TICK_PACKETS;
    uint32_t cc = STATUS_CC | o->channel;
    uint32_t n = 0;
    if (o->preset_pending) {
        if (budget == 0) {
            o->deferred++;
            return 0; /* no parameter before the program change */
        }
        out[n++] = packet(CIN_PROGRAM, STATUS_PROGRAM | o->channel, o->preset_seen, 0);
        o->preset_pending = 0;
    }
    for (uint32_t i = 0; i < LIVE_PARAM_COUNT; ++i) {
        uint32_t p = (o->next + i) % LIVE_PARAM_COUNT;
        if (!o->pending[p]) {
            continue;
        }
        const midi_map_entry* e = midi_map_for_param(p);
        uint32_t v = o->seen[p]; /* the latest: look just ran */
        int nrpn = e && as_nrpn(o, l, e);
        if (!e || (nrpn && e->nrpn == MIDI_MAP_NO_NRPN) ||
            (nrpn ? v == o->wire[p] : midi_map_to_cc(e, v) == midi_map_to_cc(e, o->wire[p]))) {
            o->pending[p] = 0; /* nothing to send it as, or nothing new on the wire */
            continue;
        }
        if (n + (nrpn ? MIDI_OUT_NRPN_PACKETS : 1u) > budget) {
            o->next = (uint8_t)p; /* the next tick starts here */
            o->deferred++;
            break;
        }
        if (nrpn) {
            out[n++] = packet(CIN_CC, cc, CC_NRPN_MSB, (uint32_t)e->nrpn >> 7);
            out[n++] = packet(CIN_CC, cc, CC_NRPN_LSB, e->nrpn & 0x7fu);
            out[n++] = packet(CIN_CC, cc, CC_DATA_MSB, v >> 7);
            out[n++] = packet(CIN_CC, cc, CC_DATA_LSB, v & 0x7fu);
        } else {
            out[n++] = packet(CIN_CC, cc, e->cc, midi_map_to_cc(e, v));
        }
        o->wire[p] = (uint16_t)v;
        o->pending[p] = 0;
    }
    o->packets += n;
    return n;
}

void midi_out_drop(midi_out* o, const live* l) {
    look(o, l, 0);
    uint32_t n = o->preset_pending;
    o->preset_pending = 0;
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        n += o->pending[p];
        o->pending[p] = 0;
    }
    o->dropped += n;
}
