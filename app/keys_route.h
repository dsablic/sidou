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
 * through the song's setup). "A song playing" is keys_route_set_song: platform/fm1/main.c's
 * song transport sets it at PLAY and clears it once a stop has reached the interrupt; or
 * keys_route_set_player (M2e's MIDI modes that give chip A to the player, app/midi_route.h),
 * which counts as one.
 *
 * For M2c's song control (song_ctl), the contract:
 * - the transport starts a song only once keys_route_ready_for_song: the keys have left chip
 *   A (KEYS AUTO, chip B ready) or KEYS is A. Until then (a profile step just before PLAY, a
 *   song autostarting at boot) the keys may still be on chip A, filtered as above;
 * - the setup chip A is to have (the song's CHPA, at its start and at every change) goes to
 *   keys_route_song_wants, the one writer of song_want: keys_route brings chip A to it in the
 *   background (prepared, a slice a tick). The CHIP view's A tab uses keys_route_song_edit
 *   so the platform can keep its unlocked base and apply character locks before wants; what
 *   chip A has (ctl[0]) stays true. M2c saves the unlocked base into the song's CHPA.
 *   keys_route_song_setup is only for a setup chip A
 *   already holds (it records it, and wants it). After the song the keys are taken back to
 *   their own setup (STATUS shows chip A's meanwhile);
 * - a profile change of chip A during a song is a chip reset (prepared, then swapped in): the
 *   reset chip has none of the song's registers, so M2c Task 6's register shadow rewrites them
 *   after the swap;
 * - a voice the keys take goes to `taking` before the first of its registers goes to chip
 *   A's sink, so the song can leave it alone from the block those registers reach (song_ctl's
 *   take, which the interrupt reads at each block's start). The release tells nothing: the
 *   main loop gives a voice back once keys_route_voices_taken has let it go and the key's
 *   gate-off is committed;
 * - on the falling edge of a voice's bit in keys_route_voices_taken it rewrites that voice's
 *   registers (the keys' writes to it stop then);
 * - a pass in which the song control writes chip A (28 entries at most) and keys_route moves
 *   or takes chip A back (30) and chip B follows (4) is within REG_RING_WORST_PASS. Under KEYS
 *   A both chips follow in one pass (chip A the song's setup, chip B the keys'): each follow
 *   makes up to 8 sink writes (the 4 character settings, which the ring's sink keeps and does
 *   not queue; the profile, the reset, the gate reset and the character tick, queued), so up
 *   to 4 entries each, and chip A gets a character tick every song tick. M2c Task 6 measures
 *   that shared pass.
 *
 * The keys' setup travels with the keys. live's model, profile, character and envelope mode
 * are the keys' setup; the chip the keys play gets it from live, and the other chip follows
 * it in the background (chip_ctl), a slice a tick after the keys' own chip, so chip B holds
 * the keys' setup whenever the keys move to it. Chip A, while a song plays on it, is the
 * song's: it follows song_want (keys_route_song_wants); when the song stops it follows the
 * keys' setup again.
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

#define KEYS_ROUTE_EDIT_CONTROLS 0x0fu
#define KEYS_ROUTE_EDIT_PROFILE 0x10u
#define KEYS_ROUTE_EDIT_ENVELOPE 0x20u
#define KEYS_ROUTE_EDIT_EXPERT_FIRST 6u
enum {
    KEYS_ROUTE_EDIT_EXPERT = ((1u << SID_EXPERT_COUNT) - 1u) << KEYS_ROUTE_EDIT_EXPERT_FIRST,
    KEYS_ROUTE_EDIT_ALL = 0x3fu | KEYS_ROUTE_EDIT_EXPERT
};

typedef struct {
    live* l;
    live_sink_fn sink;       /* each chip's writes: reg_ring_sink on the device */
    live_prepare_fn prepare; /* each chip's preparations: reg_ring_prepare_step */
    void* out[2];            /* their contexts for chip A and chip B */
    chip_ctl ctl[2];
    uint8_t song;  /* 1 while chip A is the player's: a song plays on it, or a MIDI mode
                    * gives it to the player (song_on | player) */
    uint8_t song_on; /* keys_route_set_song's */
    uint8_t player;  /* keys_route_set_player's */
    uint8_t tune;    /* keys_route_set_tune's: the player is a .sid tune (KEYS A acts as AUTO) */
    uint8_t silent;  /* keys_route_set_silent's: chip B is the tune's too, the keys are silent */
    uint8_t stale; /* 1 while chip A, the keys' under KEYS A, holds a song's setup */
    uint8_t under; /* 1 while the keys play under a song (on chip A) */
    uint8_t taken; /* the voices the keys held at the last write under a song */
    /* During a song: the setup chip A should have (the song's CHPA or an edit on the CHIP
     * view's A tab, after its edit callback); chip A follows it. */
    chip_setup song_want;
    /* While silent: the setup chip B should have (a 2SID tune's second chip); it follows it. */
    chip_setup tune_b_want;
    uint8_t song_seen; /* song as the last tick saw it (a song's start sets song_want) */
    uint8_t song_user; /* USER slot n + 1 that supplied song_want, or 0 */
    void (*moved)(void* ctx, uint32_t chip); /* NULL: nobody to tell */
    void* moved_ctx;
    /* Voices the keys take under a song (bit v), before their first write: NULL, nobody. */
    void (*taking)(void* ctx, uint32_t voices);
    void* taking_ctx;
    int (*song_edit)(void* ctx, const chip_setup* setup, uint32_t edit_mask);
    void* song_edit_ctx;
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
/* Chip A is the player's (1) or not (0) whether a song plays or not: MIDI's MULTITIMBRAL and
 * WHOLE-CHIP modes (app/midi_route.h). Treated as a song playing, so the keys move to chip B
 * (KEYS AUTO) and chip A follows song_want. */
void keys_route_set_player(keys_route* kr, int owns);
/* A .sid tune is the player (M3; docs/plans/2026-10-19-m3-sid-files.md, "The keys, chip B and
 * the rest of the firmware while a tune plays"), on top of keys_route_set_song: KEYS A acts as
 * AUTO (no voice can be taken from a tune), so the keys go to chip B and chip A follows
 * song_want, the tune's setup. */
void keys_route_set_tune(keys_route* kr, int on);
/* A 2SID tune takes chip B as well (on 1): the keys are silent (live's writes go nowhere, its
 * preparations are not made) and stay on the chip they play, chip A follows song_want and chip
 * B tune_b_want. Off (the tune stopped): the keys' own chip goes back to the keys' setup and
 * their registers (as after a song under KEYS A), the other follows it or the song. */
void keys_route_set_silent(keys_route* kr, int on);
/* The chip (0 A, 1 B) is reset to the setup it follows at its next follow, even if it has it
 * already: a tune starts on a chip as from power-on. */
void keys_route_fresh(keys_route* kr, uint32_t chip);
/* The voices of chip A the keys hold while a song plays with the keys on chip A (bit v: voice
 * v; under mono sync voice 3 with voice 1), for the song player to leave alone; 0 otherwise. */
uint8_t keys_route_voices_taken(const keys_route* kr);
/* 1 once a song may start on chip A: the keys play chip B, or KEYS is A (not during a tune), or
 * the keys are silent. */
int keys_route_ready_for_song(const keys_route* kr);
/* Runtime setup chip A is to have during a song: the one writer of song_want. Chip A is
 * brought to it in the background (prepared, a slice a tick); the latest call wins. Explicit
 * edits use keys_route_song_edit so runtime locks do not replace the saved song base. */
void keys_route_song_wants(keys_route* kr, const chip_setup* setup);
/* Explicit CHIP A edit, distinct from runtime locks: the callback owns acceptance and the
 * unlocked song base; without one, goes straight to keys_route_song_wants. Returns 1 if
 * accepted, 0 if refused, with the setup and USER identity unchanged. edit_mask marks only
 * intended fields: bits 0..3 AGE through BLEED, bit 4 PROFILE, bit 5 ENVELOPE, then one bit
 * per expert. Macro edits release only their own experts in the unlocked base; runtime
 * locks may have released a different set, which must not be copied back into that base. */
int keys_route_song_edit(keys_route* kr, const chip_setup* setup, uint32_t edit_mask);
/* A setup chip A already holds (nothing is sent): recorded as what it has, and wanted. */
void keys_route_song_setup(keys_route* kr, const chip_setup* setup);

#ifdef __cplusplus
}
#endif

#endif
