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
    uint32_t applied; /* command acknowledgements plus idle edits: pattern data may have changed */
} song_status;

/* The player's side. Every member is set. */
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
} song_transport;

#ifdef __cplusplus
}
#endif

#endif
