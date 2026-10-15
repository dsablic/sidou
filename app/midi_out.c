// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_out.h"

#include <stddef.h>

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
_Static_assert(LIVE_PARAM_COUNT <= 64u, "PRESET_LOADS is a uint64_t");

#define BIT(p) ((uint64_t)1 << (p))
/* Exactly the parameters live_set_preset sets (live.h): not the volume, the chip model or the
 * effect slots. It keeps a bend range set by RPN 0, but then nothing can be pending for it (a
 * panel change ends RPN 0's) and wire is its value, so taking it as loaded changes nothing. */
static const uint64_t PRESET_LOADS =
    BIT(LIVE_PARAM_CUTOFF) | BIT(LIVE_PARAM_RESONANCE) | BIT(LIVE_PARAM_PULSE_WIDTH) |
    BIT(LIVE_PARAM_WAVEFORM) | BIT(LIVE_PARAM_RING) | BIT(LIVE_PARAM_SYNC) |
    BIT(LIVE_PARAM_ATTACK) | BIT(LIVE_PARAM_DECAY) | BIT(LIVE_PARAM_SUSTAIN) |
    BIT(LIVE_PARAM_RELEASE) | BIT(LIVE_PARAM_FILTER_MODE) | BIT(LIVE_PARAM_FILTER_VOICES) |
    BIT(LIVE_PARAM_BEND_RANGE) | BIT(LIVE_PARAM_VELOCITY_TARGET) | BIT(LIVE_PARAM_MOD_TARGET) |
    BIT(LIVE_PARAM_PRESSURE_TARGET) | BIT(LIVE_PARAM_VIBRATO_RATE) |
    BIT(LIVE_PARAM_VIBRATO_DEPTH) | BIT(LIVE_PARAM_VIBRATO_DELAY);

static uint32_t packet(uint32_t cin, uint32_t status, uint32_t d1, uint32_t d2) {
    return MIDI_OUT_CABLE << 4 | cin | status << 8 | d1 << 16 | d2 << 24;
}

/* The CC parameter p goes out on, or MIDI_MAP_NO_CC for its NRPN (midi_out.h, "Encoding" and
 * "MIDI learn"). */
static uint32_t wire_cc(const midi_out* o, const live* l, uint32_t p, const midi_map_entry* e) {
    if (o->learn) { /* the bytes directly: this runs for every parameter in a tick */
        uint8_t learned = o->learn->param_cc[p];
        if (learned != MIDI_MAP_NO_CC) {
            return learned;
        }
        if (e->cc < 128u && o->learn->cc_row[e->cc] != MIDI_LEARN_NONE) {
            return MIDI_MAP_NO_CC; /* another parameter learned its CC */
        }
    }
    if (e->cc == MIDI_MAP_NO_CC) {
        return MIDI_MAP_NO_CC;
    }
    if (e->cc == LIVE_CC_MOD_WHEEL && l->mod_target != LIVE_MOD_PULSE_WIDTH) {
        return MIDI_MAP_NO_CC; /* CC 1 moves the wheel, not the pulse width */
    }
    return o->hires && e->max > 127u ? MIDI_MAP_NO_CC : e->cc;
}

static void resync(midi_out* o, const live* l) {
    live_params(l, o->seen);
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        o->wire[p] = o->seen[p];
        o->pending[p] = 0;
    }
}

/* What changed since the last look: the panel's (remote 0) or MIDI input's (remote 1). Every
 * parameter in one snapshot (live_params), not a live_param_get each: a CC flood pays for two
 * looks a pass. */
static void look(midi_out* o, const live* l, int remote) {
    uint16_t now[LIVE_PARAM_COUNT];
    live_params(l, now);
    if (l->preset != o->preset_seen) {
        /* A preset change: the program change carries every parameter it loaded. The rest
         * are diffed as usual below, so a pending change of one survives it. */
        o->preset_seen = l->preset;
        o->preset_pending = remote ? 0u : 1u;
        for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
            if (PRESET_LOADS >> p & 1u) {
                o->seen[p] = now[p];
                o->wire[p] = now[p];
                o->pending[p] = 0;
            }
        }
    }
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        uint16_t v = now[p];
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
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        o->row[p] = midi_map_for_param(p);
    }
    resync(o, l);
    o->preset_seen = l->preset;
    o->preset_pending = 0;
    o->channel = channel & 0x0fu;
    o->hires = hires ? 1u : 0u;
    o->next = 0;
    o->packets = 0;
    o->deferred = 0;
    o->dropped = 0;
    o->learn = NULL;
}

