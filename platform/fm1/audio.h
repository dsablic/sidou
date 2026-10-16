// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The audio render, without hardware access: main.c's ALNK0 interrupt calls audio_fill for
 * each free half of the I2S DMA buffer: the chip, then the effect slots, in stereo.
 *
 * The main loop changes the chip only through audio_sink (app/reg_ring.h): its writes queue
 * up in a ring that audio_fill drains before each block. */
#ifndef SIDOU_FM1_AUDIO_H
#define SIDOU_FM1_AUDIO_H

#include <stdint.h>

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
/* The context for reg_ring_sink (a reg_ring_out*), for live. Before audio_start, a full ring
 * is drained at once; after it, the producer sleeps until the interrupt has made room. */
void* audio_sink_ctx(void);
/* The context for reg_ring_sink bound to chip B (a reg_ring_out*). */
void* audio_sink_ctx_b(void);
/* Chip B's first reset, at boot only (after live_init): its character (the profile, a
 * SID_CHIP_*, the controls at 0) now; then audio_chip_b_tick, once a live tick, prepares the
 * chip a slice a pass (reg_ring_prepare_step, about 70 ms, during the boot's title card) and
 * sends the reset to model and the gate reset once it is whole. Not a reset made at once: 0.5 M
 * instructions before the main loop would move the boot's timeline (with it LCD goldens and
 * budgets that depend on where the audio blocks fall against the frames). Until the reset
 * has reached the chip its sid is zeroed and audio_fill never renders it. */
void audio_chip_b_boot(sid_model model, uint8_t profile, uint8_t gate_reset);
void audio_chip_b_tick(void);
/* Chip B in use (1) or not (0): from the next block audio_fill renders it beside chip A and
 * mixes it in before the slots (app/chip_mix.h), fading it in or out over that block. Out of
 * use it is not rendered and the output is chip A's bit for bit. */
void audio_chip_b_use(int on);
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
/* AUDIO_HALF_WORDS words: the SID's output through the effect slots (core/fx/fx_slots.h),
 * interleaved L and R, 24-bit left-justified ((int32)sample << 8); with every slot off, the
 * chip's output on both sides. Feeds the LIVE view's tap while it is on (scope_render,
 * scope_peaks). Called from the ALNK0 interrupt. */
void audio_fill(int32_t* out);

#endif
