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
#define SONG_TRAILER_LEAD_IN (SONG_TRAILER_FF_FREE - SONG_TRAILER_HEADER) /* 241: no $FF */
#define SONG_TRAILER_CHANNELS 14u      /* the lead-in's first byte: the GT2 part's 3 or 6 */
#define SONG_TRAILER_CHUNKS (SONG_TRAILER_FF_FREE + 4u) /* after the GT2 part's CRC */
#define SONG_TRAILER_MIN (SONG_TRAILER_CHUNKS + 4u)     /* no chunks: 263 */
#define SONG_CHUNK_HEADER 8u

/* CRC-32 (zlib's): crc 0 to start, then the running value. */
uint32_t song_crc32(uint32_t crc, const uint8_t* p, uint32_t n);

/* Chunks in the extension area. find: the data and its length, NULL if absent. set:
 * replaces a chunk of that tag in place or appends one; 0, or -1 if it does not fit (the
 * area unchanged). remove: 1 if there was one. Mutation also returns -GT2_ERR_SONG_FULL,
 * -GT2_ERR_SAVING or -GT2_ERR_PENDING (an indexed edit's locks must settle first). */
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
#define GT2_PLAYER_KNOWN 0x7fu
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
/* 0, or -1 for a short chunk, another version, or a value with no meaning to gt2reloc (an
 * NTSC byte or SID model above 1, an unknown player flag); o then holds the defaults. What
 * gt2reloc clamps is clamped as it does: multispeed to 16 (the device's cap of 4 is the
 * reader's, gt2_caps), -O and -R to 1, a clock below 100 Hz to 0 (PAL/NTSC's), fine
 * vibrato to 2 (always). */
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

/* LOCK: parameter locks, sorted by pattern, row, parameter (M2c Task 7). A lock replaces one
 * parameter for one row of a pattern (each channel that plays the pattern): the song's own
 * value returns at the next row without it. The pulse width and the routing are the voice's
 * that plays the row; the filter and the character are the chip's. */
#define SONG_LOCKS_MAX 512u
enum {
    SONG_LOCK_CUTOFF,      /* 0..2047, $15/$16 */
    SONG_LOCK_RESONANCE,   /* 0..15, $17's high nybble */
    SONG_LOCK_FILTER_MODE, /* 0..7: low-pass, band-pass, high-pass ($18 bits 4-6) */
    SONG_LOCK_ROUTE,       /* 0..1: the voice into the filter ($17's bit) */
    SONG_LOCK_PULSE,       /* 0..4095: the voice's pulse width */
    SONG_LOCK_AGE,         /* 0..127: the character's controls (chip_ctl, with its glide) */
    SONG_LOCK_DAC,
    SONG_LOCK_THUMP,
    SONG_LOCK_BLEED,
    SONG_LOCK_PARAMS
};
#define SONG_LOCK_BYTES 5u /* a record: pattern, row, parameter, value (u16) */
/* A parameter's largest value, 0 for none. */
uint32_t song_lock_max(uint32_t param);
/* A file's LOCK chunk (version 1) checked as it is read (gt2_sng_read, gt2_stream): every
 * record's pattern, row, parameter and value in range (song_lock_max) and the records in
 * strictly increasing (pattern, row, parameter) order, as song_lock_put keeps them. A chunk
 * that fails, or whose count or length is malformed, is removed whole: 1. Else 0, the chunk as
 * it was (another version's is left alone). */
int song_ext_locks_check(gt2_song* s);
typedef struct {
    uint8_t pattern, row, param;
    uint16_t value;
} song_lock;
/* The lock count (at most max copied out), or 0 if none. */
uint32_t song_ext_get_locks(const gt2_song* s, song_lock* out, uint32_t max);
/* -1 if n > SONG_LOCKS_MAX, the locks are not sorted and distinct, or they do not fit. */
int song_ext_set_locks(gt2_song* s, const song_lock* locks, uint32_t n);
/* A row's locks (SONG_LOCK_BYTES a record, by parameter): the first and their count (0:
 * NULL). A binary search: the main loop's. */
const uint8_t* song_locks_row(const gt2_song* s, uint32_t pattern, uint32_t row, uint32_t* n);
/* Sets one lock (a new one, or its value), kept sorted: 0, or -1 if out of range, the song
 * holds SONG_LOCKS_MAX, or the extension area is full. */
