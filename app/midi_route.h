// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_ROUTE_H
#define SIDOU_MIDI_ROUTE_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "midi_in.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    MIDI_MODE_LIVE = 0,
    MIDI_MODE_MULTI = 1,
    MIDI_MODE_WHOLE_A = 2,
    MIDI_MODE_WHOLE_B = 3,
    MIDI_MODE_OMNI = 4,
    MIDI_MODE_COUNT = 5
} midi_mode;

typedef enum { MIDI_WHOLE_POLY = 0, MIDI_WHOLE_CHORD = 1, MIDI_WHOLE_ARP = 2 } midi_whole_option;

#define MIDI_ROUTE_VOICES 3u
#define MIDI_ROUTE_HELD 8u
#define MIDI_ROUTE_LIVE_OFFSET 3u
#define MIDI_ROUTE_NO_NOTE 0xffu
#define MIDI_ROUTE_GT_FIRST 12u
#define MIDI_ROUTE_GT_NOTES 96u
#define MIDI_ROUTE_INSTR_MAX 63u
#define MIDI_ROUTE_ARP_CLOCKS 6u
#define MIDI_ROUTE_TICKS_PER_FRAME 2u
#define MIDI_ROUTE_CHORD_DEFAULT_2 4u
#define MIDI_ROUTE_CHORD_DEFAULT_3 7u
#define MIDI_ROUTE_ARP_DEFAULT 3u
#define MIDI_ROUTE_ROWS_DEFAULT 4u
#define MIDI_ROUTE_CHORD_MAX 24u
#define MIDI_ROUTE_ARP_MAX 32u
#define MIDI_ROUTE_ROWS_MAX 16u
#define MIDI_ROUTE_CC_BANK_MSB 0u
#define MIDI_ROUTE_CC_BANK_LSB 32u
enum { MIDI_ROUTE_BANK_SOUND = 0, MIDI_ROUTE_BANK_SONGS = 1, MIDI_ROUTE_BANK_PROFILES = 2 };
#define MIDI_ROUTE_BANKS 3u
#define MIDI_ROUTE_NO_REQUEST 0xffu

typedef struct {
    uint8_t mode;
    uint8_t base;
    uint8_t rows_per_beat;
    uint8_t legato;
    uint8_t option;
    uint8_t chord[2];
    uint8_t arp_rate;
} midi_route_setup;

typedef struct {
    int (*jam_on)(void* ctx, uint32_t voice, uint32_t note, uint32_t instr, uint32_t legato);
    int (*jam_off)(void* ctx, uint32_t voice);
    int (*jam_bend)(void* ctx, uint32_t voice, int32_t offset);
    int (*filter)(void* ctx, uint32_t locks, uint32_t cutoff, uint32_t resonance);
    void* ctx;
} midi_route_player;

typedef struct {
    uint8_t held[MIDI_ROUTE_HELD];
    uint8_t n;
    uint8_t instr;
    uint16_t bend;
    uint8_t range;
    uint8_t rpn_msb, rpn_lsb;
} midi_route_chan;

typedef struct {
    uint8_t note;
    uint32_t stamp;
    int32_t bend_sent;
} midi_route_voice;

typedef struct midi_route {
    midi_route_setup setup;
    midi_route_player player;
    live* l;
    keys_route* kr;
    midi_route_chan ch[MIDI_ROUTE_VOICES];
    midi_route_voice v[MIDI_ROUTE_VOICES];
    uint8_t to[16];
    uint8_t fast[16];
    uint8_t bank[16][2];
    uint8_t song_req;
    uint8_t locks;
    uint8_t locks_sent;
    uint16_t cutoff;
    uint8_t resonance;
    uint8_t filter_dirty;
    uint8_t arp_pos;
    uint8_t arp_wait;
    uint32_t stamp;
    uint32_t dropped;
} midi_route;

typedef enum {
    MIDI_ROUTE_IGNORE = 0,
    MIDI_ROUTE_LIVE = 1,
    MIDI_ROUTE_TAKE = 2
} midi_route_to;

void midi_route_setup_default(midi_route_setup* s);
void midi_route_init(midi_route* r, live* l, keys_route* kr, const midi_route_player* player);
void midi_route_set(midi_route* r, const midi_route_setup* s);
int midi_route_owns_chip_a(uint32_t mode);
#define midi_route_fast(r, msg) (((r)->fast[(msg)->channel & 15u] >> (msg)->kind) & 1u)
midi_route_to midi_route_classify(const midi_route* r, const midi_msg* msg);
int midi_route_edits_live(const midi_route* r, const midi_msg* msg);
int32_t midi_route_take_song(midi_route* r);
void midi_route_take(midi_route* r, const midi_msg* msg);
void midi_route_reset_input(midi_route* r);
void midi_route_tick(midi_route* r);

int midi_route_setup_from_song(const gt2_song* s, const midi_route_setup* dflt,
                               midi_route_setup* out);
int midi_route_setup_to_song(gt2_song* s, const midi_route_setup* setup);
void midi_route_setup_encode(const midi_route_setup* s, song_midi* out);
void midi_route_setup_decode(const song_midi* in, const midi_route_setup* dflt,
                             midi_route_setup* out);

const char* midi_route_mode_name(uint32_t mode);
const char* midi_route_option_name(uint32_t option);

#ifdef SIDOU_MIDI_MAP_DOC
const char* midi_route_mode_doc(uint32_t mode);
const char* midi_route_bank_doc(uint32_t bank);
#endif

#ifdef __cplusplus
}
#endif

#endif
