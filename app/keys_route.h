// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Which chip the keys play (M2b Task 4; docs/plans/2026-10-15-m2b-chip-b.md, the user's
 * decision of 2026-10-16), and the two chips' setups.
 *
 * live plays the instrument through this sink, which sends its writes to the chip the keys
 * play: chip A with no song playing, exactly as in M1; chip B while a song plays on chip A
 * (KEYS AUTO, the default); chip A always with KEYS A, where during a song a key takes a
 * voice from it (keys_route_voices_taken: the song's writes to those voices are held back, as
 * M2c's mute does). While a song plays on chip A with the keys on it, chip A is the song's:
 * live's setup writes (the character, resets, the gate reset) and its writes to $15-$18 (the
 * filter and the volume) are dropped, and of a voice's registers only those of the voices the
 * keys hold get through, and each voice's gate-off as the keys let it go (the keys sound
 * through the song's setup). Until M2c, "a song playing" is keys_route_set_song, which
 * nothing on the device calls.
 *
 * For M2c's song control (song_ctl), the contract:
 * - the transport starts a song only once keys_route_ready_for_song: the keys have left chip
 *   A (KEYS AUTO, chip B ready) or KEYS is A. Until then (a profile step just before PLAY, a
 *   song autostarting at boot) the keys may still be on chip A, filtered as above;
 * - every setup it gives chip A (the song's CHPA) is told with keys_route_song_setup, so the
 *   keys are taken back to their own setup after the song (and STATUS shows chip A's);
 * - on the falling edge of a voice's bit in keys_route_voices_taken it rewrites that voice's
 *   registers (the keys' writes to it stop then);
 * - a pass in which the song control writes chip A (28 entries at most) and keys_route moves
 *   or takes chip A back (30) and chip B follows (4) is within REG_RING_WORST_PASS; M2c Task
 *   6 measures that shared pass.
 *
 * The keys' setup travels with the keys. live's model, profile, character and envelope mode
 * are the keys' setup; the chip the keys play gets it from live, and the other chip follows
 * it in the background (chip_ctl), a slice a tick after the keys' own chip, so chip B holds
 * the keys' setup whenever the keys move to it. Chip A, while a song plays on it, is the
 * song's (M2c sets its setup) and follows nothing; when the song stops it follows the keys'
 * setup again.
 *
 * Moves happen in keys_route_tick, once a pass after the live ticks:
 * - to chip B, once chip B has the keys' setup: the notes the keys hold on chip A are released
 *   there (the song takes the chip), and every register is written to chip B (live_rewrite),
 *   so the keys still down sound again on it;
 * - back to chip A, once chip A has the keys' setup again and no key is held, so nothing is
 *   cut; until then the keys stay on chip B, which sounds the same.
 * Each move tells the device (keys_route.moved): chip B in use while the keys play it, the
 * LIVE view's tap on the keys' chip, the register ring's keys chip first (reg_ring_set_keys).
 *
 * Portable C with no hardware access. Main context. */
#ifndef SIDOU_KEYS_ROUTE_H
#define SIDOU_KEYS_ROUTE_H

#include <stdint.h>

#include "chip_ctl.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    live* l;
    live_sink_fn sink;       /* each chip's writes: reg_ring_sink on the device */
    live_prepare_fn prepare; /* each chip's preparations: reg_ring_prepare_step */
    void* out[2];            /* their contexts for chip A and chip B */
    chip_ctl ctl[2];
    uint8_t song;  /* 1 while a song plays on chip A */
    uint8_t stale; /* 1 while chip A, the keys' under KEYS A, holds a song's setup */
    uint8_t under; /* 1 while the keys play under a song (on chip A) */
    uint8_t taken; /* the voices the keys held at the last write under a song */
    /* During a song: the setup chip A should have (the song's CHPA, keys_route_song_setup, or
     * an edit on the CHIP view's A tab, keys_route_edit_song_setup); chip A follows it. */
    chip_setup song_want;
    uint8_t song_seen; /* song as the last tick saw it (a song's start sets song_want) */
    void (*moved)(void* ctx, uint32_t chip); /* NULL: nobody to tell */
    void* moved_ctx;
} keys_route;

/* The keys on chip A, chip B not made (its first reset follows from the first ticks). Then live_init(l, keys_route_sink, kr, model),
 * live_set_preparer(l, keys_route_prepare, kr) and keys_route_start. */
void keys_route_init(keys_route* kr, live* l, live_sink_fn sink, live_prepare_fn prepare,
                     void* out_a, void* out_b);
/* After live_init: sets live's two_chips (for the views); from now on live writes straight to the keys' chip's sink (keys_route
 * points live's sink at it at every move, and at keys_route_sink only while a song keeps chip
 * A under KEYS A), so the instrument's writes cost what they did with one chip. */
void keys_route_start(keys_route* kr);
/* live's sink while a song keeps chip A under KEYS A (a live_sink_fn, ctx a keys_route*): the
 * song's setup, filter and volume writes held back, the rest to chip A. */
void keys_route_sink(void* ctx, uint8_t reg, uint8_t value);
/* live's preparer (a live_prepare_fn): the keys' chip's. */
int keys_route_prepare(void* ctx, uint8_t profile);
/* Once a main-loop pass, after its live ticks: the chips' setups and the moves (above). */
void keys_route_tick(keys_route* kr);
/* A song plays on chip A (1) or not (0): M2c's transport; until then a test hook. */
void keys_route_set_song(keys_route* kr, int playing);
/* The voices of chip A the keys hold while a song plays with the keys on chip A (bit v: voice
 * v; under mono sync voice 3 with voice 1), for the song player to leave alone; 0 otherwise. */
uint8_t keys_route_voices_taken(const keys_route* kr);
/* 1 once a song may start on chip A: the keys play chip B, or KEYS is A. */
int keys_route_ready_for_song(const keys_route* kr);
/* The setup the song gives chip A (its CHPA), as chip A has it from now on (and wants). */
void keys_route_song_setup(keys_route* kr, const chip_setup* setup);
/* An edit of the song's setup (the CHIP view's A tab during a song): chip A is brought to it
 * (prepared, a slice a tick, as the other chip follows the keys'); M2c saves it in the song's
 * CHPA (song_want). */
void keys_route_edit_song_setup(keys_route* kr, const chip_setup* setup);

#ifdef __cplusplus
}
#endif

#endif
