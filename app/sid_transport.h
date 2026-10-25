// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* What the SID views (app/sid_view.h) and the song views' GLO need of the .sid player (M3 Task
 * 11; https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md, "The browser and the transport"): the tune's
 * transport, its mutes and what it is doing, as a table of functions, as song_transport.h is the
 * song's.
 *
 * The device binds it over main.c's tune (its states, the store's file through app/sid_io.h,
 * core/psid/psid_ctl.h); the host tests bind a fake. With none bound (sid_view_set_transport
 * NULL) the browser still lists the store's tunes, and choosing one does nothing.
 *
 * The device's binding:
 * - play: the store's .sid file `name`, its start song. With no tune loaded the song stops and
 *   lets go of its memory first (song_io_away discards its unsaved edits: the views ask before,
 *   app/sid_view.h); with one loaded, it stops and the new file loads in its place.
 * - start: the loaded tune's song `song` (1..songs) from init: a stop, its fade played out, the
 *   file read again from the store (sid_io: a file loaded in parts has no song change of its
 *   own), then psid_ctl_play. A request made while the tune loads waits for it; the last wins.
 * - stop, pause, resume: psid_ctl's. A stop asked while the tune loads stops it as it starts.
 * - end: the tune for good: its stop played out, the song back from the store.
 * - mute: GLO's mask, bit v voice v (solo folded in, song_view_mute_mask): on chip A's voices,
 *   and on voice v of both chips for a 2SID tune; kept for the tune's next start too.
 * - status: once a frame at most, from the main loop.
 * Every command returns at once; 1 taken, 0 refused (nothing loaded).
 *
 * Portable C, no hardware access. */
#ifndef SIDOU_SID_TRANSPORT_H
#define SIDOU_SID_TRANSPORT_H

#include <stdint.h>

#include "psid/psid_file.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The tune's state. */
#define SID_T_NONE 0u    /* no tune: the song has its memory */
#define SID_T_LOADING 1u /* asked for: the song lets go, the file loads, the chips are made */
#define SID_T_PLAYING 2u /* its init or its play calls run */
#define SID_T_PAUSED 3u
#define SID_T_STOPPED 4u
#define SID_T_FAILED 5u  /* it stopped itself (INIT TOO LONG, PLAY HUNG, a JAM) */
#define SID_T_ENDING 6u  /* ending for good: the song comes back */

/* What the tune is doing. */
typedef struct {
    uint8_t state;     /* SID_T_* */
    uint8_t chips;     /* 1, or 2 for a 2SID tune */
    uint8_t shadow;    /* 1: the voice shadows answer OSC3 and ENV3 */
    uint16_t song;     /* the song playing, or asked for (1..songs); 0 none yet */
    uint32_t seconds;  /* the audio's time since the song started */
    uint32_t cycles;   /* the 6502's cycles over the last second */
    uint16_t load;     /* the 6502's share of the CPU over the last second, tenths of a percent */
    uint16_t ticks_k;  /* 24 MHz ticks a thousand 6502 cycles took (0 with load: not measured) */
    uint32_t overruns; /* play calls that ran past their period */
    uint32_t late;     /* SID writes applied after their cycle */
    uint32_t dropped;  /* a third SID's writes, dropped */
    uint8_t env[3];    /* chip A's voices' envelopes, 0..255 */
    uint8_t model[2];  /* each chip's model as it plays the tune (sid_model) */
    char name[STORE_NAME_MAX + 1u]; /* the store's file */
    const psid_header* h; /* the tune's header while it is read (NULL before) */
} sid_status;

typedef struct {
    int (*play)(void* ctx, const char* name);
    int (*start)(void* ctx, uint32_t song);
    int (*stop)(void* ctx);
    int (*pause)(void* ctx);
    int (*resume)(void* ctx);
    int (*end)(void* ctx);
    int (*mute)(void* ctx, uint32_t mask);
    void (*status)(void* ctx, sid_status* out);
    void* ctx;
} sid_transport;

#ifdef __cplusplus
}
#endif

#endif
