// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_USB_CONFIG_H
#define SIDOU_FM1_USB_CONFIG_H

#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
#define FELUCCA_OTA 1
#define FELUCCA_CDC 0
#define FELUCCA_UAC 1
#define HALF_FRAMES AUDIO_HALF_FRAMES
#define FELUCCA_USB_MANUFACTURER                                                                   \
    38, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'c', 0, 'o', 0, 'n', 0, 't', 0, 'r', 0, \
        'i', 0, 'b', 0, 'u', 0, 't', 0, 'o', 0, 'r', 0, 's', 0
#define FELUCCA_USB_PRODUCT 22, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'F', 0, 'M', 0, '-', 0, '1', 0
#endif
