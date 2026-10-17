// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_route.h"

#include <stddef.h>
#include <string.h>

#include "midi_map.h"
#include "sid/sid_character.h"

#define CC_DATA_MSB 6u
#define CC_DATA_LSB 38u
#define CC_INCREMENT 96u
#define CC_DECREMENT 97u
#define CC_NRPN_LSB 98u
#define CC_NRPN_MSB 99u
#define CC_RPN_LSB 100u
#define CC_RPN_MSB 101u
#define CC_MODEL 25u
#define CC_ENVELOPE 26u
#define CC_PROFILE 85u
#define RPN_NONE 127u
#define LOCK_CUTOFF 1u
#define LOCK_RESONANCE 2u
#define CLOCK 0xf8u
#define START 0xfau

/* A channel's role in the mode. */
enum { ROLE_NONE, ROLE_LIVE, ROLE_JAM };

static void roles(midi_route* r);
static void program(midi_route* r, uint32_t ch, uint32_t k, uint32_t jam, uint8_t n);

static const char* const MODE_NAMES[MIDI_MODE_COUNT] = {"LIVE", "MULTI", "WHOLE A", "WHOLE B",
                                                        "OMNI"};
static const char* const OPTION_NAMES[3] = {"POLY", "CHORD", "ARP"};

const char* midi_route_mode_name(uint32_t mode) {
    return mode < MIDI_MODE_COUNT ? MODE_NAMES[mode] : "?";
}

const char* midi_route_option_name(uint32_t option) {
    return option < 3u ? OPTION_NAMES[option] : "?";
}

#ifdef SIDOU_MIDI_MAP_DOC
const char* midi_route_mode_doc(uint32_t mode) {
    static const char* const DOC[MIDI_MODE_COUNT] = {
        "channel N plays the instrument, on the chip the keys play; the other channels are "
        "ignored",
        "channels N, N+1 and N+2 play chip A's voices 1, 2 and 3, each a jam voice of the "
        "player with its own GoatTracker instrument, pitch bend and notes (one at a time, "
        "the last held sounding; LEGATO glides between them); N+3 plays the instrument on "
        "chip B",
        "channel N plays chip A's three voices with one GoatTracker instrument: POLY (three "
        "voices, the oldest note stolen), CHORD (one key plays the stored chord) or ARP (the "
        "held notes cycled on voice 1, every few frames or every 16th of the clock); N+3 "
        "plays the instrument on chip B",
        "channel N plays the instrument on chip B; chip A is the song's",
        "every channel plays the instrument, on the chip the keys play (the default, M1's)",
    };
    return mode < MIDI_MODE_COUNT ? DOC[mode] : "";
}

const char* midi_route_bank_doc(uint32_t bank) {
    static const char* const DOC[MIDI_ROUTE_BANKS] = {
        "the channel's sound: on the instrument's channel its preset 0..3 (M1's); on a chip A "
        "channel (MULTI, WHOLE A) GoatTracker instrument n + 1, 0..62",
        "songs: program n loads the store's n-th song in name order, stopped; on the "
        "instrument's channel or channel N only",
        "the chip's profile: 0 6581 R2, 1 6581 R3, 2 6581 R4AR, 3 8580 R5, 4 worn 6581, 5..12 "
        "USER 1..8; the instrument's (the keys' setup) on its channel, chip A's on chip A's",
    };
    return bank < MIDI_ROUTE_BANKS ? DOC[bank] : "";
}
#endif

void midi_route_setup_default(midi_route_setup* s) {
    s->mode = MIDI_MODE_OMNI;
    s->base = 0;
    s->rows_per_beat = MIDI_ROUTE_ROWS_DEFAULT;
    s->legato = 0;
    s->option = MIDI_WHOLE_POLY;
    s->chord[0] = MIDI_ROUTE_CHORD_DEFAULT_2;
    s->chord[1] = MIDI_ROUTE_CHORD_DEFAULT_3;
    s->arp_rate = MIDI_ROUTE_ARP_DEFAULT;
}

