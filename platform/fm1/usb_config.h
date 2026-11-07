// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_USB_CONFIG_H
#define SIDOU_FM1_USB_CONFIG_H

#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
#define FELUCCA_OTA 1
#define FELUCCA_CDC 0
#define FELUCCA_UAC 1
#define HALF_FRAMES AUDIO_HALF_FRAMES
#define FELUCCA_UAC_AS_FIRST 1
#define FELUCCA_UAC_48K 0
#define FELUCCA_USB_MANUFACTURER "SIDOU contributors"
#define FELUCCA_USB_PRODUCT "SIDOU FM-1"
#endif
