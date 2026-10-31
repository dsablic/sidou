// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_out.h"

#include <stddef.h>

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
_Static_assert(MIDI_OUT_TICK_PACKETS / 2u >= MIDI_OUT_NRPN_PACKETS, "a cold expert tick can send an NRPN");
_Static_assert(LIVE_PARAM_COUNT <= 64u, "PRESET_LOADS is a uint64_t");
_Static_assert(LIVE_PARAM_EXPERT % 2u == 0, "the frozen legacy snapshot consists of parameter pairs");
_Static_assert(offsetof(midi_out, seen) % 4u == 0, "snapshot parameter pairs are word-aligned");
_Static_assert(MIDI_MAP_ROWS <= MIDI_OUT_NO_ROW, "midi_out.row is a uint8_t index, MIDI_OUT_NO_ROW none");

#define BIT(p) ((uint64_t)1 << (p))
static const uint64_t PRESET_LOADS =
    BIT(LIVE_PARAM_CUTOFF) | BIT(LIVE_PARAM_RESONANCE) | BIT(LIVE_PARAM_PULSE_WIDTH) |
    BIT(LIVE_PARAM_WAVEFORM) | BIT(LIVE_PARAM_RING) | BIT(LIVE_PARAM_SYNC) |
    BIT(LIVE_PARAM_ATTACK) | BIT(LIVE_PARAM_DECAY) | BIT(LIVE_PARAM_SUSTAIN) |
    BIT(LIVE_PARAM_RELEASE) | BIT(LIVE_PARAM_FILTER_MODE) | BIT(LIVE_PARAM_FILTER_VOICES) |
    BIT(LIVE_PARAM_BEND_RANGE) | BIT(LIVE_PARAM_VELOCITY_TARGET) | BIT(LIVE_PARAM_MOD_TARGET) |
    BIT(LIVE_PARAM_PRESSURE_TARGET) | BIT(LIVE_PARAM_VIBRATO_RATE) |
    BIT(LIVE_PARAM_VIBRATO_DEPTH) | BIT(LIVE_PARAM_VIBRATO_DELAY) | BIT(LIVE_PARAM_PWM_DEPTH) |
    BIT(LIVE_PARAM_PWM_RATE) | BIT(LIVE_PARAM_TUNE) | BIT(LIVE_PARAM_FILTER_LFO) |
    BIT(LIVE_PARAM_ARP_RATE) | BIT(LIVE_PARAM_RESO_LINK) | BIT(LIVE_PARAM_PWM_LINK);

#define EXPERT(p) ((uint32_t)(p) - LIVE_PARAM_EXPERT < SID_EXPERT_COUNT)

static const uint16_t macro_releases[LIVE_PARAM_EXPERT] = {
    [LIVE_PARAM_MODEL] = (1u << SID_EXPERT_COUNT) - 1u,
    [LIVE_PARAM_PROFILE] = (1u << SID_EXPERT_COUNT) - 1u,
    [LIVE_PARAM_AGE] = (1u << SID_EXPERT_FILTER_6581_CURVE) | (1u << SID_EXPERT_FILTER_6581_RANGE) |
                       (1u << SID_EXPERT_FILTER_8580_CURVE) | (1u << SID_EXPERT_NONLINEARITY),
    [LIVE_PARAM_DAC] = (1u << SID_EXPERT_DAC_KINK) | (1u << SID_EXPERT_DAC_LEAKAGE),
    [LIVE_PARAM_THUMP] = 1u << SID_EXPERT_THUMP,
    [LIVE_PARAM_BLEED] = 1u << SID_EXPERT_BLEED,
};

_Static_assert(SID_EXPERT_COUNT <= 16u, "pending experts fit their mask");
_Static_assert(LIVE_PARAM_COUNT - LIVE_PARAM_EXPERT <= 32u,
               "a parameter past the experts clears a bit of the mask no expert has, in range");

static uint32_t packet(uint32_t cin, uint32_t status, uint32_t d1, uint32_t d2) {
    return MIDI_OUT_CABLE << 4 | cin | status << 8 | d1 << 16 | d2 << 24;
}

