// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* USB-MIDI 1.0 event packets as the MIDI 1.0 byte stream that midi_in parses.
 *
 * The packet is the 32-bit word as it lies in the endpoint buffer, read little-endian:
 * bits 0..3 the Code Index Number (CIN), bits 4..7 the cable, bits 8..31 MIDI_0..MIDI_2.
 * The CIN gives the number of MIDI bytes (USB Device Class Definition for MIDI Devices 1.0,
 * table 4-1):
 *
 *   0x0, 0x1  reserved, cable events            0 (ignored)
 *   0x2       two-byte system common            2
 *   0x3       three-byte system common          3
 *   0x4       SysEx starts or continues         3
 *   0x5       single-byte system common, or
 *             SysEx ends with one byte          1
 *   0x6       SysEx ends with two bytes         2
 *   0x7       SysEx ends with three bytes       3
 *   0x8..0xB  note-off, note-on, poly pressure,
 *             control change                    3
 *   0xC, 0xD  program change, channel pressure  2
 *   0xE       pitch bend                        3
 *   0xF       single byte                       1
 *
 * A channel message (CIN 0x8..0xE) whose status does not match its CIN is malformed and
 * gives no bytes. Everything else is passed on as it is: midi_in copes with stray bytes, as
 * it must for the TRS jack. The cable number is ignored: SIDOU has one MIDI IN jack. */
#ifndef SIDOU_MIDI_USB_H
#define SIDOU_MIDI_USB_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Writes the packet's MIDI bytes to out (room for 3) and returns how many, 0..3. */
uint32_t midi_usb_bytes(uint32_t packet, uint8_t out[3]);

#ifdef __cplusplus
}
#endif

#endif