static uint8_t clamp(uint32_t v, uint32_t lo, uint32_t hi) {
    return (uint8_t)(v < lo ? lo : v > hi ? hi : v);
}

static void clamp_setup(midi_route_setup* s) {
    if (s->mode >= MIDI_MODE_COUNT) {
        s->mode = MIDI_MODE_OMNI;
    }
    s->base &= 15u;
    s->rows_per_beat = clamp(s->rows_per_beat, 1u, MIDI_ROUTE_ROWS_MAX);
    s->legato = s->legato ? 1u : 0u;
    if (s->option > MIDI_WHOLE_ARP) {
        s->option = MIDI_WHOLE_POLY;
    }
    s->chord[0] = clamp(s->chord[0], 0u, MIDI_ROUTE_CHORD_MAX);
    s->chord[1] = clamp(s->chord[1], 0u, MIDI_ROUTE_CHORD_MAX);
    s->arp_rate = clamp(s->arp_rate, 0u, MIDI_ROUTE_ARP_MAX);
}

int midi_route_owns_chip_a(uint32_t mode) {
    return mode == MIDI_MODE_MULTI || mode == MIDI_MODE_WHOLE_A || mode == MIDI_MODE_WHOLE_B;
}

/* ---- the player ---- */

static int p_on(midi_route* r, uint32_t v, uint32_t note, uint32_t instr, uint32_t legato) {
    return r->player.jam_on ? r->player.jam_on(r->player.ctx, v, note, instr, legato) : 1;
}

static int p_off(midi_route* r, uint32_t v) {
    return r->player.jam_off ? r->player.jam_off(r->player.ctx, v) : 1;
}

/* Voice v sounds MIDI note n (in GoatTracker's range: the callers check) with instr. */
static void sound(midi_route* r, uint32_t v, uint8_t n, uint32_t instr, uint32_t legato) {
    if (!p_on(r, v, (uint32_t)n - MIDI_ROUTE_GT_FIRST, instr, legato)) {
        r->dropped++;
        return;
    }
    r->v[v].note = n;
    r->v[v].stamp = ++r->stamp;
}

static void silence(midi_route* r, uint32_t v) {
    if (r->v[v].note == MIDI_ROUTE_NO_NOTE) {
        return;
    }
    if (p_off(r, v)) {
        r->v[v].note = MIDI_ROUTE_NO_NOTE;
    } else {
        r->dropped++; /* the voice still sounds: its next jam_off or jam_on ends it */
    }
}

static int in_range(uint32_t n) {
    return n >= MIDI_ROUTE_GT_FIRST && n < MIDI_ROUTE_GT_FIRST + MIDI_ROUTE_GT_NOTES;
}

/* ---- held notes ---- */

static void held_remove(midi_route_chan* c, uint8_t n) {
    uint32_t k = 0;
    for (uint32_t i = 0; i < c->n; i++) {
        if (c->held[i] != n) {
            c->held[k++] = c->held[i];
        }
    }
    c->n = (uint8_t)k;
}

static void held_push(midi_route_chan* c, uint8_t n) {
    held_remove(c, n);
    if (c->n == MIDI_ROUTE_HELD) {
        for (uint32_t i = 1; i < MIDI_ROUTE_HELD; i++) {
            c->held[i - 1u] = c->held[i];
        }
        c->n--;
    }
    c->held[c->n++] = n;
}

/* ---- the setups ---- */

static void chan_reset(midi_route_chan* c) {
    c->n = 0;
    c->bend = LIVE_BEND_CENTRE;
    c->rpn_msb = RPN_NONE;
    c->rpn_lsb = RPN_NONE;
}

static void release_all(midi_route* r) {
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
        silence(r, v);
        r->v[v].note = MIDI_ROUTE_NO_NOTE; /* a new mode starts with none */
    }
    for (uint32_t c = 0; c < MIDI_ROUTE_VOICES; c++) {
        r->ch[c].n = 0;
    }
}

