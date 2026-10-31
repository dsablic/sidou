// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_AUDIO_H
#define SIDOU_FM1_AUDIO_H

#include <stdint.h>

#include "player/song_ctl.h"
#include "psid/psid_ctl.h"
#include "scope.h"
#include "sid/sid.h"

#define AUDIO_SAMPLE_RATE 44118u
#define AUDIO_HALF_FRAMES 128u
#define AUDIO_HALF_WORDS (2u * AUDIO_HALF_FRAMES)

void audio_init(void);
scope* audio_scope(void);
gt2_song* audio_song_boot(void);
gt2_song* audio_song_remake(void);
song_ctl* audio_song(void);
enum { AUDIO_SONG = 0, AUDIO_PSID = 1, AUDIO_NONE = 2 };
void audio_player(uint32_t player);
uint32_t audio_player_now(void);
psid_ctl* audio_psid_init(void);
psid_ctl* audio_psid(void);
psid_header* audio_psid_header(void);
psid_lift* audio_lift(void);
int audio_psid_poll(void);
sid* audio_chip(uint32_t chip);
void audio_song_anchor(uint32_t* ticks, uint32_t* samples);
void audio_song_poll(void);
int audio_work_slice(void);
void* audio_sink_ctx(void);
void* audio_sink_ctx_b(void);
void audio_keys(void* ctx, uint32_t chip);
#define AUDIO_B_KEYS 1u
#define AUDIO_B_UI 2u
#define AUDIO_B_TUNE 4u
void audio_chip_b_use(uint32_t user, int on);
void audio_set_master(uint32_t master);
void audio_set_spread(uint32_t spread);
void audio_tap_chip(uint32_t chip);
void audio_commit(void);
uint32_t audio_queued(void);
void audio_start(void);
void audio_irqs_on(void);
void audio_halted(void);
int audio_running(void);
extern volatile uint32_t audio_block_end;
void audio_hold(int on);
int audio_held(void);
#define AUDIO_QUIET_PEAK 128
void audio_peak_scan(int on);
uint32_t audio_quiet_blocks(void);
int audio_pass_sliced(void);
void audio_fill(int32_t* out);

#endif
