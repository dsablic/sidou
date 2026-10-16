// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player (M2c Tasks 4-5b): GoatTracker 2's packed play routine in C, playing a
 * song from the model (gt2_song.h) as GoatTracker 2.77's player.s plays it once gt2reloc
 * has packed it with the song's options (decision record 0007, "Equivalence, defined").
 *
 * One gt2_play_frame call is one call of player.s's play routine: it emits the SID writes
 * that call makes, in the order it makes them, through a sink (register 0-24, value). With
 * multispeed N the caller calls it N times a frame, as the C64 calls the 6502 routine.
 *
 * player.s is assembled per song: gt2reloc (greloc.c) reads the song and leaves out the
 * code for what the song does not use (NUMCHANNELS, NOFILTER, SIMPLEPULSE, NOWAVEDELAY,
 * ...). Most of those cuts change nothing a song can hear, but some change the writes: a
 * channel whose patterns hold only rests is not played at all, a song with no filter use
 * never writes the cutoff, a song whose pulse table has no low nybble writes the pulse in
 * one byte, and so on. gt2_play_init therefore runs greloc's analysis over the song
 * (patterns named by its subtunes, the instruments and table rows they reach) and the
 * player follows the shape the packer would give it. The analysis walks the song; it is
 * the main loop's, not the audio interrupt's (M2c Task 6 hands the state over).
 *
 * The packer also rewrites the data (instruments reordered with hard restart ones first,
 * patterns packed, tables remapped, wavetable bytes recoded, tempos less one). The player
 * reads the model's .sng bytes and applies each rewrite where it reads them, so a cell edit
 * plays at once.
 *
 * The contract with the song model (gt2_song.h): the player re-reads the index entry each
 * time it reads a pattern or an orderlist, and keeps no arena pointer from one call to the
 * next. It plays orderlists that break readme 3.1's rules, and restart positions past the
 * entries, as player.s reads the packed bytes, with every read bounded.
 *
 * Portable C11, no allocation, no libm, no calls into libc: the device runs it from SRAM in
 * the audio interrupt (M2c Task 6). */
#ifndef SIDOU_GT2_PLAY_H
#define SIDOU_GT2_PLAY_H

#include <stdint.h>

#include "song/gt2_song.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

#define GT2_PLAY_CHANNELS 3u
#define GT2_SID_REGS 25u

/* One SID write: a register 0-24 (offsets from $D400) and its value. */
typedef void (*gt2_sid_fn)(void* ctx, uint8_t reg, uint8_t value);

/* A channel's variables, player.s's mt_chn* (the names are its own, less the prefix). */
typedef struct {
    uint16_t songptr;  /* the orderlist entry the sequencer reads next (a model position) */
    uint8_t trans, repeat;
    uint8_t pattptr;   /* the row the next fetch reads; 0 also when a pattern is due */
    uint8_t newfx, newparam, fx, param, newnote;
    uint8_t waveptr, wave, pulseptr, pulsetime;
    uint8_t pattnum, tempo, counter, note, instr, gate;
    uint8_t vibtime, vibdelay, wavetime, freqlo, freqhi, pulselo, pulsehi;
    uint8_t ad, sr, gatetimer, lastnote;
} gt2_channel;

/* What the song and its options make of player.s (gt2_play_init). */
enum {
    GT2_PF_BUFFERED = 1u << 0,   /* BUFFEREDWRITES */
    GT2_PF_GHOST = 1u << 1,      /* GHOSTREGS: the player writes ghost registers only */
    GT2_PF_ZPGHOST = 1u << 2,    /* ZPGHOSTREGS: the main program copies them after a call */
    GT2_PF_ALT = 1u << 3,        /* altplayer.s: a hard restart ADSR of $F000 and up */
    GT2_PF_PULSEOPT = 1u << 4,   /* PULSEOPTIMIZATION */
    GT2_PF_REALTIMEOPT = 1u << 5,/* REALTIMEOPTIMIZATION */
    GT2_PF_AUTHORINFO = 1u << 6, /* timing marks kept */
    GT2_PF_NOEFFECTS = 1u << 7,  /* no pattern command, no wavetable command */
    GT2_PF_NOFILTER = 1u << 8,   /* no filter table: the cutoff is never written */
    GT2_PF_SIMPLEPULSE = 1u << 9,/* pulse widths in one byte, written to both registers */
    GT2_PF_WAVEDELAY = 1u << 10, /* the wavetable's delays (and its recoding) */
    GT2_PF_WAVECMD = 1u << 11    /* the wavetable's commands */
};

typedef struct {
    const gt2_song* song;
    uint16_t flags;          /* GT2_PF_* */
    uint8_t subtune;         /* the subtune whose orderlists play */
    uint8_t songs;           /* the subtunes the packer writes orderlists for */
    uint8_t channels;        /* NUMCHANNELS: 1-3, the channels the packed player runs */
    uint8_t ad_param, sr_param; /* the hard restart ADSR */
    uint8_t reset_instr;     /* the instrument the packer numbers 1 (each channel's first) */
    uint8_t first_legato;    /* multispeed only: the legato instrument played as no-HR */
    uint8_t default_tempo;   /* DEFAULTTEMPO */
    uint8_t first_note, last_note; /* the frequency table the packer keeps */
    uint8_t initsong;        /* 1 until the first call initialises the song */
    uint8_t filtstep, filttime, filtcutoff, filtctrl, filttype, masterfader;
    uint8_t funktempo[2];
    uint8_t timing_mark;     /* DXY's mark, with author info */
    /* A bit a channel: its frequency came from past the packed frequency table, where the
     * 6502 reads bytes only the packed layout knows (gt2_play.c, "Frequencies"). */
    uint8_t freq_unknown;
    uint8_t unknown;         /* scratch: the last frequency reads hit that region */
    uint8_t regs[GT2_SID_REGS]; /* the SID's registers as written (the ghost registers) */
    uint32_t pattused[(GT2_MAX_PATTERNS + 31u) / 32u]; /* the patterns the packer numbers */
    gt2_channel chn[GT2_PLAY_CHANNELS];
} gt2_player;

/* Prepares the player for a subtune with the options (o NULL: GoatTracker's defaults), as
 * the packed player is after `JSR init` with A the subtune: the next call initialises the
 * song. 0, or -1 if the packed song would not hold the subtune (it has an empty orderlist
 * on channels 1-3, or lies past the subtunes gt2reloc packs). Walks the song (greloc's
 * analysis): the main loop's. */
int gt2_play_init(gt2_player* p, const gt2_song* s, uint32_t subtune, const gt2_options* o);

/* One call of the play routine: its SID writes, in order, to sink. */
void gt2_play_frame(gt2_player* p, gt2_sid_fn sink, void* ctx);

#ifdef __cplusplus
}
#endif

#endif
