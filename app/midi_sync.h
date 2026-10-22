// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI clock and transport for the song player (M2e Task 2): the clock follower
 * (app/midi_clock.h) driving the player's frame clock and transport, and MIDI clock out.
 *
 * **In.** Timing clocks, start (FA), continue (FB), stop (FC), song position (F2) and song
 * select (F3), from TRS and USB merged, as midi_rx parses them (midi_sync_msg, each with its
 * entry's stamp). With CLOCK at AUTO (the default) or FOLLOW and a clock arriving, the
 * player's call period follows the clock, and its calls land on the clock's 16ths:
 * - start plays the song from its start on the next clock: the play is held (song_ctl_hold)
 *   and its first call aligned to that clock (song_ctl_align); a stopped song is first sought
 *   back to its start, so it needs no new analysis;
 * - continue resumes on the next clock, held and aligned the same way; one that arrives while
 *   a song position's seek runs starts when the seek is done, the seek's target moved on by
 *   the clocks that came meanwhile (song_ctl_seek_further), so the song jumps ahead by the
 *   frames it missed; for a song never played, a continue with no song position before it is
 *   a start (the song from its first call, on the next clock);
 * - stop stops (the releases ring, as song_ctl_stop does);
 * - a song position, while the song is stopped or not started, seeks the player there in the
 *   main loop's slices (song_ctl_seek: a fast-forward handed over at the next call); rows =
 *   position x rows a beat / 4, calls = rows x the song's calls a row now. A song position
 *   while playing is ignored (a sequencer sends one only while stopped);
 * - song select's subtune is kept for the next start (song_transport plays the song's first
 *   until it takes a subtune).
 * Following: once a beat is known (MIDI_CLOCK_LOCK clocks), at each row the next call's time
 * is compared with its clock's (the clock's fit at the call's position, MIDI_SYNC_DELAY
 * later): an error past MIDI_SYNC_ALIGN_SAMPLES re-aligns the next call at once, a smaller
 * one goes into the next beat's period (midi_clock_frame_q16, at most 1 % of a beat), so the
 * phase never drifts. A song started from the panel while a clock runs follows its tempo
 * only (it has no position on the clock).
 *
 * **The delay.** Every call lands MIDI_SYNC_DELAY samples after its clock reached the jack:
 * the clock's stamp says when it came, and the call it places must not yet be rendered when
 * the command reaches the interrupt: the block being heard and the next, already rendered,
 * plus the stamp's age (decision record 0005's "one block plus a poll", with the render
 * ahead). A constant: a sequencer that compensates its outputs' latency takes it off. A
 * command that comes later than that places its call at the earliest block it still can.
 *
 * **The clock that stops.** No clock for MIDI_CLOCK_LOST_MS: with AUTO the song carries on at
 * the last tempo (its period stays), with FOLLOW it stops. CLOCK OFF ignores clock and
 * transport: the song runs on its own frames.
 *
 * **Out** (USB only: the FM-1 has no TRS out), with CLOCK OUT on and no clock followed:
 * MIDI_CLOCK_PER_BEAT clocks a beat from the player's calls (a beat is the song's calls a row
 * now times rows a beat), each sent in the first main-loop pass once its time has come
 * (midi_sync_poll), and start, stop, continue (after a song position) as the player's
 * transport changes. The song position waits while a SysEx frame is part way out
 * (out_common_wait): real-time bytes may go inside a frame, system common ends it. A clock that comes in is never echoed: while one is followed nothing
 * goes out.
 *
 * **Time.** The stamps are fm1_ticks() (24 MHz, MIDI_SYNC_TICKS_PER_SAMPLE a sample); the
 * player counts samples (song_ctl.samples). midi_sync_anchor ties the two once a block: the
 * interrupt's block that starts at sample `samples` was rendered at tick `ticks` and plays
 * one block later (MIDI_SYNC_LATENCY). Everything here is in the player's samples, as heard.
 *
 * The player is the song views' transport (app/song_transport.h, M2d) with its clock members
 * (hold, align, period, seek, seek_further, clock), bound over song control in
 * platform/fm1/main.c; the host tests bind it over the real song_ctl. Song select's subtune is
 * kept for a transport that plays one (song_transport's play takes an order position).
 *
 * Portable C with no hardware access. Main context. */
#ifndef SIDOU_MIDI_SYNC_H
#define SIDOU_MIDI_SYNC_H

#include <stdint.h>

#include "midi_clock.h"
#include "midi_in.h"
#include "song_transport.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The CLOCK setting. */
typedef enum {
    MIDI_SYNC_AUTO = 0,   /* follow a clock while one arrives; the song carries on without */
    MIDI_SYNC_FOLLOW = 1, /* follow; the song stops when the clock does */
    MIDI_SYNC_OFF = 2,    /* the song on its own frames; MIDI transport ignored */
    MIDI_SYNC_MODES = 3
} midi_sync_mode;

/* 24 MHz / 544: ticks a sample. */
#define MIDI_SYNC_TICKS_PER_SAMPLE 544u
/* A block rendered at the anchor plays this many samples later. */
#define MIDI_SYNC_LATENCY 128u
/* A call lands this long after its clock came in, 8 ms: the block being heard, the next
 * one (already rendered when a command is sent), and the stamp's age when the main loop takes
 * it (the TRS poll, the drain), up to 96 samples (2.2 ms). */
#define MIDI_SYNC_DELAY 352u
/* A phase error past this (2 ms) re-aligns at once. */
#define MIDI_SYNC_ALIGN_SAMPLES 88u
/* The packets midi_sync_poll writes at most a pass. */
#define MIDI_SYNC_OUT_MAX 4u
/* USB-MIDI's cable for out (midi_out.h's). */
#define MIDI_SYNC_CABLE 0u

/* The player's states: song_transport's (SONG_T_IDLE, SONG_T_STARTING, SONG_T_PLAYING,
 * SONG_T_STOPPED). */
enum { MIDI_SYNC_IDLE = 0, MIDI_SYNC_LOADING = 1, MIDI_SYNC_PLAYING = 2, MIDI_SYNC_STOPPED = 3 };

/* What a pass wants of the player (the pending requests). */
enum {
    MIDI_SYNC_WANT_START = 1u,
    MIDI_SYNC_WANT_RESUME = 2u,
    MIDI_SYNC_WANT_STOP = 4u,
    MIDI_SYNC_WANT_SEEK = 8u,
    MIDI_SYNC_WANT_FREE = 16u
};

typedef struct midi_sync {
    midi_clock clock;
    const song_transport* t; /* the player (midi_sync_bind), or NULL: nothing happens */
    uint8_t mode;        /* a midi_sync_mode */
    uint8_t clock_out;   /* CLOCK OUT: 0 off, 1 on */
    uint8_t rows_per_beat;
    uint8_t subtune;     /* song select's */
    uint8_t want;        /* MIDI_SYNC_WANT_* */
    uint8_t held;        /* a held play/resume remains outstanding, including during a seek */
    uint8_t aligning;    /* a held start or resume waits for its clock */
    uint8_t synced;      /* the playing song has a position on the clock */
    uint8_t fresh;       /* a clock has come since the last pass */
    uint32_t seek_call;  /* MIDI_SYNC_WANT_SEEK's target */
    uint32_t seek_pos;   /* and its clock position */
    uint8_t seek_on;     /* a seek of ours runs */
    /* the anchor between calls and clock positions: call c_anchor at clock p_anchor */
    uint32_t c_anchor, p_anchor;
    uint32_t next_row;   /* the call at which the period is looked at again */
    /* the time base (midi_sync_anchor) */
    uint32_t a_ticks, a_samples;
    uint8_t anchored;
    /* out */
    uint8_t out_state;   /* the player's state as clock out last saw it */
    /* The caller's, before each poll: 1 while a SysEx frame is part way out on clock out's
     * cable, so a song position (system common, which would end the frame) waits, and the
     * continue after it with it (midi_sync_poll sends neither until it is 0). */
    uint8_t out_common_wait;
    uint32_t out_clock;  /* the next clock's number since the start */
    uint32_t sent;       /* packets written */
    /* counters, for the tests */
    uint32_t aligns, periods, lost;
} midi_sync;

/* Nothing bound; AUTO, CLOCK OUT off, 4 rows a beat, subtune 0. */
void midi_sync_init(midi_sync* s);
/* The player: the song views' transport (app/song_transport.h) with its clock members; one
 * without them, or NULL, leaves clock and transport doing nothing. */
void midi_sync_bind(midi_sync* s, const song_transport* t);
/* A new stopped song: cancel the old song's pending transport, retaining clock fit/settings. */
void midi_sync_song_changed(midi_sync* s);
/* The settings (clamped). */
void midi_sync_set(midi_sync* s, uint32_t mode, uint32_t clock_out);
/* The song's rows a beat (MIDI's mode, app/midi_route.h). */
void midi_sync_rows(midi_sync* s, uint32_t rows_per_beat);
/* The interrupt's latest block: rendered at `ticks`, starting at sample `samples`. */
void midi_sync_anchor(midi_sync* s, uint32_t ticks, uint32_t samples);
/* A stamp (ticks) as the player's sample heard then. */
uint32_t midi_sync_sample_of(const midi_sync* s, uint32_t ticks);
/* A message from midi_in with its entry's stamp: the real-time ones, song position and song
 * select (anything else is ignored). */
void midi_sync_msg(midi_sync* s, const midi_msg* msg, uint32_t ticks);
/* 1 while a clock is followed (CLOCK not OFF and a clock arriving). */
int midi_sync_following(const midi_sync* s);
/* Once a main-loop pass, at `ticks`: the requests to the player, the alignment and the
 * period, then clock out's packets (USB-MIDI, midi_usb.h) into out, at most
 * MIDI_SYNC_OUT_MAX; their count. */
uint32_t midi_sync_poll(midi_sync* s, uint32_t ticks, uint32_t out[MIDI_SYNC_OUT_MAX]);

/* The settings' names (at most 9 characters). */
const char* midi_sync_mode_name(uint32_t mode);

#ifdef __cplusplus
}
#endif

#endif
