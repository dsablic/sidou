// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_EDIT_H
#define SIDOU_LAB_EDIT_H

#include <stdint.h>

#include "lab_sound.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_EDIT_STEPS 16u
#define LAB_EDIT_ENTRIES 52u
#define LAB_EDIT_MODE LAB_SOUND_BYTES
#define LAB_EDIT_BYTES (LAB_SOUND_BYTES + 1u)
#define LAB_EDIT_FIRST 0x80u
#define LAB_SILENT_MS 500u
#define LAB_SILENT_DB (-40)

_Static_assert(LAB_EDIT_BYTES <= LAB_EDIT_ENTRIES && LAB_EDIT_BYTES < LAB_EDIT_FIRST,
               "any one step fits the ring, and a place fits beside its mark");

typedef struct {
    uint8_t at[LAB_EDIT_ENTRIES];
    uint8_t old[LAB_EDIT_ENTRIES];
    uint8_t first;
    uint8_t n;
    uint8_t steps;
} lab_edit;

void lab_edit_clear(lab_edit* e);
void lab_edit_get(const live* l, uint8_t out[LAB_EDIT_BYTES]);
uint32_t lab_edit_push(lab_edit* e, const uint8_t before[LAB_EDIT_BYTES],
                       const uint8_t after[LAB_EDIT_BYTES], int merge);
#define LAB_AUDIBLE_DB (-38)
uint32_t lab_audible(const uint8_t s[LAB_SOUND_BYTES], uint32_t model);
uint32_t lab_edit_hash(const uint8_t s[LAB_SOUND_BYTES]);
uint32_t lab_edit_undo(lab_edit* e, live* l);

#ifdef __cplusplus
}
#endif

#endif
