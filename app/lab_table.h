// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_TABLE_H
#define SIDOU_LAB_TABLE_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_TABLE_NOTE_SPAN 24
#define LAB_TABLE_WAVE_CODES 13u

uint32_t lab_table_steps(const live* l);
uint32_t lab_table_on(const live* l);
uint32_t lab_table_layer(const live* l);
const char* lab_table_wave_code(uint8_t control, char out[4]);
void lab_table_note_text(uint8_t note, char out[5]);
int lab_table_card(const live* l, uint32_t step, uint32_t k, char* name, char* value,
                   uint32_t* fill, uint32_t* max);
void lab_table_choice(const live* l, char* out);
uint32_t lab_table_turn(live* l, uint32_t step, uint32_t k, int32_t d);
void lab_table_choice_turn(live* l, int32_t d);

#ifdef __cplusplus
}
#endif

#endif