static uint32_t wire_cc(const midi_out* o, const live* l, uint32_t p, const midi_map_entry* e) {
    if (o->learn) {
        uint8_t learned = o->learn->param_cc[p];
        if (learned != MIDI_MAP_NO_CC) {
            return learned;
        }
        if (e->cc < 128u && o->learn->cc_row[e->cc] != MIDI_LEARN_NONE) {
            return MIDI_MAP_NO_CC;
        }
    }
    if (e->cc == MIDI_MAP_NO_CC) {
        return MIDI_MAP_NO_CC;
    }
    if (e->cc == LIVE_CC_MOD_WHEEL && l->mod_target != LIVE_MOD_PULSE_WIDTH) {
        return MIDI_MAP_NO_CC;
    }
    return o->hires && e->max > 127u ? MIDI_MAP_NO_CC : e->cc;
}

static void resync(midi_out* o, const live* l) {
    live_params_wire(l, o->seen);
    o->pwm_step = l->pwm_rate;
    o->expert_seen = l->expert_version;
    o->expert_pending = 0;
    o->release_pending = 0;
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        o->wire[p] = o->seen[p];
        o->pending[p] = 0;
    }
}

__attribute__((noinline, unused)) static void restore_experts(midi_out* o, const live* l,
                                                            uint32_t released) {
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        uint32_t p = LIVE_PARAM_EXPERT + f;
        if ((released & (1u << f)) && l->expert[f] != SID_EXPERT_DEFAULT) {
            o->pending[p] = 1;
            o->wire[p] = SID_EXPERT_DEFAULT;
            o->expert_pending |= (1u << f);
        }
    }
}

__attribute__((noinline, unused)) static void look_experts(midi_out* o, const live* l, int remote,
                                                         uint32_t released) {
    uint32_t follows = 0;
    o->expert_seen = l->expert_version;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; ++f) {
        uint32_t p = LIVE_PARAM_EXPERT + f;
        uint16_t v = l->expert[f];
        if (v == o->seen[p]) {
            continue;
        }
        if (v == SID_EXPERT_DEFAULT) {
            follows |= (1u << f);
        }
        o->seen[p] = v;
        if (remote || v == SID_EXPERT_DEFAULT) {
            o->wire[p] = v;
            o->pending[p] = 0;
            o->expert_pending &= (uint16_t)~(1u << f);
        } else {
            o->pending[p] = 1;
            o->expert_pending |= (1u << f);
        }
    }
    if (!remote) {
        o->release_pending |= (uint16_t)follows;
    }
    if (released && !remote && l->expert_active) {
        restore_experts(o, l, released);
    }
}

__attribute__((noinline, unused)) static void carry_releases(midi_out* o, const live* l) {
    uint32_t carried = 0;
    for (uint32_t i = 0; i < LIVE_CHAR_FIELDS + 1u; i++) {
        uint32_t p = i ? LIVE_PARAM_PROFILE + i - 1u : LIVE_PARAM_MODEL;
        if (o->pending[p] && o->seen[p] != o->wire[p]) {
            carried |= macro_releases[p];
        }
    }
    if (o->release_pending & ~carried) {
        o->pending[LIVE_PARAM_PROFILE] = 1;
        o->wire[LIVE_PARAM_PROFILE] = UINT16_MAX;
        if (l->expert_active) {
            restore_experts(o, l, (1u << SID_EXPERT_COUNT) - 1u);
        }
    }
}

static void look_one(midi_out* o, uint32_t p, uint16_t v, int remote) {
    o->seen[p] = v;
    if (remote) {
        o->wire[p] = v;
        o->pending[p] = 0;
    } else {
        o->pending[p] = 1;
    }
}

