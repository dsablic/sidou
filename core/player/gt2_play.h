// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player (M2c Tasks 4-6): GoatTracker 2's packed play routine in C, playing a song
 * from the model (gt2_song.h) as GoatTracker 2.77's player.s plays it once gt2reloc has
 * packed it with the song's options (decision record 0007, "Equivalence, defined").
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
 * one byte, and so on. The player follows the shape the packer would give the song: a
 * gt2_shape, which gt2_shape.h's analysis computes from the song (greloc's analysis: the
 * patterns its subtunes name, the instruments and table rows they reach). The analysis walks
 * the song and runs in the main loop, in slices; this file is the frame path, which reads
 * only O(1) state each step, and runs in the audio interrupt on the device (M2c Task 6).
 *
 * The shape also holds the packer's numberings the 6502 can read in corners (the packed
 * pattern and instrument numbers, the variables below the frequency table) as tables, and
 * the subtune's orderlists flattened into the packed byte streams player.s indexes with
 * its 8-bit song pointer, so an orderlist that breaks readme 3.1's rules and reads on past
 * its restart byte costs a read like any other.
 *
 * The packer also rewrites the data (instruments reordered with hard restart ones first,
 * patterns packed, tables remapped, wavetable bytes recoded, tempos less one). The player
 * reads the model's .sng bytes for patterns, instruments and tables and applies each
 * rewrite where it reads them, so a cell edit there plays at once. An orderlist edit plays
 * through the shape (its streams), which the main loop rebuilds and swaps in with the edit.
 *
 * The options (song_ext.h's gt2_options): the hard restart ADSR (altplayer.s from $F000),
 * multispeed (the caller's N calls; 0, 25 Hz, is one call), the pulse and realtime
 * optimisations, buffered writes, sound-effect support, author info (timing marks kept),
 * no optimisation, and the ghost registers: with them the player writes all 25 registers
 * as the main program copies them, before each call with full buffering (player.s $18 down
 * to 0, altplayer.s 0 up), after it from the zero page (gt2-ref's order, trace.rs). NTSC
 * changes nothing in a packed player, and neither does a custom SID clock (-F): gt2reloc
 * 2.77 parses it and never uses it. Not played: A-4's pitch (-G, SIDOU's OPTS a4_millihz),
 * from which gt2reloc computes another frequency table; the player keeps the built-in one
 * and reports it (GT2_PLAY_WARN_PITCH).
 *
 * The contract with the song model (gt2_song.h): the player re-reads a pattern's index entry
 * each time it reads the pattern, and keeps no arena pointer from one call to the next.
 *
 * Portable C11, no allocation, no libm, no calls into libc: the device runs this file from
 * SRAM in the audio interrupt, compiled at -Os (it runs once a frame, not a sample). */
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
/* player.s's song pointer is an 8-bit index into the channel's packed orderlist. */
#define GT2_ORDER_STREAM 256u
/* The bytes below the packed frequency table the player can name: the ghost registers and
 * five blocks of seven variables a channel (gt2_play.c, "Frequencies"). */
#define GT2_VAR_MAX (GT2_SID_REGS + 5u * 7u * GT2_PLAY_CHANNELS)

/* One SID write: a register 0-24 (offsets from $D400) and its value. */
typedef void (*gt2_sid_fn)(void* ctx, uint8_t reg, uint8_t value);

/* A channel's variables, player.s's mt_chn* (the names are its own, less the prefix). */
typedef struct {
    uint8_t songptr;   /* the packed orderlist byte the sequencer reads next */
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
    /* Steps whose note is dropped when read, (pattern + 1) << 8 | row, 0: none. The main
     * loop's (song_ctl's chance rolls), up to the two rows a pattern's end may lead to. */
    uint16_t skip[2];
    uint16_t encoding; /* retained pulse/wave packing, or 0: follow the shape */
} gt2_channel;

/* What one call did on a channel, for M2c Task 6's event ring (M2d's views, M2e's clock,
 * M2f's scene). gt2_play_frame fills gt2_player.ev for the channels the player plays; a
 * channel the packer does not play (gt2_shape.channels < 3) has no events, and its
 * orderlist's position must be followed another way (M2d's cursors). */
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

/* What the song and its options make of player.s (gt2_shape.flags). */
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

/* The tick 0 handlers the packer assembles (gt2_shape.tick0): bit c for commands 5-E, and
 * the tempo command's two parts. */
#define GT2_TICK0_TEMPO_GLOBAL (1u << 16)
#define GT2_TICK0_TEMPO_CHANNEL (1u << 17)

/* What gt2_shape could not follow, for the UI to show (the song plays regardless). */
enum {
    GT2_PLAY_WARN_PITCH = 1u << 0 /* OPTS asks for another A-4 (gt2reloc's -G): gt2reloc
                                   * computes another frequency table, with libm; the player
                                   * keeps the built-in one */
};

/* A byte below the frequency table (gt2_shape.var): what player.s keeps there. */
#define GT2_VAR_ZERO 0xfeu    /* a variable the assembly never writes, or none: 0 */
#define GT2_VAR_UNKNOWN 0xffu /* the player's own code or data, which only the packed layout
                               * knows */
#define GT2_VAR_GHOST 0x80u   /* | register: the ghost register */
/* Otherwise channel << 5 | one of these (gt2_play.c reads the channel's variable). */
enum {
    GT2_VF_ORDIDX, GT2_VF_PATTNUM, GT2_VF_INSTR, GT2_VF_TEMPO, GT2_VF_COUNTER, GT2_VF_NOTE,
    GT2_VF_GATE, GT2_VF_VIBTIME, GT2_VF_VIBDELAY, GT2_VF_WAVETIME, GT2_VF_FREQLO,
    GT2_VF_FREQHI, GT2_VF_PULSELO, GT2_VF_PULSEHI, GT2_VF_AD, GT2_VF_SR, GT2_VF_GATETIMER,
    GT2_VF_LASTNOTE, GT2_VF_PULSETIME
};

/* The packed player's shape for a song, its options and a subtune: everything gt2reloc
 * decides that changes the writes, and its numberings, as tables. Built by gt2_shape.h in
 * the main loop; read by the player through a pointer (gt2_play_set_shape swaps it), so the
 * main loop builds the next one in another buffer and never writes the one in use. */
typedef struct {
    uint16_t flags;          /* GT2_PF_* */
    uint8_t warnings;        /* GT2_PLAY_WARN_* */
    uint8_t subtune;         /* the packed subtune whose orderlists the streams hold */
    uint8_t songs;           /* the subtunes the packer writes orderlists for */
    uint8_t channels;        /* NUMCHANNELS: 1-3, the channels the packed player runs */
    uint8_t ad_param, sr_param; /* the hard restart ADSR */
    uint8_t reset_instr;     /* the instrument the packer numbers 1 (each channel's first) */
    uint8_t first_legato;    /* multispeed only: the legato instrument played as no-HR */
    uint8_t default_tempo;   /* DEFAULTTEMPO */
    uint8_t first_note, last_note; /* the frequency table the packer keeps */
    uint8_t layout;          /* GT2_LAYOUT_* */
    uint8_t var_count;       /* the bytes var describes */
    uint8_t reserved;
    uint32_t tick0;          /* the tick 0 handlers assembled: bits 5-14, GT2_TICK0_TEMPO_* */
    /* var[k] is the byte k + 1 below the frequency table: GT2_VAR_* or channel << 5 |
     * GT2_VF_*. */
    uint8_t var[GT2_VAR_MAX];
    uint8_t pat_num[GT2_MAX_PATTERNS]; /* the packer's number of a model pattern, $FF none */
    uint8_t ins_num[GT2_MAX_INSTR + 1u]; /* the packer's number of an instrument, 0 none */
    /* Each channel's packed orderlist as player.s reads it from its start: the entries (a
     * repeat after its pattern, R0 dropped), $FF and the restart byte, then whatever the
     * packer wrote next (the next orderlists; $00 past the last). */
    uint8_t ord[GT2_PLAY_CHANNELS][GT2_ORDER_STREAM];
    /* For a byte the sequencer takes as a pattern: its entry in the model's orderlist, $FF
     * outside the channel's own entries, GT2_ORD_RAW for a byte read as the packed player
     * holds it (a restart byte, or past every orderlist): as a pattern number it is the
     * packer's. */
    uint8_t ord_pos[GT2_PLAY_CHANNELS][GT2_ORDER_STREAM];
} gt2_shape;
#define GT2_ORD_RAW 0xfeu

/* A wavetable draft heard before it is written (1.1, TABLE's ARP helper): on one channel the
 * wavetable's rows [first, first + len) read as these (left bytes, then right bytes), and a
 * jammed note there starts its wave program at first. channel GT2_DRAFT_OFF: none. */
#define GT2_DRAFT_ROWS 10u
#define GT2_DRAFT_OFF 0xffu
typedef struct {
    uint8_t channel, first, len, pad;
    uint8_t rows[2u * GT2_DRAFT_ROWS];
} gt2_draft;

typedef struct {
    const gt2_song* song;
    const gt2_shape* shape;
    uint8_t initsong;        /* 1 until the first call initialises the song */
    uint8_t filtstep, filttime, filtcutoff, filtctrl, filttype, masterfader;
    uint8_t funktempo[2];
    uint8_t timing_mark;     /* DXY's mark, with author info */
    /* A bit a channel: its frequency came from past the packed frequency table, where the
     * 6502 reads bytes only the packed layout knows (gt2_play.c, "Frequencies"). */
    uint8_t freq_unknown;
    uint8_t unknown;         /* scratch: the last frequency reads hit that region */
    uint8_t gates;           /* each channel's gate bit after the last call */
    /* 1: the sequencer holds (M2d's audition with the song stopped, song_ctl.h): no row,
     * orderlist or pattern is read. Jammed channels play their instruments and tables as if
     * every row were empty; unrequested channels hold their state. 0 for every song played. */
    uint8_t frozen;
    uint8_t jam_voices; /* explicitly jammed voices; unpacked ones are never sequenced */
    uint8_t draft_jam;  /* a jam on the draft's channel: its note starts the draft */
    uint8_t start[GT2_PLAY_CHANNELS]; /* the packed orderlist byte the song starts each
                                       * channel from (0: its start) */
    uint8_t regs[GT2_SID_REGS]; /* the SID's registers as written (the ghost registers) */
    gt2_channel chn[GT2_PLAY_CHANNELS];
    gt2_event ev[GT2_PLAY_CHANNELS]; /* the last call's events */
    /* NULL or a draft (song_ctl's), read by the wavetable's reads; last: gt2_play_start and
     * song_ctl's hand-over leave it as it is */
    const gt2_draft* draft;
} gt2_player;

/* The player as the packed player is after `JSR init` (its draft kept: gt2_player.draft): the next call initialises the song,
 * whose channels start at start[] (packed orderlist bytes, gt2_shape_order_byte; NULL:
 * their starts). O(1): the audio interrupt's. The shape must stay as it is until another
 * replaces it (gt2_play_set_shape) or the player is started again. */
void gt2_play_start(gt2_player* p, const gt2_song* s, const gt2_shape* shape,
                    const uint8_t* start);

/* From the next call on the player follows shape (one pointer store: the audio
 * interrupt's, between calls). It must be the same song's and subtune's, with the same
 * channel count: a shape that changes how the player's state reads (gt2_shape.h,
 * gt2_shape_compatible) goes through a hand-over of a fast-forwarded player instead. */
void gt2_play_set_shape(gt2_player* p, const gt2_shape* shape);
/* A live audition cannot be reconstructed by replaying the song. Retain every running
 * table's encoding and all channel/filter state, initializing only newly packed voices.
 * New table pointers take the new shape's encoding. */
void gt2_play_retain_shape(gt2_player* p, const gt2_shape* shape);

/* One call of the play routine: its SID writes, in order, to sink; then its events in
 * p->ev. */
void gt2_play_frame(gt2_player* p, gt2_sid_fn sink, void* ctx);

/* Channel ch takes note (0 C-0 ... 95) and instrument instr (1-63; 0 keeps the channel's) as
 * if its next row held them with no command (player.s's mt_getnewnote): the note starts at
 * the channel's next tick 0, after the hard restart its instrument asks for, whose writes go
 * to sink as the row's would. Between calls (M2c Task 6's jam command). */
void gt2_play_jam(gt2_player* p, uint32_t ch, uint8_t note, uint8_t instr, gt2_sid_fn sink,
                  void* ctx);

/* A new or changed draft on its channel (between calls): a wave pointer left inside the draft's
 * rows past its end, or on its jump, goes back to first (the shape reads delays while a draft is
 * on: song_ctl_draft, gt2_analysis.xwave). */
void gt2_play_draft(gt2_player* p, const gt2_draft* d);

#ifdef __cplusplus
}
#endif

#endif
