// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_EXT_H
#define SIDOU_SONG_EXT_H

#include <stdint.h>

#include "gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_TAG(a, b, c, d)                                                                 \
    ((uint32_t)(uint8_t)(a) | (uint32_t)(uint8_t)(b) << 8 | (uint32_t)(uint8_t)(c) << 16 |  \
     (uint32_t)(uint8_t)(d) << 24)
#define SONG_TAG_OPTS SONG_TAG('O', 'P', 'T', 'S')
#define SONG_TAG_CHPA SONG_TAG('C', 'H', 'P', 'A')
#define SONG_TAG_CHPB SONG_TAG('C', 'H', 'P', 'B')
#define SONG_TAG_SLOT SONG_TAG('S', 'L', 'O', 'T')
#define SONG_TAG_LOCK SONG_TAG('L', 'O', 'C', 'K')
#define SONG_TAG_MIDI SONG_TAG('M', 'I', 'D', 'I')
#define SONG_TAG_SCNE SONG_TAG('S', 'C', 'N', 'E')
#define SONG_TAG_KEYS SONG_TAG('K', 'E', 'Y', 'S')
#define SONG_TAG_DIGI SONG_TAG('D', 'I', 'G', 'I')
#define SONG_TAG_DICE SONG_TAG('D', 'I', 'C', 'E')
#define SONG_TAG_PAD SONG_TAG('P', 'A', 'D', ' ')
#define SONG_TAG_TRKB SONG_TAG('T', 'R', 'K', 'B')
#define SONG_TAG_RIPD SONG_TAG('R', 'I', 'P', 'D')

#define SONG_TRAILER_MAGIC "SIDOU-X"
#define SONG_TRAILER_VERSION 1u
#define SONG_TRAILER_HEADER 14u
#define SONG_TRAILER_FF_FREE 255u
#define SONG_TRAILER_LEAD_IN (SONG_TRAILER_FF_FREE - SONG_TRAILER_HEADER)
#define SONG_TRAILER_CHANNELS 14u
#define SONG_TRAILER_CHUNKS (SONG_TRAILER_FF_FREE + 4u)
#define SONG_TRAILER_MIN (SONG_TRAILER_CHUNKS + 4u)
#define SONG_CHUNK_HEADER 8u

uint32_t song_crc32(uint32_t crc, const uint8_t* p, uint32_t n);

const uint8_t* song_ext_find(const gt2_song* s, uint32_t tag, uint32_t* len);
int song_ext_set(gt2_song* s, uint32_t tag, const uint8_t* data, uint32_t len);
int song_ext_remove(gt2_song* s, uint32_t tag);
int song_ext_valid(const gt2_song* s);

#define GT2_PLAYER_BUFFERED 0x01u
#define GT2_PLAYER_SOUNDEFFECTS 0x02u
#define GT2_PLAYER_VOLUME 0x04u
#define GT2_PLAYER_AUTHORINFO 0x08u
#define GT2_PLAYER_ZPGHOSTREGS 0x10u
#define GT2_PLAYER_NOOPTIMIZATION 0x20u
#define GT2_PLAYER_FULLBUFFERED 0x40u
#define GT2_PLAYER_KNOWN 0x7fu
typedef struct {
    uint16_t hr_adsr;
    uint8_t multispeed;
    uint8_t ntsc;
    uint32_t clock_hz;
    uint32_t a4_millihz;
    uint8_t finevibrato;
    uint8_t optimize_pulse;
    uint8_t optimize_realtime;
    uint8_t sid_model;
    uint16_t player_flags;
    uint16_t sid_address;
    uint8_t player_page;
    uint8_t zeropage;
} gt2_options;
#define SONG_OPTS_BYTES 23u

void gt2_options_default(gt2_options* o);
int song_ext_get_options(const gt2_song* s, gt2_options* o);
int song_ext_set_options(gt2_song* s, const gt2_options* o);
void song_opts_encode(const gt2_options* o, uint8_t out[SONG_OPTS_BYTES]);
int song_opts_decode(const uint8_t* data, uint32_t len, gt2_options* o);

#define SONG_EXPERT_MAX 32u
typedef struct {
    uint8_t model;
    uint8_t profile;
    uint8_t age, dac, thump, bleed;
    uint8_t envelope;
    uint8_t expert_count;
    uint8_t expert[SONG_EXPERT_MAX];
} song_chip_setup;
int song_ext_get_chip(const gt2_song* s, uint32_t tag, song_chip_setup* c);
int song_ext_set_chip(gt2_song* s, uint32_t tag, const song_chip_setup* c);

#define SONG_SLOTS_MAX 8u
#define SONG_SLOT_FIELDS 5u
typedef struct {
    uint8_t count;
    uint8_t slot[SONG_SLOTS_MAX][SONG_SLOT_FIELDS];
} song_slots;
int song_ext_get_slots(const gt2_song* s, song_slots* v);
int song_ext_set_slots(gt2_song* s, const song_slots* v);

