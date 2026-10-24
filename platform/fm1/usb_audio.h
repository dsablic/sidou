// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* USB audio out: SIDOU's output, as the DAC gets it, to a computer over the cable. Felucca's USB
 * stack (src/usb.c, built into midi.c with FELUCCA_UAC) presents a class-compliant UAC1 audio
 * input, 16-bit stereo at 44.1 or 48 kHz (the host's choice), on EP4 beside USB-MIDI. Its audio
 * input is a ring that a producer fills (uac_render_start, then uac_tap a block at a time; at 48
 * kHz uac_tap resamples, from a 7.5 KiB table) and the TIMER5 interrupt empties, one isochronous
 * packet per USB frame (uac_service). usb_audio.c, which midi.c includes after usb.c, drives it.
 *
 * Felucca calls the producer from its audio interrupt. SIDOU's audio interrupt runs from SRAM and
 * reads nothing from flash (app.ld, emu-gate tests/ram_audio.rs), and RAMTEXT has no room for the
 * producer nor RAM for its table. So the audio interrupt does nothing for USB audio: the TIMER5
 * interrupt, from flash, takes each block from the I2S half buffer the render has just written
 * (main.c: until the DMA moves on, the free half is that block, and it is not written again for a
 * block's time) and runs Felucca's producer on it: uac_render_start, then uac_tap on a quarter of
 * the block a tick (32 frames, Felucca's own block size), so no tick is long.
 *
 * TIMER5 (main.c), every tick, from flash:
 * - usb_audio_take(half) at the first tick after a render (a new block in the half);
 * - usb_audio_tick(): the producer's next step;
 * - usb_audio_isr() at the USB poll's tick while a block renders: usb_poll waits for the render's
 *   end, and a render can outlast a USB frame (Felucca's fm1_timer5_irq does the same). It
 *   touches EP4 and the ring's reading side only. */
#ifndef SIDOU_FM1_USB_AUDIO_H
#define SIDOU_FM1_USB_AUDIO_H

#include <stdint.h>

/* Frames in a block (audio.h AUDIO_HALF_FRAMES) and in one of TIMER5's taps. */
#define USB_AUDIO_FRAMES 128u
#define USB_AUDIO_CHUNK 32u
#define USB_AUDIO_CHUNKS (USB_AUDIO_FRAMES / USB_AUDIO_CHUNK)

/* The block being tapped: want while a host records and none is; left, the taps to go. */
typedef struct {
    int32_t frames[2u * USB_AUDIO_FRAMES]; /* L R, int16's range: what uac_tap takes */
    uint8_t want;
    uint8_t left;
    uint32_t blocks;  /* blocks taken */
    uint32_t skipped; /* blocks a recording host did not get (usb_audio_skip, or still tapping) */
} usb_audio_block;

/* TIMER5, the first tick after a render: the block in the I2S half buffer it wrote (2 x
 * USB_AUDIO_FRAMES words, L R, 24-bit: int16 << 8), taken while a host records. */
void usb_audio_take(const int32_t* half);
/* TIMER5: a block went by that could not be read (the DMA had moved on): counted, if a host
 * records. */
void usb_audio_skip(void);
/* TIMER5, every tick: the producer's next step. */
void usb_audio_tick(void);
/* TIMER5, nested in the render, at the USB poll's tick: the next packet, if the last has gone
 * (usb.c's uac_service). */
void usb_audio_isr(void);

#endif