static int look(midi_out* o, const live* l, int remote) {
    _Alignas(4) uint16_t now[LIVE_PARAM_COUNT];
    uint32_t released = 0;
    live_params_base(l, now);
    int tail = l->pwm_depth != o->seen[LIVE_PARAM_PWM_DEPTH] || l->pwm_rate != o->pwm_step ||
               (uint32_t)(l->tune + LIVE_TUNE_CENTS) != o->seen[LIVE_PARAM_TUNE] ||
               l->filter_lfo != o->seen[LIVE_PARAM_FILTER_LFO] ||
               l->arp_rate != o->seen[LIVE_PARAM_ARP_RATE] ||
               (uint32_t)(l->reso_link + LIVE_LINK_MAX) != o->seen[LIVE_PARAM_RESO_LINK] ||
               (uint32_t)(l->pwm_link + LIVE_LINK_MAX) != o->seen[LIVE_PARAM_PWM_LINK];
    if (tail || l->preset != o->preset_seen) {
        live_params_tail(l, now);
        o->pwm_step = l->pwm_rate;
    }
    if (l->preset != o->preset_seen) {
        o->preset_seen = l->preset;
        o->preset_pending = remote || l->preset == LIVE_PRESET_DICE ? 0u : 1u;
        _Alignas(4) uint16_t factory[LIVE_PARAM_COUNT];
        live_preset_params(l->preset, factory);
        for (uint32_t p = 0; p < LIVE_PARAM_COUNT && l->preset != LIVE_PRESET_DICE; ++p) {
            if (PRESET_LOADS >> p & 1u) {
                uint16_t carried = remote || p == LIVE_PARAM_BEND_RANGE ? now[p] : factory[p];
                o->seen[p] = now[p];
                o->wire[p] = carried;
                o->pending[p] = now[p] != carried;
            }
        }
    }
    for (uint32_t pair = 0; pair < LIVE_PARAM_EXPERT / 2u; ++pair) {
        uint32_t p = pair * 2u;
        uint32_t current, previous;
        __builtin_memcpy(&current, &now[p], sizeof current);
        __builtin_memcpy(&previous, &o->seen[p], sizeof previous);
        if (current == previous) {
            continue;
        }
        for (uint32_t end = p + 2u; p < end; ++p) {
            uint16_t v = now[p];
            if (v == o->seen[p]) {
                continue;
            }
            if (p >= LIVE_PARAM_MODEL) {
                released |= macro_releases[p];
            }
            look_one(o, p, v, remote);
        }
    }
    for (uint32_t p = LIVE_PARAM_EXPERT_END; tail && p < LIVE_PARAM_COUNT; ++p) {
        if (now[p] != o->seen[p]) {
            look_one(o, p, now[p], remote);
        }
    }
    int expert_work = o->expert_seen != l->expert_version || (released && l->expert_active);
    if (expert_work) {
        look_experts(o, l, remote, released);
    }
    return expert_work;
}

void midi_out_init(midi_out* o, const live* l, uint8_t channel, uint8_t hires) {
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        const midi_map_entry* e = midi_map_for_param(p);
        o->row[p] = e ? (uint8_t)(e - midi_map_rows) : (uint8_t)MIDI_OUT_NO_ROW;
    }
    resync(o, l);
    o->preset_seen = l->preset;
    o->preset_pending = 0;
    o->bank_sent = 0;
    o->song_pending = 0;
    o->song_program = 0;
    o->channel = channel & 0x0fu;
    o->hires = hires ? 1u : 0u;
    o->next = 0;
    o->packets = 0;
    o->deferred = 0;
    o->dropped = 0;
    o->learn = NULL;
}

void midi_out_song(midi_out* o, uint8_t n) {
    o->song_pending = 1;
    o->song_program = (uint8_t)(n & 0x7fu);
}

