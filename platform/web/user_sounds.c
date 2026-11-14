// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "user_sounds.h"

#include <string.h>

#include "user_sound.h"

#define RECORD_USED 1u
#define NAME_FIRST 0x20u
#define NAME_LAST 0x5fu

static int stride_of(const uint8_t* file, uint32_t length, uint32_t* stride) {
    if (length < USER_SOUND_HEADER || file[0] < USER_SOUND_VERSION ||
        file[1] < LAB_SOUND_BYTES_1_0 || file[2] < USER_SOUNDS) {
        return 0;
    }
    *stride = 1u + LAB_SOUND_NAME + file[1];
    return length >= USER_SOUND_HEADER + USER_SOUNDS * *stride;
}

static int name_ok(const uint8_t* name) {
    for (uint32_t i = 0; i < LAB_SOUND_NAME && name[i]; i++) {
        if (name[i] < NAME_FIRST || name[i] > NAME_LAST) {
            return 0;
        }
    }
    return 1;
}

int user_sounds_file_read(const uint8_t* file, uint32_t length, uint32_t slot,
                          char name[LAB_SOUND_NAME + 1u], uint8_t sound[LAB_SOUND_BYTES]) {
    uint32_t stride;
    uint8_t record[USER_SOUND_STRIDE];
    if (slot >= USER_SOUNDS || !stride_of(file, length, &stride)) {
        return 0;
    }
    memset(record, 0, sizeof record);
    memcpy(record, file + USER_SOUND_HEADER + slot * stride,
           stride < sizeof record ? stride : sizeof record);
    if (record[0] != RECORD_USED || !name_ok(&record[1]) ||
        !lab_sound_valid(&record[1u + LAB_SOUND_NAME])) {
        return 0;
    }
    memcpy(name, &record[1], LAB_SOUND_NAME);
    name[LAB_SOUND_NAME] = 0;
    memcpy(sound, &record[1u + LAB_SOUND_NAME], LAB_SOUND_BYTES);
    return 1;
}
