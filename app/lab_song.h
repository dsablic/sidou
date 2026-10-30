// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LAB's song bridge (docs/plans/2026-10-30-1.5-design.md, section 2): a LAB sound (lab_sound.h)
 * as a GoatTracker 2 instrument and its table rows, and back.
 *
 * Send (lab_song_encode): the waveform, ring and sync as a one-row wave program (`ctrl|gate 00`,
 * then `FF 00`) and the first-frame waveform; AD and SR with GoatTracker's hard restart (gate
 * timer 2, HR on), an attack of 0 raised to 1; the pulse width as a pulse program's `$8H LL` row;
 * the PWM (its link baked) as a pulse loop; the filter (mode, resonance with RESO LINK baked at
 * the cutoff, the cutoff's top 8 bits) routed to the song's current voice; the filter LFO as a
 * filter loop; the vibrato as the instrument's delay and a calculated-mode speed row (50 / (2S +
 * 4) Hz, a power-of-two depth); the arpeggio as the ARP helper's default chord (song_pages.h).
 * One-voice sync, the fine tune, the performance targets and the effect slots are dropped.
 * lab_song_instr.notes says what was approximated or dropped.
 *
 * The exact round trip: a trailer chunk per sent instrument (LAB_SONG_TAG: `LB` and the
 * instrument in two hex digits; docs/reference/song-format.md "LBnn") keeps the 48-byte sound and
 * a CRC-32 of the instrument's 25 bytes and its programs' rows. Pull (lab_song_pull) restores the
 * sound exactly while the CRC matches, else decodes the instrument.
 *
 * Portable C, no allocation; the main loop's. */
#ifndef SIDOU_LAB_SONG_H
#define SIDOU_LAB_SONG_H

#include <stdint.h>

#include "lab_sound.h"
#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The rows a program takes at most (the ARP helper's: SONG_PAGES_ARP_ROWS). */
#define LAB_SONG_ROWS 10u

/* What a send approximated or dropped, and what a pull approximated (lab_song_words names them). */
enum {
    LAB_SONG_ATTACK = 1u << 0, /* attack 0 raised to 1 (GoatTracker's hard restart) */
    LAB_SONG_PWM = 1u << 1,    /* the PWM as a pulse loop: from each note, 50 Hz steps */
    LAB_SONG_FILTER = 1u << 2, /* the filter takes over the song's (cutoff in 8 bits) */
    LAB_SONG_LFO = 1u << 3,    /* the filter LFO as a filter loop */
    LAB_SONG_VIB = 1u << 4,    /* the vibrato at 50/n Hz and a power-of-two depth */
    LAB_SONG_ARP = 1u << 5,    /* a default chord: TABLE's ARP helper decides */
    LAB_SONG_WAVE = 1u << 6,   /* a waveform the wave table cannot hold (noise combined) */
    LAB_SONG_DRUM = 1u << 7,   /* pull: absolute notes or a program of several waveforms */
    LAB_SONG_SWEEP = 1u << 8,  /* pull: a one-shot pulse or filter sweep */
    LAB_SONG_GATE = 1u << 9,   /* pull: another gate timer, no hard restart, or legato */
    LAB_SONG_APPROX = (1u << 10) - 1u,
    /* dropped on a send */
    LAB_SONG_SYNC = 1u << 10, /* one-voice sync */
    LAB_SONG_TUNE = 1u << 11, /* the fine tune */
    LAB_SONG_PLAY = 1u << 12, /* the velocity, wheel and pressure targets */
    LAB_SONG_FX = 1u << 13,   /* the effect slots */
    LAB_SONG_DROPPED = 15u << 10,
    LAB_SONG_EXACT = 1u << 14 /* pull: the sound kept with the instrument, exactly */
};

/* An instrument and its programs: rows[t] rows of table t (GT2_WTBL..GT2_STBL), from the row the
 * instrument's pointer names. */
typedef struct {
    uint8_t instr[GT2_INSTR_BYTES];
    uint8_t rows[GT2_TABLES];
    uint8_t left[GT2_TABLES][LAB_SONG_ROWS];
    uint8_t right[GT2_TABLES][LAB_SONG_ROWS];
    uint16_t notes; /* LAB_SONG_* */
} lab_song_instr;

/* The sound as an instrument whose programs start at first[t] (1 based: the jumps and pointers
 * name them), its filter routed to voice (0..2), the filter LFO's band on model's cutoff walk,
 * named name (up to 16 characters). */
void lab_song_encode(const uint8_t sound[LAB_SOUND_BYTES], const char* name, uint32_t voice,
                     uint32_t model, const uint8_t first[GT2_TABLES], lab_song_instr* out);

/* The chunk's tag for instrument i (1..63): `LB` and i in hex. */
uint32_t lab_song_tag(uint32_t instr);
/* The CRC-32 of instrument i's bytes and rows[t] rows of each table from its pointer. */
uint32_t lab_song_crc(const gt2_song* s, uint32_t instr, const uint8_t rows[GT2_TABLES]);
/* Instrument i's kept sound into sound and its rows into rows, if its chunk holds one and the
 * CRC still matches: 1; else 0. */
int lab_song_kept(const gt2_song* s, uint32_t instr, uint8_t rows[GT2_TABLES],
                  uint8_t sound[LAB_SOUND_BYTES]);
/* Keeps the sound with instrument i as sent (its rows), the CRC of the song's bytes as they will
 * be: in.instr and in's rows. At most LAB_SONG_KEPT_MAX chunks: past them a chunk whose CRC no
 * longer matches makes room. 0, or -1 (none kept: a pull decodes). */
#define LAB_SONG_KEPT_MAX 16u
int lab_song_keep(gt2_song* s, uint32_t instr, const lab_song_instr* in,
                  const uint8_t sound[LAB_SOUND_BYTES]);
/* The CRC lab_song_crc will give once in is written. */
uint32_t lab_song_crc_of(const lab_song_instr* in);

/* Pull: instrument i as a sound, into sound (which holds the current one: the bend range and the
 * three targets stay). The LAB_SONG_* notes: LAB_SONG_EXACT from the chunk, else what the decode
 * approximated (0: close to lossless). */
uint32_t lab_song_pull(const gt2_song* s, uint32_t instr, uint32_t model,
                       uint8_t sound[LAB_SOUND_BYTES]);

/* The notes' words, at most max - 1 characters: the dropped ones (`FX TUNE SYNC PLAY`) when
 * dropped, else the approximated ones (`PWM FILTER LFO VIB ARP ATTACK WAVE DRUM SWEEP GATE`). */
void lab_song_words(uint32_t notes, int dropped, char* out, uint32_t max);

/* The lowest instrument whose 25 bytes are all 0 (1..63), or 0. */
uint32_t lab_song_free(const gt2_song* s);
/* The rows of table t up to its last that is not blank (its first free row, 0 based, after). */
uint32_t lab_song_table_end(const gt2_song* s, uint32_t t);
/* 1 when an instrument other than i has its pointer to table t within rows first..first + n - 1
 * (1 based). */
int lab_song_shared(const gt2_song* s, uint32_t instr, uint32_t t, uint32_t first, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
