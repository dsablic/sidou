// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PSID_TWEAK_H
#define SIDOU_PSID_TWEAK_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_TWEAK_CUT_MAX 1024
#define PSID_TWEAK_CUT_STEP 32
#define PSID_TWEAK_RES_MAX 15
#define PSID_TWEAK_VOICES 3u
#define PSID_TWEAK_TUNES_WAVE 0u

typedef struct {
    int16_t cut;
    int8_t res;
    uint8_t wave[PSID_TWEAK_VOICES];
} psid_tweak;

#ifdef __cplusplus
}
#endif

#endif
