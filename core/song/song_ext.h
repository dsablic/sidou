// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's trailer (M2c Task 2; docs/reference/song-format.md, "SIDOU's trailer"): the
 * chunks a song keeps in its extension area, and a model for each M2 chunk.
 *
 * The extension area (gt2_song.ext, SONG_EXT_BYTES) holds the chunks as the file has them,
 * in the file's order: a tag, a 32-bit little-endian length, the data (its first byte the
 * chunk's version). PAD is never kept. Unknown chunks are kept and written back unchanged,
 * so a newer SIDOU's chunks survive an older one's save. The reader and writer
 * (gt2_sng.h) frame them with the trailer's header, lead-in and CRCs.
 *
 * Portable C11, no allocation. */
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
#define SONG_TAG_PAD SONG_TAG('P', 'A', 'D', ' ')
#define SONG_TAG_TRKB SONG_TAG('T', 'R', 'K', 'B') /* reserved: chip B's tracks (M5) */
#define SONG_TAG_RIPD SONG_TAG('R', 'I', 'P', 'D') /* reserved: rip data (M5) */

/* The trailer's frame. */
#define SONG_TRAILER_MAGIC "SIDOU-X"   /* and a 0 byte: 8 bytes */
#define SONG_TRAILER_VERSION 1u
#define SONG_TRAILER_HEADER 14u        /* magic, version, length */
#define SONG_TRAILER_FF_FREE 255u      /* bytes 0-254 hold no $FF (stereo detection) */
#define SONG_TRAILER_LEAD_IN (SONG_TRAILER_FF_FREE - SONG_TRAILER_HEADER) /* 241 */
#define SONG_TRAILER_CHUNKS (SONG_TRAILER_FF_FREE + 4u) /* after the GT2 part's CRC */
#define SONG_TRAILER_MIN (SONG_TRAILER_CHUNKS + 4u)     /* no chunks: 263 */
#define SONG_CHUNK_HEADER 8u

/* CRC-32 (zlib's): crc 0 to start, then the running value. */
uint32_t song_crc32(uint32_t crc, const uint8_t* p, uint32_t n);

/* Chunks in the extension area. find: the data and its length, NULL if absent. set:
 * replaces a chunk of that tag in place or appends one; 0, or -1 if it does not fit (the
 * area unchanged). remove: 1 if there was one. */
const uint8_t* song_ext_find(const gt2_song* s, uint32_t tag, uint32_t* len);
int song_ext_set(gt2_song* s, uint32_t tag, const uint8_t* data, uint32_t len);
int song_ext_remove(gt2_song* s, uint32_t tag);
/* 1 if the area is a well-formed chunk sequence (each length at least 1, the last ending
 * at ext_used). */
int song_ext_valid(const gt2_song* s);

/* OPTS: the playroutine options, every gt2reloc option (readme 4.4) and the SID model. */
#define GT2_PLAYER_BUFFERED 0x01u      /* -B */
#define GT2_PLAYER_SOUNDEFFECTS 0x02u  /* -D */
#define GT2_PLAYER_VOLUME 0x04u        /* -E */
#define GT2_PLAYER_AUTHORINFO 0x08u    /* -H */
#define GT2_PLAYER_ZPGHOSTREGS 0x10u   /* -C */
#define GT2_PLAYER_NOOPTIMIZATION 0x20u /* -I0 */
#define GT2_PLAYER_FULLBUFFERED 0x40u  /* -J */
typedef struct {
    uint16_t hr_adsr;      /* -A, $0F00 */
    uint8_t multispeed;    /* -S, 1 (0: 25 Hz) */
    uint8_t ntsc;          /* -N: 1, -P: 0 */
    uint32_t clock_hz;     /* -F, 0 */
    uint32_t a4_millihz;   /* -G in thousandths of a Hz, 0 */
    uint8_t finevibrato;   /* -V, 1 */
    uint8_t optimize_pulse;    /* -O, 1 */
    uint8_t optimize_realtime; /* -R, 1 */
    uint8_t sid_model;     /* 0 6581, 1 8580 (the editor's) */
    uint16_t player_flags; /* GT2_PLAYER_* */
    uint16_t sid_address;  /* -L, $D400 */
    uint8_t player_page;   /* -W, $10 */
    uint8_t zeropage;      /* -Z, $FC */
} gt2_options;
#define SONG_OPTS_BYTES 23u

