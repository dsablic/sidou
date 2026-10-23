// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The audio render, without hardware access: main.c's ALNK0 interrupt calls audio_fill for
 * each free half of the I2S DMA buffer: the chip (split at the song's call, song_ctl.h),
 * then the effect slots, in stereo.
 *
 * The main loop changes the chip only through audio_sink (app/reg_ring.h): its writes queue
 * up in a ring that audio_fill drains before each block. */
#ifndef SIDOU_FM1_AUDIO_H
#define SIDOU_FM1_AUDIO_H

#include <stdint.h>

#include "player/song_ctl.h"
#include "scope.h"
#include "sid/sid.h"

/* The I2S frame rate: 24 MHz / 544 = 44117.6 Hz. */
#define AUDIO_SAMPLE_RATE 44118u
/* Frames per half buffer; each frame is two int32 words, L then R. */
#define AUDIO_HALF_FRAMES 128u
#define AUDIO_HALF_WORDS (2u * AUDIO_HALF_FRAMES)

/* Before interrupts are enabled. */
void audio_init(void);
/* The tap the LIVE view reads (app/scope.h); audio_fill feeds it while its `on` is set. */
scope* audio_scope(void);
/* Before audio_init (main.c runs it in the LCD's reset wait): the song's region formatted and
 * the song transport made; the empty song. */
gt2_song* audio_song_boot(void);
/* The song transport (core/player/song_ctl.h): its main-loop side, the song's region in POOL. */
song_ctl* audio_song(void);
/* The latest block's anchor for MIDI's clock (app/midi_sync.h, midi_sync_anchor): the tick
 * its render started at and its first sample on the song's count (song_ctl.samples). */
void audio_song_anchor(uint32_t* ticks, uint32_t* samples);
/* song_ctl_poll, with a slice unless the pass has run one of the spare chip's preparation. */
void audio_song_poll(void);
/* Claim a foreground slice; excludes chip preparation and the store's work this pass. */
int audio_work_slice(void);
/* The context for reg_ring_sink (a reg_ring_out*), for live. Before audio_start, a full ring
 * is drained at once; after it, the producer sleeps until the interrupt has made room. */
void* audio_sink_ctx(void);
/* The context for reg_ring_sink bound to chip B (a reg_ring_out*). */
void* audio_sink_ctx_b(void);
/* The keys moved to chip (0 A, 1 B; app/keys_route.h, keys_route.moved): its requests first
 * in the register ring (reg_ring_set_keys), chip B in use while the keys play it, and the
 * LIVE view's tap on it. */
void audio_keys(void* ctx, uint32_t chip);
/* Chip B's users, or'd: the keys while they play it, M2b Task 7's UI sounds. */
#define AUDIO_B_KEYS 1u
#define AUDIO_B_UI 2u
/* A user takes chip B (on 1) or lets it go (0). Chip B is in use while it has a user and,
 * after the last let go, until its three envelopes are at 0: from the next block audio_fill
 * renders it beside chip A and mixes it in before the slots (app/chip_mix.h), fading it in or
 * out over the block it goes in or out of use. Out of use it is not rendered and the output
 * is chip A's bit for bit. */
void audio_chip_b_use(uint32_t user, int on);
/* The MASTER pot's value, 0..LIVE_MASTER_MAX (live.master), as a digital gain on the final
 * output from the next block (app/master_gain.h): after the chips, chip B's mix and the effect
 * slots, so it scales the keys, a song on either chip and the UI sounds alike. Before the
 * interrupt runs it takes effect at once. Main.c passes live's value when it changes. */
void audio_set_master(uint32_t master);
/* SETTINGS' SPREAD, 0..100 % (app/chip_spread.h), from the main loop: the gains move over the
 * next block. */
void audio_set_spread(uint32_t spread);
/* The chip the LIVE view's tap reads (0 A, 1 B; the keys' chip), from the next block. While
 * chip B is out of use the tap reads chip A. */
void audio_tap_chip(uint32_t chip);
/* Makes the main loop's writes so far visible to the interrupt (once per pass), and ends the
 * pass for the spare chip's preparation (reg_ring_pass_end: a slice a pass). */
void audio_commit(void);
/* Register writes queued and not yet applied by the interrupt (reg_ring_used). */
uint32_t audio_queued(void);
/* Applies everything queued so far. Before the audio interrupt is enabled. */
void audio_start(void);
/* Right after interrupts are enabled: from then on only audio_fill drains, and a producer
 * waiting for room sleeps until the next interrupt. Between audio_start and this call
 * nothing may write to live (the ISR may already drain, and a wait would drain too). */
void audio_irqs_on(void);
/* After main.c has stopped the I2S DMA (fm1_audio_stop) for good: flash erases and writes
 * check audio_running first (midi.c), since the audio path reads flash (.rodata) and the DMA
 * would loop a stale buffer while interrupts are off. */
void audio_halted(void);
/* 1 between audio_irqs_on and audio_halted. */
int audio_running(void);
/* fm1_ticks() at the end of the last audio block: main.c's ALNK0 interrupt writes it as its
 * last act. A write window (platform/fm1/store_fm1.c) starts only shortly after it. */
extern volatile uint32_t audio_block_end;
/* A held flash operation (store_fm1.c): with on, audio_fill renders one more block faded out to
 * 0, then writes silence without rendering (the chips and their clocks stand still; the ring's
 * writes still apply) until on is 0, then renders again, the first block faded in. */
void audio_hold(int on);
/* 1 once both halves of the DMA buffer hold silence for the hold: an operation with
 * interrupts masked now loops silence. */
int audio_held(void);
/* The quiet detector's peak scan (store_fm1.c): on only while the store wants a held operation
 * and no song plays; off it costs nothing. While on, audio_quiet_blocks counts the blocks in
 * a row whose output stayed under AUDIO_QUIET_PEAK; off, it is 0. -48 dBFS, not the plan's
 * -60: before the tail gate (app/tail_gate.h) the 6581 left a floor of about -55 dBFS after
 * notes that never decayed (measured in fm1-emu, emu-gate store.rs), and a chip with a filter
 * bleed still does, so -60 would never come there. */
#define AUDIO_QUIET_PEAK 128
void audio_peak_scan(int on);
uint32_t audio_quiet_blocks(void);
/* Reads and clears whether a pass ran a preparation, character-table or song slice (of
 * either chip's writer): the store skips its work in such a pass. */
int audio_pass_sliced(void);
/* AUDIO_HALF_WORDS words: the SID's output through the effect slots (core/fx/fx_slots.h),
 * interleaved L and R, 24-bit left-justified ((int32)sample << 8); with every slot off, the
 * chip's output on both sides. Feeds the LIVE view's tap while it is on (scope_render,
 * scope_peaks). Called from the ALNK0 interrupt. */
void audio_fill(int32_t* out);

#endif