void midi_route_init(midi_route* r, live* l, keys_route* kr, const midi_route_player* player) {
    r->l = l;
    r->kr = kr;
    if (player) {
        r->player = *player;
    } else {
        r->player.jam_on = NULL;
        r->player.jam_off = NULL;
        r->player.jam_bend = NULL;
        r->player.filter = NULL;
        r->player.ctx = NULL;
    }
    midi_route_setup_default(&r->setup);
    roles(r);
    for (uint32_t c = 0; c < MIDI_ROUTE_VOICES; c++) {
        chan_reset(&r->ch[c]);
        r->ch[c].instr = 1;
        r->ch[c].range = LIVE_BEND_RANGE_DEFAULT;
        r->v[c].note = MIDI_ROUTE_NO_NOTE;
        r->v[c].stamp = 0;
        r->v[c].bend_sent = 0;
    }
    r->locks = 0;
    r->locks_sent = 0;
    r->cutoff = 0;
    r->resonance = 0;
    r->filter_dirty = 0;
    r->arp_pos = 0;
    r->arp_wait = 0;
    r->stamp = 0;
    r->dropped = 0;
    for (uint32_t ch = 0; ch < 16u; ch++) {
        r->bank_msb[ch] = 0;
        r->bank_lsb[ch] = 0;
    }
    r->song_req = MIDI_ROUTE_NO_REQUEST;
    r->user_req = MIDI_ROUTE_NO_REQUEST;
    r->user_chip = MIDI_ROUTE_CHIP_KEYS;
}

void midi_route_set(midi_route* r, const midi_route_setup* s) {
    midi_route_setup n = *s;
    clamp_setup(&n);
    const midi_route_setup* o = &r->setup;
    int same = n.mode == o->mode && n.base == o->base && n.rows_per_beat == o->rows_per_beat &&
               n.legato == o->legato && n.option == o->option && n.chord[0] == o->chord[0] &&
               n.chord[1] == o->chord[1] && n.arp_rate == o->arp_rate;
    if (same) {
        return;
    }
    release_all(r);
    for (uint32_t c = 0; c < MIDI_ROUTE_VOICES; c++) {
        chan_reset(&r->ch[c]);
    }
    if (r->locks || r->locks_sent) {
        r->locks = 0;
        r->filter_dirty = 1;
    }
    r->arp_pos = 0;
    r->arp_wait = 0;
    live_all_notes_off(r->l); /* a note on a channel that is no longer the instrument's */
    r->setup = n;
    roles(r);
    if (r->kr) {
        keys_route_set_player(r->kr, midi_route_owns_chip_a(n.mode));
    }
}

void midi_route_reset_input(midi_route* r) {
    release_all(r);
    for (uint32_t c = 0; c < MIDI_ROUTE_VOICES; c++) {
        chan_reset(&r->ch[c]);
    }
}

/* ---- classification ---- */

/* The channel's role, and for a jam channel which (0..2). */
static uint32_t role(const midi_route_setup* s, uint32_t channel, uint32_t* jam) {
    uint32_t d = (channel - s->base) & 15u;
    switch (s->mode) {
    case MIDI_MODE_OMNI: return ROLE_LIVE;
    case MIDI_MODE_LIVE:
    case MIDI_MODE_WHOLE_B: return d == 0u ? ROLE_LIVE : ROLE_NONE;
    case MIDI_MODE_MULTI:
        if (d < MIDI_ROUTE_VOICES) {
            *jam = d;
            return ROLE_JAM;
        }
        return d == MIDI_ROUTE_LIVE_OFFSET ? ROLE_LIVE : ROLE_NONE;
    case MIDI_MODE_WHOLE_A:
        if (d == 0u) {
            *jam = 0;
            return ROLE_JAM;
        }
        return d == MIDI_ROUTE_LIVE_OFFSET ? ROLE_LIVE : ROLE_NONE;
    default: return ROLE_NONE;
    }
}

