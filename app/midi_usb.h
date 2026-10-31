// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_USB_H
#define SIDOU_MIDI_USB_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

uint32_t midi_usb_bytes(uint32_t packet, uint8_t out[3]);

#ifdef __cplusplus
}
#endif

#endif
