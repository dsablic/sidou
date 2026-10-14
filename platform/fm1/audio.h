// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The audio render, without hardware access: main.c's ALNK0 interrupt calls audio_fill for
 * each free half of the I2S DMA buffer.
 *
 * The main loop changes the chip only through audio_sink (app/reg_ring.h): its writes queue
 * up in a ring that audio_fill drains before each block. */
#ifndef SIDOU_FM1_AUDIO_H
#define SIDOU_FM1_AUDIO_H

#include <stdint.h>

/* The I2S frame rate: 24 MHz / 544 = 44117.6 Hz. */
#define AUDIO_SAMPLE_RATE 44118u
/* Frames per half buffer; each frame is two int32 words, L then R. */
#define AUDIO_HALF_FRAMES 128u
#define AUDIO_HALF_WORDS (2u * AUDIO_HALF_FRAMES)

/* Before interrupts are enabled. */
void audio_init(void);
/* The context for reg_ring_sink (a reg_ring_out*), for live. Before audio_start, a full ring
 * is drained at once; after it, the producer sleeps until the interrupt has made room. */
void* audio_sink_ctx(void);
/* Makes the main loop's writes so far visible to the interrupt (once per pass). */
void audio_commit(void);
/* Applies everything queued so far; from then on only audio_fill drains. Before the audio
 * interrupt is enabled. */
void audio_start(void);
/* AUDIO_HALF_WORDS words: the SID's mono output on both channels, 24-bit left-justified
 * ((int32)sample << 8). Called from the ALNK0 interrupt. */
void audio_fill(int32_t* out);

#endif
