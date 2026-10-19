// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* What the song views (app/song_view.h) need of the song's player: the transport, the mutes,
 * the jam, and what the player is doing, as a table of functions (M2d Task 1).
 *
 * The device binds it over M2c Task 6's song control (core/player/song_ctl.h) in
 * platform/fm1/main.c; the host tests bind a fake. With none bound (song_view_set_transport
 * NULL) the views still run: the header shows the transport stopped and no tempo, PLAY does
 * nothing, the note keys play the instrument in the song views too, and the mutes and solos
 * are kept and shown but reach nothing.
 *
 * The device's binding over song_ctl:
 * - play: song_ctl_play(subtune 0, order_pos); the keys leave chip A (keys_route_set_song)
 *   while the song has it, and the song starts once keys_route_ready_for_song and chip A has
 *   the song's setup (song_ctl's chip hooks): until then the state is SONG_T_STARTING. stop:
 *   song_ctl_stop (the keys come back once it is applied); asked again while it starts.
 *   resume: song_ctl_continue, which takes chip A again first if it went back meanwhile.
 * - mute: song_ctl_mute(mask), solo already folded into the mask (song_view_mute_mask).
 * - jam: song_ctl_jam(voice, note, instr) while the song plays; a release (SONG_T_RELEASE)
 *   is song_ctl's SONG_JAM_RELEASE, the voice's gate off as a key-off row gives it. With the
 *   song stopped, the jam goes through song_ctl's audition (song_ctl_audition: chip A taken
 *   for the song, the calls running with the sequencer held), the note waiting until it is
 *   on; it ends two seconds after the last jam is let go.
 * - status: once a frame, from the main loop: state from song_ctl_shown_state; position
 *   from the row events (song_ctl_events, which the binding drains: it is their one reader);
 *   speed from the first voice's tempo (funktempo's two speeds); multispeed and ntsc from the
 *   song's options; mute as applied; wave from the player's shadow of each voice's control
 *   register; env from chip A's envelope counters.
 * - M2e's clock members (below): song_ctl_hold, song_ctl_align, song_ctl_set_period,
 *   song_ctl_seek, song_ctl_seek_further, and the calls' samples song control publishes.
 * Every command returns at once (the main loop's, no waiting); a 0 (song_ctl's ring full) is
 * asked again by the views at the next frame.
 *
 * Portable C, no hardware access. */
#ifndef SIDOU_SONG_TRANSPORT_H
#define SIDOU_SONG_TRANSPORT_H

#include <stdint.h>

#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_T_VOICES 3u

/* The transport's state, in song_ctl_state's order (SONG_IDLE, SONG_LOADING, SONG_PLAYING,
 * SONG_STOPPED), so the binding passes it through. */
#define SONG_T_IDLE 0u     /* nothing played yet: resume plays from the start */
#define SONG_T_STARTING 1u /* asked to play: the analysis, or chip A not the song's yet */
#define SONG_T_PLAYING 2u
#define SONG_T_STOPPED 3u  /* stopped, resume goes on from there */

/* jam's note for a release: the voice's gate off. */
#define SONG_T_RELEASE 0xffu
/* status's position before a row event says it. */
#define SONG_T_NOWHERE 0xffu

/* What the player is doing, once a frame. */
typedef struct {
    uint8_t state;      /* SONG_T_* */
    uint8_t multispeed; /* play routine calls a frame: 1-4, 0 for 25 Hz */
    uint8_t ntsc;       /* 1: NTSC's frame, 0 PAL's */
    uint8_t mute;       /* the voices the player keeps silent now (bit v: voice v) */
    /* Play routine calls a row, the song's tempo now: both the same, or funktempo's two that
     * alternate; 0 0 before the song has a tempo. */
    uint8_t speed[2];
    uint8_t order_pos[SONG_T_VOICES]; /* each voice's: the orderlist entry it plays */
    uint8_t row[SONG_T_VOICES];       /* and the row, SONG_T_NOWHERE until a row event */
    uint8_t wave[SONG_T_VOICES];      /* each voice's control register as last written */
    uint8_t env[SONG_T_VOICES];       /* each voice's envelope, 0..255 */
    uint8_t subtune; /* actual playing subtune, independent of the UI selection */
    uint32_t applied; /* command acknowledgements plus idle edits: pattern data may have changed */
} song_status;

