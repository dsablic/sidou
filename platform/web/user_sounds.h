// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_USER_SOUNDS_H
#define SIDOU_USER_SOUNDS_H

#include <stdint.h>

#include "lab_sound.h"

#ifdef __cplusplus
extern "C" {
#endif

int user_sounds_file_read(const uint8_t* file, uint32_t length, uint32_t slot,
                          char name[LAB_SOUND_NAME + 1u], uint8_t sound[LAB_SOUND_BYTES]);

#ifdef __cplusplus
}
#endif

#endif
