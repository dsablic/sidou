// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's first song, "SIDOU LOOP" (M2c Task 6): a GoatTracker 2 .sng in flash
 * (app/demo_song.c, generated from host/crates/sidou-tests/src/song_embed.rs). Boot reads it
 * into the song's region (the audio interrupt never reads flash data); PLAY plays and stops it
 * until M2d's transport arrives. */
#ifndef SIDOU_DEMO_SONG_H
#define SIDOU_DEMO_SONG_H

#include <stdint.h>

extern const uint8_t demo_song_sng[];
extern const uint32_t demo_song_sng_bytes;

#endif
