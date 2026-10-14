// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_usb.h"

/* MIDI bytes per CIN (midi_usb.h). */
static const uint8_t cin_bytes[16] = {0, 0, 2, 3, 3, 1, 2, 3, 3, 3, 3, 3, 2, 2, 3, 1};

uint32_t midi_usb_bytes(uint32_t packet, uint8_t out[3]) {
    uint32_t cin = packet & 0x0fu;
    uint32_t status = (packet >> 8) & 0xffu;
    uint32_t n = cin_bytes[cin];
    if (cin >= 0x8u && cin <= 0xeu && status >> 4 != cin) {
        return 0; /* a channel message whose status is not the one its CIN announces */
    }
    for (uint32_t i = 0; i < n; i++) {
        out[i] = (uint8_t)(packet >> (8u * (i + 1u)));
    }
    return n;
}