void midi_out_learn(midi_out* o, const midi_learn* learn) {
    o->learn = learn;
}

void midi_out_local(midi_out* o, const live* l) {
    look(o, l, 0);
}

void midi_out_remote(midi_out* o, const live* l) {
    look(o, l, 1);
}

/* Parameter p's pending change into out[*n..], within budget. 1: done, sent or with nothing to
 * send; 0: no room, it stays pending. */
static int send(midi_out* o, const live* l, uint32_t p, uint32_t budget, uint32_t* out,
                uint32_t* n) {
    const midi_map_entry* e = o->row[p]; /* midi_map_for_param searches the table */
    uint32_t v = o->seen[p]; /* the latest: look just ran */
    if (v == o->wire[p]) {
        o->pending[p] = 0; /* back where it was: nothing new however it would go */
        return 1;
    }
    uint32_t on = e ? wire_cc(o, l, p, e) : MIDI_MAP_NO_CC;
    int nrpn = on == MIDI_MAP_NO_CC;
    if (!e || (nrpn && e->nrpn == MIDI_MAP_NO_NRPN) ||
        (nrpn ? v == o->wire[p] : midi_map_to_cc(e, v) == midi_map_to_cc(e, o->wire[p]))) {
        o->pending[p] = 0; /* nothing to send it as, or nothing new on the wire */
        return 1;
    }
    if (*n + (nrpn ? MIDI_OUT_NRPN_PACKETS : 1u) > budget) {
        return 0;
    }
    uint32_t cc = STATUS_CC | o->channel;
    if (nrpn) {
        out[(*n)++] = packet(CIN_CC, cc, CC_NRPN_MSB, (uint32_t)e->nrpn >> 7);
        out[(*n)++] = packet(CIN_CC, cc, CC_NRPN_LSB, e->nrpn & 0x7fu);
        out[(*n)++] = packet(CIN_CC, cc, CC_DATA_MSB, v >> 7);
        out[(*n)++] = packet(CIN_CC, cc, CC_DATA_LSB, v & 0x7fu);
    } else {
        out[(*n)++] = packet(CIN_CC, cc, on, midi_map_to_cc(e, v));
    }
    o->wire[p] = (uint16_t)v;
    o->pending[p] = 0;
    return 1;
}

uint32_t midi_out_tick(midi_out* o, const live* l, uint32_t room,
                       uint32_t out[MIDI_OUT_TICK_PACKETS]) {
    look(o, l, 0);
    uint32_t budget = room < MIDI_OUT_TICK_PACKETS ? room : MIDI_OUT_TICK_PACKETS;
    uint32_t n = 0;
    if (o->preset_pending) {
        if (budget == 0) {
            o->deferred++;
            return 0; /* no parameter before the program change */
        }
        out[n++] = packet(CIN_PROGRAM, STATUS_PROGRAM | o->channel, o->preset_seen, 0);
        o->preset_pending = 0;
    }
    /* The mod target next: it says what CC 1 is, so the receiver must have it before the
     * pulse width goes out on CC 1 (or the wheel on another target). Nothing goes before it. */
    if (o->pending[LIVE_PARAM_MOD_TARGET] && !send(o, l, LIVE_PARAM_MOD_TARGET, budget, out, &n)) {
        o->deferred++;
        o->packets += n;
        return n;
    }
    for (uint32_t i = 0; i < LIVE_PARAM_COUNT; ++i) {
        uint32_t p = (o->next + i) % LIVE_PARAM_COUNT;
        if (o->pending[p] && !send(o, l, p, budget, out, &n)) {
            o->next = (uint8_t)p; /* the next tick starts here */
            o->deferred++;
            break;
        }
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