static void roles(midi_route* r) {
    for (uint32_t ch = 0; ch < 16u; ch++) {
        uint32_t jam = 0;
        uint32_t k = role(&r->setup, ch, &jam);
        r->to[ch] = (uint8_t)(k == ROLE_LIVE ? MIDI_ROUTE_LIVE
                              : k == ROLE_JAM ? MIDI_ROUTE_TAKE
                                              : MIDI_ROUTE_IGNORE);
    }
}

midi_route_to midi_route_classify(const midi_route* r, const midi_msg* msg) {
    if (msg->kind == MIDI_REALTIME) {
        return MIDI_ROUTE_TAKE; /* the ARP's clock; live has no use for it */
    }
    if (msg->kind == MIDI_SONG_POSITION || msg->kind == MIDI_SONG_SELECT) {
        return MIDI_ROUTE_IGNORE; /* the transport's (M2e Task 2) */
    }
    uint8_t to = r->to[msg->channel & 15u];
    if (to == MIDI_ROUTE_LIVE &&
        (msg->kind == MIDI_PROGRAM ||
         (msg->kind == MIDI_CC && (msg->data1 == MIDI_ROUTE_CC_BANK_MSB ||
                                   msg->data1 == MIDI_ROUTE_CC_BANK_LSB)))) {
        return MIDI_ROUTE_TAKE; /* bank select and its program changes */
    }
    return (midi_route_to)to;
}

/* ---- notes ---- */

/* The ARP's or CHORD's: the voices of WHOLE-CHIP A sounding note n. */
static void chord_sound(midi_route* r, uint8_t n, uint32_t legato) {
    const uint8_t iv[MIDI_ROUTE_VOICES] = {0, r->setup.chord[0], r->setup.chord[1]};
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
        uint32_t m = (uint32_t)n + iv[v];
        if (in_range(m)) {
            sound(r, v, (uint8_t)m, r->ch[0].instr,
                  legato && r->v[v].note != MIDI_ROUTE_NO_NOTE);
        } else {
            silence(r, v);
        }
    }
}

static void chord_silence(midi_route* r) {
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
        silence(r, v);
    }
}

/* Mono on voice v, channel c: the last note held sounds. */
static void mono_on(midi_route* r, uint32_t v, midi_route_chan* c, uint8_t n) {
    uint32_t legato = r->setup.legato && r->v[v].note != MIDI_ROUTE_NO_NOTE;
    held_push(c, n);
    sound(r, v, n, c->instr, legato);
}

static void mono_off(midi_route* r, uint32_t v, midi_route_chan* c, uint8_t n) {
    held_remove(c, n);
    if (r->v[v].note != n) {
        return;
    }
    if (c->n) {
        sound(r, v, c->held[c->n - 1u], c->instr, r->setup.legato);
    } else {
        silence(r, v);
    }
}

/* WHOLE-CHIP A, POLY: the voice of the note, a free one, or the oldest. */
static void poly_on(midi_route* r, uint8_t n) {
    uint32_t pick = MIDI_ROUTE_VOICES;
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES && pick == MIDI_ROUTE_VOICES; v++) {
        if (r->v[v].note == n) {
            pick = v;
        }
    }
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES && pick == MIDI_ROUTE_VOICES; v++) {
        if (r->v[v].note == MIDI_ROUTE_NO_NOTE) {
            pick = v;
        }
    }
    if (pick == MIDI_ROUTE_VOICES) {
        pick = 0;
        for (uint32_t v = 1; v < MIDI_ROUTE_VOICES; v++) {
            if (r->v[v].stamp < r->v[pick].stamp) {
                pick = v;
            }
        }
    }
    sound(r, pick, n, r->ch[0].instr, 0);
}

static void poly_off(midi_route* r, uint8_t n) {
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
        if (r->v[v].note == n) {
            silence(r, v);
        }
    }
}

static void arp_step(midi_route* r) {
    midi_route_chan* c = &r->ch[0];
    if (!c->n) {
        return;
    }
    r->arp_pos = (uint8_t)((r->arp_pos + 1u) % c->n);
    sound(r, 0, c->held[r->arp_pos], c->instr, 0);
}

