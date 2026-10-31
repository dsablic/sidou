// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_USB_AUDIO_H
#define SIDOU_FM1_USB_AUDIO_H

#include <stdint.h>

#define USB_AUDIO_FRAMES 128u
#define USB_AUDIO_CHUNK 32u
#define USB_AUDIO_CHUNKS (USB_AUDIO_FRAMES / USB_AUDIO_CHUNK)

typedef struct {
    int32_t frames[2u * USB_AUDIO_FRAMES];
    uint8_t want;
    uint8_t left;
    uint32_t blocks;
    uint32_t skipped;
} usb_audio_block;

void usb_audio_take(const int32_t* half);
void usb_audio_skip(void);
void usb_audio_tick(void);
void usb_audio_isr(void);

#endif