void gt2_options_default(gt2_options* o);
/* 1 and the song's options if it has an OPTS chunk of version 1, else 0 and the defaults. */
int song_ext_get_options(const gt2_song* s, gt2_options* o);
int song_ext_set_options(gt2_song* s, const gt2_options* o);
void song_opts_encode(const gt2_options* o, uint8_t out[SONG_OPTS_BYTES]);
/* 0, or -1 for a short chunk or another version (o then holds the defaults). */
int song_opts_decode(const uint8_t* data, uint32_t len, gt2_options* o);

/* CHPA, CHPB: a chip's setup. */
#define SONG_EXPERT_MAX 32u
typedef struct {
    uint8_t model;    /* 0 6581, 1 8580 */
    uint8_t profile;  /* sid_character.profile */
    uint8_t age, dac, thump, bleed; /* 0..127 */
    uint8_t envelope; /* 0 INSTANT, 1 AUTHENTIC */
    uint8_t expert_count;
    uint8_t expert[SONG_EXPERT_MAX];
} song_chip_setup;
int song_ext_get_chip(const gt2_song* s, uint32_t tag, song_chip_setup* c);
int song_ext_set_chip(gt2_song* s, uint32_t tag, const song_chip_setup* c);

/* SLOT: the effect slots (algorithm, Amount, A, B, Mix; 0..127). */
#define SONG_SLOTS_MAX 8u
#define SONG_SLOT_FIELDS 5u
typedef struct {
    uint8_t count;
    uint8_t slot[SONG_SLOTS_MAX][SONG_SLOT_FIELDS];
} song_slots;
int song_ext_get_slots(const gt2_song* s, song_slots* v);
int song_ext_set_slots(gt2_song* s, const song_slots* v);

/* LOCK: parameter locks, sorted by pattern, row, parameter (M2c Task 7 gives them meaning). */
#define SONG_LOCKS_MAX 512u
typedef struct {
    uint8_t pattern, row, param;
    uint16_t value;
} song_lock;
/* The lock count (at most max copied out), or 0 if none. */
uint32_t song_ext_get_locks(const gt2_song* s, song_lock* out, uint32_t max);
/* -1 if n > SONG_LOCKS_MAX, the locks are not sorted and distinct, or they do not fit. */
int song_ext_set_locks(gt2_song* s, const song_lock* locks, uint32_t n);

/* MIDI: the song's MIDI setup (M2e gives the fields meaning). */
#define SONG_MIDI_CHANNELS_MAX 6u
typedef struct {
    uint8_t mode, rows_per_beat, count;
    uint8_t channel[SONG_MIDI_CHANNELS_MAX];
} song_midi;
int song_ext_get_midi(const gt2_song* s, song_midi* v);
int song_ext_set_midi(gt2_song* s, const song_midi* v);

/* SCNE: the SCENE piece and the voices' roles (M2f gives them meaning). */
#define SONG_SCENE_VOICES_MAX 6u
typedef struct {
    uint8_t piece, count;
    uint8_t role[SONG_SCENE_VOICES_MAX];
} song_scene;
int song_ext_get_scene(const gt2_song* s, song_scene* v);
int song_ext_set_scene(gt2_song* s, const song_scene* v);

/* KEYS: the keys' live preset and setup, live_params's values in LIVE_PARAM_* order. */
#define SONG_KEYS_MAX 64u
typedef struct {
    uint8_t count;
    uint16_t param[SONG_KEYS_MAX];
} song_keys;
int song_ext_get_keys(const gt2_song* s, song_keys* v);
int song_ext_set_keys(gt2_song* s, const song_keys* v);

#ifdef __cplusplus
}
#endif

#endif