static void arp_rewind(midi_route* r) {
    r->arp_wait = r->setup.arp_rate ? (uint8_t)(r->setup.arp_rate * MIDI_ROUTE_TICKS_PER_FRAME)
                                    : (uint8_t)MIDI_ROUTE_ARP_CLOCKS;
}

static void arp_on(midi_route* r, uint8_t n) {
    midi_route_chan* c = &r->ch[0];
    int first = c->n == 0u;
    held_push(c, n);
    if (first) {
        r->arp_pos = 0;
        arp_rewind(r);
        sound(r, 0, n, c->instr, 0);
    }
}

static void arp_off(midi_route* r, uint8_t n) {
    midi_route_chan* c = &r->ch[0];
    held_remove(c, n);
    if (!c->n) {
        silence(r, 0);
    } else if (r->arp_pos >= c->n) {
        r->arp_pos = (uint8_t)(c->n - 1u); /* the next step wraps to the first */
    }
}

static void note_on(midi_route* r, uint32_t jam, uint8_t n) {
    if (!in_range(n)) {
        return;
    }
    if (r->setup.mode == MIDI_MODE_MULTI) {
        mono_on(r, jam, &r->ch[jam], n);
        return;
    }
    switch (r->setup.option) {
    case MIDI_WHOLE_CHORD: {
        uint32_t legato = r->setup.legato && r->ch[0].n;
        held_push(&r->ch[0], n);
        chord_sound(r, n, legato);
        break;
    }
    case MIDI_WHOLE_ARP: arp_on(r, n); break;
    default: poly_on(r, n); break;
    }
}

static void note_off(midi_route* r, uint32_t jam, uint8_t n) {
    if (!in_range(n)) {
        return;
    }
    if (r->setup.mode == MIDI_MODE_MULTI) {
        mono_off(r, jam, &r->ch[jam], n);
        return;
    }
    midi_route_chan* c = &r->ch[0];
    switch (r->setup.option) {
    case MIDI_WHOLE_CHORD: {
        int top = c->n && c->held[c->n - 1u] == n;
        held_remove(c, n);
        if (!top) {
            break;
        }
        if (c->n) {
            chord_sound(r, c->held[c->n - 1u], r->setup.legato);
        } else {
            chord_silence(r);
        }
        break;
    }
    case MIDI_WHOLE_ARP: arp_off(r, n); break;
    default: poly_off(r, n); break;
    }
}

/* The jam channel's voices: its own in MULTITIMBRAL, all three in WHOLE-CHIP A. */
static uint32_t voices_of(const midi_route* r, uint32_t jam) {
    return r->setup.mode == MIDI_MODE_MULTI ? 1u << jam : 7u;
}

static void channel_off(midi_route* r, uint32_t jam) {
    uint32_t m = voices_of(r, jam);
    for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
        if ((m >> v) & 1u) {
            silence(r, v);
        }
    }
    r->ch[jam].n = 0;
}

/* ---- controllers ---- */

/* Chip A's setup, as the song wants it, changed by a chip-level CC. */
static void chip_cc(midi_route* r, uint8_t cc, uint8_t value) {
    if (!r->kr) {
        return;
    }
    const midi_map_entry* e = midi_map_for_cc(cc);
    if (!e || e->param == MIDI_MAP_RESERVED) {
        return;
    }
    chip_setup s = r->kr->song_want;
    uint32_t v = midi_map_from_cc(e, value);
    switch (e->param) {
    case LIVE_PARAM_PROFILE: s.profile = (uint8_t)v; break;
    case LIVE_PARAM_MODEL: {
        const sid_chip_profile* p = sid_chip_profile_get(s.profile);
        sid_model now = p ? p->model : SID_MODEL_6581;
        if ((uint32_t)now != v) {
            s.profile = v == SID_MODEL_8580 ? SID_CHIP_8580_R5 : SID_CHIP_6581_R4AR;
        }
        break;
    }
    case LIVE_PARAM_ENVELOPE_MODE: s.gate_reset = v == LIVE_ENVELOPE_INSTANT; break;
    case LIVE_PARAM_AGE:
    case LIVE_PARAM_DAC:
    case LIVE_PARAM_THUMP:
    case LIVE_PARAM_BLEED: s.controls[e->param - LIVE_PARAM_AGE] = (uint8_t)v; break;
    default: return;
    }
    keys_route_song_wants(r->kr, &s);
}

