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
 * The options (song_ext.h's gt2_options): the hard restart ADSR (altplayer.s from $F000),
 * multispeed (the caller's N calls; 0, 25 Hz, is one call), the pulse and realtime
 * optimisations, buffered writes, sound-effect support, author info (timing marks kept),
 * no optimisation, and the ghost registers: with them the player writes all 25 registers
 * as the main program copies them, before each call with full buffering (player.s $18 down
 * to 0, altplayer.s 0 up), after it from the zero page (gt2-ref's order, trace.rs). NTSC
 * changes nothing in a packed player. Not played: a custom SID clock (-F) and A-4's pitch
 * (-G), which make gt2reloc compute another frequency table; the player keeps the
 * built-in one.
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
    /* not player.s's: the row fetched for the next tick 0, for its row event */
    uint8_t orderpos;  /* the orderlist entry the playing pattern came from, $FF: elsewhere */
    uint8_t ev_pending, ev_pattern, ev_row, ev_orderpos;
} gt2_channel;

/* What one call did on a channel, for M2c Task 6's event ring (M2d's views, M2e's clock,
 * M2f's scene). gt2_play_frame fills gt2_player.ev for the channels the player plays. */
enum {
    GT2_EV_ROW = 1u << 0,     /* a row starts (its tick 0): pattern, row, order_pos */
    GT2_EV_NOTE = 1u << 1,    /* a note starts: note, instr */
    GT2_EV_SLIDE = 1u << 2,   /* a toneportamento's target: note, instr (no new start) */
    GT2_EV_GATE_ON = 1u << 3, /* the wave register's gate bit rose in this call */
    GT2_EV_GATE_OFF = 1u << 4 /* it fell */
};

typedef struct {
    uint8_t flags;        /* GT2_EV_* */
    uint8_t pattern, row; /* GT2_EV_ROW: the model's pattern and row */
    uint8_t order_pos;    /* GT2_EV_ROW: the pattern's orderlist entry; $FF if the packed
                           * player read it from past its orderlist */
    uint8_t note, instr;  /* GT2_EV_NOTE, GT2_EV_SLIDE: 0 is C-0, transposes applied */
    uint8_t gate;         /* the gate bit the wave register holds after the call (with full
                           * buffering the SID gets it at the next call) */
    uint8_t reserved;
} gt2_event;

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

/* How player.s lays out its variables (the reads below the frequency table, gt2_play.c). */
enum {
    GT2_LAYOUT_NORMAL = 1u << 0,      /* "Normal channel variables", else "Optimized" */
    GT2_LAYOUT_G4 = 1u << 1,          /* the frequency and pulse block */
    GT2_LAYOUT_G5 = 1u << 2,          /* the AD/SR, gate-off timer and last note block */
    GT2_LAYOUT_FIXEDPARAMS = 1u << 3, /* one gate-off timer and first wave: never stored */
    GT2_LAYOUT_NOINSVIB = 1u << 4,    /* no instrument vibrato: its delay never loaded */
    GT2_LAYOUT_NOCALCSPEED = 1u << 5  /* no note-independent speed: no last note kept */
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
    uint32_t tick0;          /* the tick 0 handlers assembled (gt2_play.c) */
    uint8_t layout;          /* GT2_LAYOUT_* */
    uint8_t class_count[3];  /* instruments in use: hard restart, no hard restart, legato */
    uint32_t pattused[(GT2_MAX_PATTERNS + 31u) / 32u]; /* the patterns the packer numbers */
    uint32_t instrused[2];   /* the instruments it numbers (bit i: instrument i) */
    gt2_channel chn[GT2_PLAY_CHANNELS];
    gt2_event ev[GT2_PLAY_CHANNELS]; /* the last call's events */
    uint8_t gates;                   /* each channel's gate bit after the last call */
} gt2_player;

/* Prepares the player for a subtune with the options (o NULL: GoatTracker's defaults), as
 * the packed player is after `JSR init` with A the subtune: the next call initialises the
 * song. 0, or -1 if the packed song would not hold the subtune (it has an empty orderlist
 * on channels 1-3, or lies past the subtunes gt2reloc packs). Walks the song (greloc's
 * analysis): the main loop's. */
int gt2_play_init(gt2_player* p, const gt2_song* s, uint32_t subtune, const gt2_options* o);

/* One call of the play routine: its SID writes, in order, to sink; then its events in
 * p->ev. */
void gt2_play_frame(gt2_player* p, gt2_sid_fn sink, void* ctx);

#ifdef __cplusplus
}
#endif

#endif