static int program_change(midi_out* o, uint32_t bank, uint32_t program, uint32_t budget,
                          uint32_t* out, uint32_t* n) {
    uint32_t need = bank != o->bank_sent ? 3u : 1u;
    if (*n + need > budget) {
        return 0;
    }
    if (need == 3u) {
        out[(*n)++] = packet(CIN_CC, STATUS_CC | o->channel, 0u, 0u);
        out[(*n)++] = packet(CIN_CC, STATUS_CC | o->channel, 32u, bank);
        o->bank_sent = (uint8_t)bank;
    }
    out[(*n)++] = packet(CIN_PROGRAM, STATUS_PROGRAM | o->channel, program, 0);
    return 1;
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

static int same_cc(const live* l, const midi_map_entry* e, uint32_t v, uint32_t wire) {
    uint32_t max = e->max;
    if (e->curve == MIDI_CURVE_WALK) {
        return 0;
    }
    if (v <= max && wire <= max) {
        if (max <= 127u) {
            return 0;
        }
        uint32_t apart = v > wire ? v - wire : wire - v;
        if (apart * 127u >= max) {
            return 0;
        }
    }
    return midi_map_to_cc(l, e, v) == midi_map_to_cc(l, e, wire);
}

static int send(midi_out* o, const live* l, uint32_t p, uint32_t budget, uint32_t* out,
                uint32_t* n) {
    uint32_t v = o->seen[p];
    uint32_t wire = o->wire[p];
    if (v == wire || (v == SID_EXPERT_DEFAULT && EXPERT(p))) {
        o->pending[p] = 0;
        return 1;
    }
    uint32_t r = o->row[p];
    if (r >= MIDI_MAP_ROWS) {
        o->pending[p] = 0;
        return 1;
    }
    const midi_map_entry* e = &midi_map_rows[r];
    uint32_t on = wire_cc(o, l, p, e);
    int nrpn = on == MIDI_MAP_NO_CC;
    if ((nrpn && e->nrpn == MIDI_MAP_NO_NRPN) ||
        (!nrpn && !(wire == SID_EXPERT_DEFAULT && EXPERT(p)) && same_cc(l, e, v, wire))) {
        o->pending[p] = 0;
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
        out[(*n)++] = packet(CIN_CC, cc, on, midi_map_to_cc(l, e, v));
    }
    o->wire[p] = (uint16_t)v;
    o->pending[p] = 0;
    if (p >= LIVE_PARAM_MODEL && p < LIVE_PARAM_EXPERT &&
        (l->expert_active || o->release_pending)) {
        uint32_t released = macro_releases[p];
        o->release_pending &= (uint16_t)~released;
        if (released && l->expert_active) {
            restore_experts(o, l, released);
        }
    }
    return 1;
}

uint32_t midi_out_tick(midi_out* o, const live* l, uint32_t room,
                       uint32_t out[MIDI_OUT_TICK_PACKETS]) {
    int expert_work = look(o, l, 0);
    if (o->release_pending) {
        carry_releases(o, l);
    }
    int macros = o->expert_pending &&
                 (o->pending[LIVE_PARAM_MODEL] | o->pending[LIVE_PARAM_PROFILE] |
                  o->pending[LIVE_PARAM_AGE] | o->pending[LIVE_PARAM_DAC] |
                  o->pending[LIVE_PARAM_THUMP] | o->pending[LIVE_PARAM_BLEED]);
    uint32_t limit = expert_work || macros ? MIDI_OUT_TICK_PACKETS / 2u : MIDI_OUT_TICK_PACKETS;
    uint32_t budget = room < limit ? room : limit;
    uint32_t n = 0;
    if (o->song_pending) {
        if (!program_change(o, 1u, o->song_program, budget, out, &n)) {
            o->deferred++;
            return 0;
        }
        o->song_pending = 0;
    }
    if (o->preset_pending) {
        if (!program_change(o, 0u, o->preset_seen, budget, out, &n)) {
            o->deferred++;
            o->packets += n;
            return n;
        }
        o->preset_pending = 0;
    }
    if (o->pending[LIVE_PARAM_MOD_TARGET] && !send(o, l, LIVE_PARAM_MOD_TARGET, budget, out, &n)) {
        o->deferred++;
        o->packets += n;
        return n;
    }
    if (o->expert_pending) {
        for (uint32_t i = 0; i < LIVE_CHAR_FIELDS + 1u; i++) {
            uint32_t p = i ? LIVE_PARAM_PROFILE + i - 1u : LIVE_PARAM_MODEL;
            if (o->pending[p] && !send(o, l, p, budget, out, &n)) {
                o->deferred++;
                o->packets += n;
                return n;
            }
        }
    }
    uint32_t from = o->next;
    uint32_t p = from;
    uint32_t end = LIVE_PARAM_COUNT;
    uint8_t* pending = o->pending;
    for (;;) {
        if (p == end) {
            if (end != LIVE_PARAM_COUNT || !from) {
                break;
            }
            p = 0;
            end = from;
        }
        if (pending[p]) {
            if (!send(o, l, p, budget, out, &n)) {
                o->next = (uint8_t)p;
                o->deferred++;
                break;
            }
            if (p >= LIVE_PARAM_EXPERT) {
                o->expert_pending &= (uint16_t)~(1u << (p - LIVE_PARAM_EXPERT));
            }
        }
        p++;
    }
    o->packets += n;
    return n;
}

void midi_out_drop(midi_out* o, const live* l) {
    look(o, l, 0);
    uint32_t n = o->preset_pending + o->song_pending;
    o->preset_pending = 0;
    o->song_pending = 0;
    o->expert_pending = 0;
    o->release_pending = 0;
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        n += o->pending[p];
        o->pending[p] = 0;
    }
    o->dropped += n;
}