#define SONG_LOCKS_MAX 512u
enum {
    SONG_LOCK_CUTOFF,
    SONG_LOCK_RESONANCE,
    SONG_LOCK_FILTER_MODE,
    SONG_LOCK_ROUTE,
    SONG_LOCK_PULSE,
    SONG_LOCK_AGE,
    SONG_LOCK_DAC,
    SONG_LOCK_THUMP,
    SONG_LOCK_BLEED,
    SONG_LOCK_CHANCE,
    SONG_LOCK_PARAMS
};
#define SONG_LOCK_BYTES 5u
uint32_t song_lock_max(uint32_t param);
uint32_t song_mix(uint32_t x);
int song_chance_plays(uint32_t seed, uint32_t n, uint32_t chance);
int song_ext_locks_check(gt2_song* s);
typedef struct {
    uint8_t pattern, row, param;
    uint16_t value;
} song_lock;
uint32_t song_ext_get_locks(const gt2_song* s, song_lock* out, uint32_t max);
int song_ext_set_locks(gt2_song* s, const song_lock* locks, uint32_t n);
const uint8_t* song_locks_row(const gt2_song* s, uint32_t pattern, uint32_t row, uint32_t* n);
int song_lock_put(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param, uint32_t value);
int song_lock_remove(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param);
enum { SONG_LOCKS_INSERT = 1, SONG_LOCKS_DELETE, SONG_LOCKS_RESIZE, SONG_LOCKS_REMOVE,
       SONG_LOCKS_PUSH };
void song_locks_pattern_edit(gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row);
uint32_t song_locks_pattern_size(const gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row);
gt2_err song_locks_restore_check(const gt2_song* s, uint32_t op, uint32_t pattern,
                                uint32_t row, uint32_t restored_row, const uint8_t* locks,
                                uint32_t n, uint32_t* ext_used);
void song_locks_restore_reserved(gt2_song* s, const uint8_t* locks, uint32_t n);

#define SONG_MIDI_CHANNELS_MAX 6u
#define SONG_MIDI_OPTION_BYTES 5u
#define SONG_MIDI_LEGATO 0x01u
typedef struct {
    uint8_t mode, rows_per_beat, count;
    uint8_t channel[SONG_MIDI_CHANNELS_MAX];
    uint8_t has_options;
    uint8_t flags;
    uint8_t whole;
    uint8_t chord[2];
    uint8_t arp_rate;
} song_midi;
int song_ext_get_midi(const gt2_song* s, song_midi* v);
int song_ext_set_midi(gt2_song* s, const song_midi* v);

#define SONG_SCENE_VOICES_MAX 6u
#define SONG_SCENE_DRUMS_MAX 16u
enum { SONG_ROLE_AUTO, SONG_ROLE_DRUMS, SONG_ROLE_BASS, SONG_ROLE_ARP, SONG_ROLE_LEAD,
       SONG_ROLE_NONE, SONG_ROLE_BASS_DRUMS, SONG_ROLES };
enum { SONG_DRUM_NONE, SONG_DRUM_KICK, SONG_DRUM_SNARE, SONG_DRUM_HAT };
typedef struct {
    uint8_t piece, count;
    uint8_t role[SONG_SCENE_VOICES_MAX];
    uint8_t drums;
    uint8_t drum[SONG_SCENE_DRUMS_MAX][2];
} song_scene;
int song_ext_get_scene(const gt2_song* s, song_scene* v);
int song_ext_set_scene(gt2_song* s, const song_scene* v);

#define SONG_KEYS_MAX 64u
#define SONG_KEYS_PRESET_NONE 255u
typedef struct {
    uint8_t count;
    uint8_t preset;
    uint16_t param[SONG_KEYS_MAX];
} song_keys;
int song_ext_get_keys(const gt2_song* s, song_keys* v);
int song_ext_set_keys(gt2_song* s, const song_keys* v);

#define SONG_DICE_DENSITIES 5u
#define SONG_DICE_SCALES 6u
#define SONG_DICE_RHYTHMS 5u
#define SONG_DICE_RANGES 4u
#define SONG_DICE_LANES 3u
#define SONG_DICE_DENSITY_DEFAULT 2u
#define SONG_DICE_RANGE_DEFAULT 1u
#define SONG_DICE_BYTES 7u
typedef struct {
    uint8_t density;
    uint8_t scale[SONG_DICE_LANES];
    uint8_t rhythm, range;
} song_dice;
int song_ext_get_dice(const gt2_song* s, song_dice* v);
int song_ext_set_dice(gt2_song* s, const song_dice* v);

#define SONG_DIGI_MAX 16u
enum { SONG_DIGI_NONE, SONG_DIGI_KICK, SONG_DIGI_SNARE, SONG_DIGI_CLAP, SONG_DIGI_HAT,
       SONG_DIGI_KINDS };
typedef struct {
    uint8_t count;
    uint8_t entry[SONG_DIGI_MAX][2];
} song_digi;
int song_ext_get_digi(const gt2_song* s, song_digi* v);
int song_ext_set_digi(gt2_song* s, const song_digi* v);
void song_ext_digi_map(const gt2_song* s, uint8_t map[GT2_MAX_INSTR + 1u]);

#ifdef __cplusplus
}
#endif

#endif
