// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* How SIDOU builds Felucca's src/usb.c: its build switches and the two string hooks
 * (platform/fm1/VENDORED.md). midi.c includes this before usb.c, and so does the host harness
 * (host/crates/sidou-pack/harness/uac_harness.c), so the descriptors it tests are the device's.
 * HALF_FRAMES is SIDOU's block (audio.h AUDIO_HALF_FRAMES); the includer defines AUDIO_HALF_FRAMES
 * first. */
#ifndef SIDOU_FM1_USB_CONFIG_H
#define SIDOU_FM1_USB_CONFIG_H

/* The ring's index stores are ordered by midi_ring.c itself; this orders usb.c's own rings. */
#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
/* The update protocol's SysEx frames and TX queue in usb.c (midi.c includes ota.c). usb.c also sets
 * usb.uboot_req on the stock UBOOT soft key F0 22 24 35 7D F7; nothing here reads it, and SIDOU
 * never sends it. */
#define FELUCCA_OTA 1
/* Felucca's build switches as its default firmware leaves them, but without the CDC console:
 * MIDI streaming on EP1 and the USB audio input on EP4 (FELUCCA_UAC: class-compliant UAC1, 16-bit
 * stereo at 44.1 or 48 kHz, FELUCCA_UAC_48K and FELUCCA_UAC_AS_FIRST at usb.c's defaults), which
 * midi.c's usb_audio.c hands SIDOU's output (platform/fm1/usb_audio.h). */
#define FELUCCA_CDC 0
#define FELUCCA_UAC 1
/* usb.c's ring takes a render of HALF_FRAMES (Felucca's core.h): SIDOU's block. */
#define HALF_FRAMES AUDIO_HALF_FRAMES
/* usb.c's two SIDOU hooks (platform/fm1/VENDORED.md). The device presents as "SIDOU FM-1" by
 * "SIDOU contributors" (string descriptors: bLength, 3, UTF-16LE). "FM-1" in the name is what
 * the installers look for (Felucca's fm1_install.py and fm1ota.js, the Firmware Hub), as
 * Melodee 1.4.5 found. */
#define FELUCCA_USB_MANUFACTURER                                                                   \
    38, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'c', 0, 'o', 0, 'n', 0, 't', 0, 'r', 0, \
        'i', 0, 'b', 0, 'u', 0, 't', 0, 'o', 0, 'r', 0, 's', 0
#define FELUCCA_USB_PRODUCT 22, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'F', 0, 'M', 0, '-', 0, '1', 0
#endif
