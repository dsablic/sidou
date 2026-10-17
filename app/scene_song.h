// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* RED MOON RITE's ears (M2f Task 4): what the music does, a frame at a time, as the vignette's
 * steps (app/scene_rmr.h scene_rmr_beat), from the song while one plays, else from the
 * instrument (docs/art/README.md "RED MOON RITE").
 *
 * From a song: the song control's event ring (core/player/song_ctl.h: each call's row, note
 * and gate events, drained once a main-loop pass and handed to each reader,
 * platform/fm1/main.c) and chip A's registers as the player last wrote them (the cutoff):
 * - the voices' roles, DRUMS, BASS and ARP (or LEAD, which counts as ARP): the song's SCNE
 *   chunk's (core/song/song_ext.h), and where it leaves a voice AUTO, guessed from the
 *   current and the last bar: a voice that played an instrument whose wavetable starts with
 *   noise is DRUMS; of the others, the one whose lowest note is lowest is BASS, the rest ARP;
 * - a drum voice's note is a KICK, SNARE or HAT: the SCNE chunk's drum table's for its
 *   instrument, else by the drum's pitch, the first wavetable step's note (an absolute note,
 *   or the row's note moved by a relative one): below SCENE_SONG_KICK_BELOW a kick, from
 *   SCENE_SONG_HAT_FROM a hat, a snare between (MIDI numbers);
 * - the place: a row is each row event of voice 1 (SCENE_SONG_CLOCK); its place in the bar is
 *   the pattern's row modulo 4 beats of the song's rows a beat (the MIDI chunk's, else
 *   SCENE_SONG_RPB), and the bar counts each time that place goes back (bar 0 is the first
 *   after the play); the fill is the last beat of every fourth bar;
 * - the bass's note is high when it is above the middle of the bass's range over the current
 *   and the last bar (the prototype's 50, D3, while that range is a single note);
 * - every voice's gate; a voice muted (or held by the keys, KEYS A) plays no part: its notes
 *   and gate count for nothing, so a muted drum voice stops the chops and parries.
 * From the instrument (no song playing): each note-on is a hit by its voice's waveform as LIVE
 * shows it: with noise a drum by its pitch, as above; else the lowest note held is the bass,
 * the others the arp. A free clock makes a row every SCENE_SONG_FREE_FRAMES frames while a
 * note is held (about 125 BPM at 4 rows a beat), so the fill comes with playing too.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SCENE_SONG_H
#define SIDOU_SCENE_SONG_H

#include <stdint.h>

#include "live.h"
#include "player/song_ctl.h"
#include "scene_rmr.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCENE_SONG_VOICES 3u
/* The steps a frame keeps at most: at multispeed 4 and speed 1 a frame has 8 rows. Older
 * ones are dropped (counted in lost). */
#define SCENE_SONG_BEATS 16u
/* Drum pitches, MIDI notes: a kick below, a hat from, a snare between. */
#define SCENE_SONG_KICK_BELOW 100u
#define SCENE_SONG_HAT_FROM 106u
/* The bass's split while its range is a single note: the prototype's D3. */
#define SCENE_SONG_BASS_SPLIT 50u
/* Rows a beat without the MIDI chunk, and the voice whose rows are the clock. */
#define SCENE_SONG_RPB 4u
#define SCENE_SONG_CLOCK 0u
/* With no song, a row every this many frames while a note is held. */
#define SCENE_SONG_FREE_FRAMES 3u
#define SCENE_SONG_NO_NOTE 0xffu

typedef struct {
    const song_ctl* sc; /* NULL: none bound */
    uint8_t playing;    /* the song plays: its events are the music */
    uint8_t rpb;
    uint8_t role[SCENE_SONG_VOICES]; /* the SCNE chunk's SONG_ROLE_*, AUTO where none */
    uint8_t drums;
    uint8_t drum[SONG_SCENE_DRUMS_MAX][2];
    /* The place. */
    uint8_t s, bar;
    uint8_t rows_seen;  /* a row has come since the play */
    /* Each voice: its gate, and this bar's [0] and the last bar's [1] noise and lowest note. */
    uint8_t gate[SCENE_SONG_VOICES];
    uint8_t noise[2][SCENE_SONG_VOICES];
    uint8_t low[2][SCENE_SONG_VOICES];
    /* The bass's range, this bar's and the last's. */
    uint8_t blo[2], bhi[2];
    /* The call being gathered: its number, its row event, its notes (SCENE_SONG_NO_NOTE). */
    uint8_t open;
    uint16_t call;
    uint8_t row_ev, row;
    uint8_t note[SCENE_SONG_VOICES], instr[SCENE_SONG_VOICES];
    /* The steps since the last frame (scene_song_frame takes them). */
    scene_rmr_beat beat[SCENE_SONG_BEATS];
    uint8_t n;
    uint32_t lost;
    /* The instrument's: each voice's stamp at the last frame, the free clock. */
    uint32_t stamp[SCENE_SONG_VOICES];
    uint8_t free;
} scene_song;

/* What a frame heard. */
typedef struct {
    scene_rmr_beat beat[SCENE_SONG_BEATS]; /* the steps, oldest first */
    uint32_t n;
    uint32_t cutoff;  /* 0..2047 */
    uint8_t sounding; /* a voice's gate is on */
} scene_song_frame_out;

void scene_song_init(scene_song* ss);
/* Once a main-loop pass, from the event ring's one reader: the song control (NULL: none) and
 * the events drained from it this pass. A play that starts reads the song's SCNE and MIDI
 * chunks and starts the place again. */
void scene_song_events(scene_song* ss, const song_ctl* sc, const song_event* ev, uint32_t n);
/* Once a frame: the steps since the last, the cutoff and whether a voice sounds, from the
 * song while it plays, else from the instrument l. */
void scene_song_frame(scene_song* ss, const live* l, scene_song_frame_out* out);
/* The drum (SONG_DRUM_*) a note's pitch (MIDI) makes. */
uint32_t scene_song_drum_of_pitch(uint32_t pitch);

#ifdef __cplusplus
}
#endif

#endif
