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
/* Makes the main loop's writes so far visible to the interrupt (once per pass). */
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