int song_lock_put(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param, uint32_t value);
/* 1 if there was one, removed; a negative GT2_ERR_* on refusal. */
int song_lock_remove(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param);
/* What a pattern's rebuild does to its locks (gt2_song.c, once the swap is applied): a row
 * inserted at row (the locks at it and after move down one; any reaching 128 go), deleted
 * at row (its locks go, those after move up), the rows cut to row (the locks past them go),
 * or the pattern removed (all go). PUSH keeps the length: row's low byte is the insertion
 * row, its next byte the body length (locks reaching that length go). A fixed-length pull
 * uses DELETE: the removed row's locks go, later rows move up, the new empty tail has none.
 * Main-loop only: the interrupt reads song_ctl's resolved copies, never this chunk. */
enum { SONG_LOCKS_INSERT = 1, SONG_LOCKS_DELETE, SONG_LOCKS_RESIZE, SONG_LOCKS_REMOVE,
       SONG_LOCKS_PUSH };
void song_locks_pattern_edit(gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row);
/* Extension length after that edit, without changing the chunk or its records. */
uint32_t song_locks_pattern_size(const gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row);
/* A shift undo's final extension size, including its discarded row's packed locks. No
 * mutation; the records must name the restored row and be sorted/distinct. */
gt2_err song_locks_restore_check(const gt2_song* s, uint32_t op, uint32_t pattern,
                                uint32_t row, uint32_t restored_row, const uint8_t* locks,
                                uint32_t n, uint32_t* ext_used);
/* Only after the matching checked shift applies, before its pending reservation ends.
 * Storage and serialized growth were reserved by song_locks_restore_check's caller. */
void song_locks_restore_reserved(gt2_song* s, const uint8_t* locks, uint32_t n);

/* MIDI: the song's MIDI setup (M2e Task 3, app/midi_route.h gives the fields meaning). The
 * options follow the channels in the chunk (M2e); a chunk without them reads as has_options
 * 0, and the writer always writes them. */
#define SONG_MIDI_CHANNELS_MAX 6u
#define SONG_MIDI_OPTION_BYTES 5u
#define SONG_MIDI_LEGATO 0x01u /* flags */
typedef struct {
    uint8_t mode, rows_per_beat, count;
    uint8_t channel[SONG_MIDI_CHANNELS_MAX];
    uint8_t has_options;
    uint8_t flags;    /* SONG_MIDI_* */
    uint8_t whole;    /* WHOLE-CHIP A's option */
    uint8_t chord[2]; /* the stored chord: semitones above the note on voices 2 and 3 */
    uint8_t arp_rate; /* frames an ARP step, 0: a 16th of the clock */
} song_midi;
int song_ext_get_midi(const gt2_song* s, song_midi* v);
int song_ext_set_midi(gt2_song* s, const song_midi* v);

/* SCNE: the SCENE piece, the voices' roles and the instruments' drums (M2f: RED MOON RITE's
 * music hooks, app/scene_song.h). A role or a drum the chunk leaves out (AUTO, or an
 * instrument not in the table) is guessed from the music (scene_song.h). The drum table
 * follows the roles, and a chunk without one (M2c's) has none. */
#define SONG_SCENE_VOICES_MAX 6u
#define SONG_SCENE_DRUMS_MAX 16u
enum { SONG_ROLE_AUTO, SONG_ROLE_DRUMS, SONG_ROLE_BASS, SONG_ROLE_ARP, SONG_ROLE_LEAD,
       SONG_ROLE_NONE };
enum { SONG_DRUM_NONE, SONG_DRUM_KICK, SONG_DRUM_SNARE, SONG_DRUM_HAT };
typedef struct {
    uint8_t piece, count;
    uint8_t role[SONG_SCENE_VOICES_MAX]; /* SONG_ROLE_* of each voice */
    uint8_t drums;                       /* the drum table's pairs */
    uint8_t drum[SONG_SCENE_DRUMS_MAX][2]; /* an instrument (1-63) and its SONG_DRUM_* */
} song_scene;
int song_ext_get_scene(const gt2_song* s, song_scene* v);
int song_ext_set_scene(gt2_song* s, const song_scene* v);

/* KEYS: the keys' live preset and setup, live_params's values in LIVE_PARAM_* order. */
#define SONG_KEYS_MAX 64u
#define SONG_KEYS_PRESET_NONE 255u
typedef struct {
    uint8_t count;
    uint8_t preset; /* optional trailing preset index; NONE in an older chunk */
    uint16_t param[SONG_KEYS_MAX];
} song_keys;
int song_ext_get_keys(const gt2_song* s, song_keys* v);
int song_ext_set_keys(gt2_song* s, const song_keys* v);

#ifdef __cplusplus
}
#endif

#endif