static void filter_cc(midi_route* r, uint8_t cc, uint8_t value) {
    const midi_map_entry* e = midi_map_for_cc(cc);
    uint32_t v = midi_map_from_cc(e, value);
    if (cc == LIVE_CC_CUTOFF) {
        r->cutoff = (uint16_t)v;
        r->locks |= LOCK_CUTOFF;
    } else {
        r->resonance = (uint8_t)v;
        r->locks |= LOCK_RESONANCE;
    }
    r->filter_dirty = 1;
}

/* RPN 0, the channel's bend range (the others do nothing): as midi_map's for live. */
static void rpn_data(midi_route_chan* c, uint8_t cc, uint8_t value) {
    if (c->rpn_msb != 0u || c->rpn_lsb != 0u) {
        return;
    }
    uint32_t now = c->range;
    switch (cc) {
    case CC_DATA_MSB: now = value; break;
    case CC_INCREMENT: now++; break;
    case CC_DECREMENT: now = now ? now - 1u : 0u; break;
    default: return; /* the cents */
    }
    c->range = clamp(now, 0u, LIVE_BEND_RANGE_MAX);
}

static void jam_cc(midi_route* r, uint32_t jam, uint8_t cc, uint8_t value) {
    midi_route_chan* c = &r->ch[jam];
    switch (cc) {
    case CC_RPN_MSB: c->rpn_msb = value; break;
    case CC_RPN_LSB: c->rpn_lsb = value; break;
    case CC_NRPN_MSB:
    case CC_NRPN_LSB:
        c->rpn_msb = RPN_NONE; /* data entry is an NRPN's now: none of the jam channel's */
        c->rpn_lsb = RPN_NONE;
        break;
    case CC_DATA_MSB:
    case CC_DATA_LSB:
    case CC_INCREMENT:
    case CC_DECREMENT: rpn_data(c, cc, value); break;
    case LIVE_CC_CUTOFF:
    case LIVE_CC_RESONANCE: filter_cc(r, cc, value); break;
    case LIVE_CC_ALL_SOUND_OFF:
    case LIVE_CC_ALL_NOTES_OFF: channel_off(r, jam); break;
    case LIVE_CC_RESET_ALL:
        c->bend = LIVE_BEND_CENTRE;
        c->rpn_msb = RPN_NONE;
        c->rpn_lsb = RPN_NONE;
        if (r->locks) {
            r->locks = 0;
            r->filter_dirty = 1;
        }
        break;
    case CC_MODEL:
    case CC_ENVELOPE:
    case CC_PROFILE:
    case 86:
    case 87:
    case 89:
    case 90: chip_cc(r, cc, value); break;
    default: break;
    }
}

/* ---- program change and banks ---- */

static uint32_t bank_of(const midi_route* r, uint32_t ch) {
    return (uint32_t)r->bank_msb[ch] << 7 | r->bank_lsb[ch];
}

int midi_route_edits_live(const midi_route* r, const midi_msg* msg) {
    if (msg->kind != MIDI_PROGRAM || r->to[msg->channel & 15u] != MIDI_ROUTE_LIVE) {
        return 0;
    }
    switch (bank_of(r, msg->channel & 15u)) {
    case MIDI_ROUTE_BANK_SOUND: return msg->data1 < LIVE_PRESET_COUNT;
    case MIDI_ROUTE_BANK_PROFILES: return msg->data1 < SID_CHIPS;
    default: return 0;
    }
}