/* What M2e's MIDI clock needs of the player (app/midi_sync.h), as often as it asks: the state
 * in song_ctl_state's order (SONG_T_STARTING for a load, including a held start's), whether a
 * seek runs its slices (1) or waits for its call (2), the speed (as song_status's; 0 0
 * before a play), the calls since the play and the last one's sample on song control's count
 * (song_ctl.samples), and the samples a call now (Q16). */
typedef struct {
    uint8_t state;
    uint8_t seeking;
    uint8_t speed[2];
    uint32_t calls;
    uint32_t call_sample;
    uint32_t period_q16;
} song_clock;

enum { SONG_EDIT_ROW = 1, SONG_EDIT_PUSH, SONG_EDIT_PULL };
typedef struct {
    uint32_t kind;
    uint32_t pattern, row;
    uint8_t cell[GT2_ROW_BYTES];
} song_edit_request;
enum { SONG_EDIT_GUARD_OK, SONG_EDIT_GUARD_SAVING, SONG_EDIT_GUARD_FULL, SONG_EDIT_GUARD_BUSY };

/* The player's side. Every member up to ctx is set; the MIDI clock's after it may be NULL
 * (a binding without them: MIDI clock and transport do nothing). */
typedef struct {
    /* The song from orderlist entry order_pos of each voice. 1: taken, 0: not now. */
    int (*play)(void* ctx, uint32_t order_pos);
    int (*stop)(void* ctx);
    /* On from where it stopped. */
    int (*resume)(void* ctx);
    /* The voices to keep silent (bit v: voice v): a voice's gate off, then its writes held. */
    int (*mute)(void* ctx, uint32_t mask);
    /* Voice voice takes note (0 is C-0, ..95) with instrument instr (1-63) at its next
     * tick, as if its next row held them; SONG_T_RELEASE lets it go. */
    int (*jam)(void* ctx, uint32_t voice, uint32_t note, uint32_t instr);
    void (*status)(void* ctx, song_status* out);
    void* ctx;
    /* The song it plays, for the views to read (never to write: edits go through the player,
     * which applies them between its calls); NULL: none. */
    const gt2_song* song;
    /* M2e Task 2's MIDI clock (app/midi_sync.h), over song control's: on, the next play or
     * resume is held, playing nothing, until align places its first call (song_ctl_hold); the
     * next call at a sample (song_ctl_align); samples a call, Q16, 0 the song's own
     * (song_ctl_set_period); a seek to call `call` from the play's start, and a running one
     * moved on (song_ctl_seek, song_ctl_seek_further); and the clock's view of the player. The
     * status above drains the song's events: clock does not. */
    int (*hold)(void* ctx, uint32_t on);
    int (*align)(void* ctx, uint32_t sample);
    int (*period)(void* ctx, uint32_t q16);
    int (*seek)(void* ctx, uint32_t call);
    int (*seek_further)(void* ctx, uint32_t call);
    void (*clock)(void* ctx, song_clock* out);
    /* Main-loop edits: a 1 transfers ownership and returns an edit ticket. Completion means
     * applied AND released (including the main-loop lock move), not just a command ack.
     * The prospective guard runs before any primitive; absent edit hooks refuse editing.
     * A save must not start while accepted edits await completion; guard refusals cannot
     * revoke an edit after ownership has transferred to the player. */
    int (*edit)(void* ctx, const gt2_edit* edit, uint32_t* ticket);
    int (*edit_done)(void* ctx, uint32_t ticket);
    int (*edit_guard)(void* ctx, const song_edit_request* request);
    /* Undo of a shift restores its lost row's packed locks with the swap. The bytes stay
     * alive until completion; the player prepares the prospective lock cache before send. */
    int (*edit_with_locks)(void* ctx, const gt2_edit* edit, const uint8_t* locks,
                           uint32_t nlocks, uint32_t* ticket);
    /* Optional selected-subtune PLAY. The original play remains MIDI's subtune-zero start. */
    int (*play_from)(void* ctx, uint32_t subtune, uint32_t order_pos);
} song_transport;

#ifdef __cplusplus
}
#endif

#endif
