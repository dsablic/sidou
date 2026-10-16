// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* What the song views (app/song_view.h) need of the song's player: the transport, the mutes,
 * the jam, and what the player is doing, as a table of functions (M2d Task 1).
 *
 * The device binds it over M2c Task 6's song control (core/player/song_ctl.h) in
 * platform/fm1/main.c; the host tests bind a fake. Until the device binds one, the views run
 * without (song_view_set_transport NULL): the header shows the transport stopped and no
 * tempo, PLAY does nothing, the note keys play the instrument in the song views too, and the
 * mutes and solos are kept and shown but reach nothing.
 *
 * What the binding over song_ctl has to do:
 * - play: song_ctl_play(subtune 0, order_pos), and keys_route_set_song(1), so the keys leave
 *   chip A; the song starts only once keys_route_ready_for_song (song_ctl's chip hooks), and
 *   until then state is SONG_T_STARTING. stop: song_ctl_stop (the keys come back once it is
 *   applied). resume: song_ctl_continue.
 * - mute: song_ctl_mute(mask), solo already folded into the mask (song_view_mute_mask).
 * - jam: song_ctl_jam(voice, note, instr); a release (SONG_T_RELEASE) is the voice's gate off
 *   as a key-off row would give it. The song views audition with the song stopped as well, so
 *   the player's jam has to sound without the song playing.
 * - status: once a frame, from the main loop: state from song_ctl_state; position from the
 *   row events (song_ctl_events, which the binding drains: it is their one reader); speed from
 *   the player's tempo (funktempo's two speeds); multispeed and ntsc from the song's options;
 *   mute as last applied; wave from the player's shadow of each voice's control register; env
 *   from chip A (ENV3 for voice 3; voices 1 and 2 from their gate in the events where nothing
 *   better is at hand: 255 while gated, 0 once not).
 * Every command returns at once (the main loop's, no waiting); a 0 (song_ctl's ring full) is
 * asked again by the views at the next frame.
 *
 * Portable C, no hardware access. */
#ifndef SIDOU_SONG_TRANSPORT_H
#define SIDOU_SONG_TRANSPORT_H

#include <stdint.h>

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
} song_transport;

#ifdef __cplusplus
}
#endif

#endif
