// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "usb_audio.h"

_Static_assert(USB_AUDIO_FRAMES == HALF_FRAMES, "usb.c's ring holds a block of SIDOU's");
_Static_assert(USB_AUDIO_CHUNKS * USB_AUDIO_CHUNK == USB_AUDIO_FRAMES, "whole taps a block");

#ifndef USB_AUDIO_POOL
#define USB_AUDIO_POOL __attribute__((section(".pool")))
#endif
static usb_audio_block usb_audio_in USB_AUDIO_POOL __attribute__((used));

void usb_audio_take(const int32_t* half) {
    usb_audio_block* b = &usb_audio_in;
    if (!b->want) {
        b->skipped += b->left != 0u;
        return;
    }
    for (uint32_t i = 0; i < 2u * USB_AUDIO_FRAMES; i++) {
        b->frames[i] = half[i] >> 8;
    }
    b->blocks++;
    b->left = USB_AUDIO_CHUNKS;
    b->want = 0;
}

void usb_audio_skip(void) {
    usb_audio_in.skipped += usb_audio_in.want | (usb_audio_in.left != 0u);
}

void usb_audio_tick(void) {
    usb_audio_block* b = &usb_audio_in;
    uint32_t left = b->left;
    if (left) {
        if (left == USB_AUDIO_CHUNKS) {
            uac_render_start();
        }
        uac_tap(&b->frames[2u * USB_AUDIO_CHUNK * (USB_AUDIO_CHUNKS - left)], USB_AUDIO_CHUNK);
        b->left = (uint8_t)(left - 1u);
        if (left > 1u) {
            return;
        }
    }
    b->want = (uint8_t)(uac.alt && usb.config && uac.flowing);
}

void usb_audio_isr(void) {
    uac_service();
}
