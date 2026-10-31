// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_KIT_H
#define SIDOU_GT2_KIT_H

#include <stdint.h>

#include "gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

enum {
    GT2_KIT_KICK, GT2_KIT_SNARE, GT2_KIT_CLOSED_HAT, GT2_KIT_OPEN_HAT, GT2_KIT_CLAP,
    GT2_KIT_LOW_TOM, GT2_KIT_HIGH_TOM, GT2_KIT_RIM, GT2_KIT_DRUMS
};

typedef struct {
    uint8_t head[GT2_INS_NAME];
    char name[GT2_INSTR_NAME];
} gt2_kit_drum;
_Static_assert(sizeof(gt2_kit_drum) == GT2_INSTR_BYTES, "a kit drum is an instrument's bytes");
extern const gt2_kit_drum gt2_kit_instr[GT2_KIT_DRUMS];
#define GT2_KIT_WAVE_ROWS 45u
#define GT2_KIT_PULSE_ROWS 2u
extern const uint8_t gt2_kit_wave[2][GT2_KIT_WAVE_ROWS];
extern const uint8_t gt2_kit_pulse[2][GT2_KIT_PULSE_ROWS];

uint32_t gt2_kit_find(const gt2_song* s, uint32_t drum);

#ifdef __cplusplus
}
#endif

#endif