static void program(midi_route* r, uint32_t ch, uint32_t k, uint32_t jam, uint8_t n) {
    if (k == ROLE_NONE) {
        return;
    }
    switch (bank_of(r, ch)) {
    case MIDI_ROUTE_BANK_SOUND:
        if (k == ROLE_JAM) {
            if (n < MIDI_ROUTE_INSTR_MAX) {
                r->ch[jam].instr = (uint8_t)(n + 1u); /* from the next note */
            }
        } else if (n < LIVE_PRESET_COUNT) {
            live_set_preset(r->l, n);
        }
        break;
    case MIDI_ROUTE_BANK_SONGS:
        if (k == ROLE_LIVE || ch == r->setup.base) {
            r->song_req = n; /* the latest wins */
        }
        break;
    case MIDI_ROUTE_BANK_PROFILES:
        if (n < SID_CHIPS) {
            if (k == ROLE_LIVE) {
                live_param_set(r->l, LIVE_PARAM_PROFILE, n);
            } else if (r->kr) {
                chip_setup s = r->kr->song_want;
                s.profile = n;
                keys_route_song_wants(r->kr, &s);
            }
        } else if (n < SID_CHIPS + MIDI_ROUTE_USER_PROFILES) {
            r->user_req = (uint8_t)(n - SID_CHIPS);
            r->user_chip = k == ROLE_LIVE ? MIDI_ROUTE_CHIP_KEYS : MIDI_ROUTE_CHIP_A;
        }
        break;
    default: break;
    }
}

int32_t midi_route_take_song(midi_route* r) {
    int32_t n = r->song_req == MIDI_ROUTE_NO_REQUEST ? -1 : (int32_t)r->song_req;
    r->song_req = MIDI_ROUTE_NO_REQUEST;
    return n;
}

int32_t midi_route_take_user_profile(midi_route* r, uint32_t* chip) {
    int32_t n = r->user_req == MIDI_ROUTE_NO_REQUEST ? -1 : (int32_t)r->user_req;
    *chip = r->user_chip;
    r->user_req = MIDI_ROUTE_NO_REQUEST;
    return n;
}

void midi_route_take(midi_route* r, const midi_msg* msg) {
    if (msg->kind == MIDI_REALTIME) {
        if (r->setup.mode != MIDI_MODE_WHOLE_A || r->setup.option != MIDI_WHOLE_ARP ||
            r->setup.arp_rate != 0u) {
            return;
        }
        if (msg->data1 == START) {
            r->arp_wait = MIDI_ROUTE_ARP_CLOCKS;
        } else if (msg->data1 == CLOCK && r->arp_wait && --r->arp_wait == 0u) {
            arp_step(r);
            arp_rewind(r);
        }
        return;
    }
    uint32_t ch = msg->channel & 15u;
    if (msg->kind == MIDI_CC && msg->data1 == MIDI_ROUTE_CC_BANK_MSB) {
        r->bank_msb[ch] = msg->data2;
        return;
    }
    if (msg->kind == MIDI_CC && msg->data1 == MIDI_ROUTE_CC_BANK_LSB) {
        r->bank_lsb[ch] = msg->data2;
        return;
    }
    uint32_t jam = 0;
    uint32_t k = role(&r->setup, ch, &jam);
    if (msg->kind == MIDI_PROGRAM) {
        program(r, ch, k, jam, msg->data1);
        return;
    }
    if (k != ROLE_JAM) {
        return;
    }
    switch (msg->kind) {
    case MIDI_NOTE_ON: note_on(r, jam, msg->data1); break;
    case MIDI_NOTE_OFF: note_off(r, jam, msg->data1); break;
    case MIDI_PITCH_BEND:
        r->ch[jam].bend = msg->bend > LIVE_BEND_MAX ? (uint16_t)LIVE_BEND_MAX : msg->bend;
        break;
    case MIDI_CC: jam_cc(r, jam, msg->data1, msg->data2); break;
    default: break; /* aftertouch: GoatTracker's instruments have none */
    }
}

/* ---- the tick ---- */

void midi_route_tick(midi_route* r) {
    const midi_route_setup* s = &r->setup;
    if (s->mode == MIDI_MODE_MULTI || s->mode == MIDI_MODE_WHOLE_A) {
        for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
            const midi_route_chan* c = &r->ch[s->mode == MIDI_MODE_MULTI ? v : 0u];
            int32_t off = ((int32_t)c->bend - (int32_t)LIVE_BEND_CENTRE) * (int32_t)c->range;
            if (off != r->v[v].bend_sent &&
                (!r->player.jam_bend || r->player.jam_bend(r->player.ctx, v, off))) {
                r->v[v].bend_sent = off;
            }
        }
        if (s->mode == MIDI_MODE_WHOLE_A && s->option == MIDI_WHOLE_ARP && s->arp_rate &&
            r->arp_wait && --r->arp_wait == 0u) {
            arp_step(r);
            arp_rewind(r);
        }
    } else {
        /* back to the song: no bend kept */
        for (uint32_t v = 0; v < MIDI_ROUTE_VOICES; v++) {
            if (r->v[v].bend_sent &&
                (!r->player.jam_bend || r->player.jam_bend(r->player.ctx, v, 0))) {
                r->v[v].bend_sent = 0;
            }
        }
    }
    if (r->filter_dirty) {
        if (!r->player.filter ||
            r->player.filter(r->player.ctx, r->locks, r->cutoff, r->resonance)) {
            r->filter_dirty = 0;
            r->locks_sent = r->locks;
        }
    }
}

/* ---- the MIDI chunk ---- */

void midi_route_setup_encode(const midi_route_setup* s, song_midi* out) {
    memset(out, 0, sizeof *out);
    out->mode = s->mode;
    out->rows_per_beat = s->rows_per_beat;
    /* each voice's channel: chip A's three, then the instrument's */
    uint8_t n = s->base;
    switch (s->mode) {
    case MIDI_MODE_MULTI:
        out->count = 4;
        for (uint32_t i = 0; i < 4u; i++) {
            out->channel[i] = (uint8_t)((n + i) & 15u);
        }
        break;
    case MIDI_MODE_WHOLE_A:
        out->count = 4;
        out->channel[0] = out->channel[1] = out->channel[2] = n;
        out->channel[3] = (uint8_t)((n + MIDI_ROUTE_LIVE_OFFSET) & 15u);
        break;
    case MIDI_MODE_OMNI: out->count = 0; break;
    default: /* LIVE, WHOLE-CHIP B: the instrument's */
        out->count = 1;
        out->channel[0] = n;
        break;
    }
    out->has_options = 1;
    out->flags = s->legato ? SONG_MIDI_LEGATO : 0u;
    out->whole = s->option;
    out->chord[0] = s->chord[0];
    out->chord[1] = s->chord[1];
    out->arp_rate = s->arp_rate;
}

void midi_route_setup_decode(const song_midi* in, const midi_route_setup* dflt,
                             midi_route_setup* out) {
    *out = *dflt;
    out->mode = in->mode < MIDI_MODE_COUNT ? in->mode : dflt->mode;
    out->base = in->count ? (uint8_t)(in->channel[0] & 15u) : dflt->base;
    out->rows_per_beat = in->rows_per_beat ? in->rows_per_beat : dflt->rows_per_beat;
    if (in->has_options) {
        out->legato = (in->flags & SONG_MIDI_LEGATO) ? 1u : 0u;
        out->option = in->whole;
        out->chord[0] = in->chord[0];
        out->chord[1] = in->chord[1];
        out->arp_rate = in->arp_rate;
    }
    clamp_setup(out);
}

int midi_route_setup_from_song(const gt2_song* s, const midi_route_setup* dflt,
                               midi_route_setup* out) {
    song_midi m;
    if (!song_ext_get_midi(s, &m)) {
        *out = *dflt;
        return 0;
    }
    midi_route_setup_decode(&m, dflt, out);
    return 1;
}

int midi_route_setup_to_song(gt2_song* s, const midi_route_setup* setup) {
    song_midi m;
    midi_route_setup_encode(setup, &m);
    return song_ext_set_midi(s, &m);
}
