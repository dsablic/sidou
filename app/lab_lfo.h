// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_LFO_H
#define SIDOU_LAB_LFO_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

const char* lab_lfo_dest_code(uint32_t dest);
int lab_lfo_card(const live* l, uint32_t k, char* name, char* value, uint32_t* fill, uint32_t* max);
void lab_lfo_choice(const live* l, char* out);
void lab_lfo_turn(live* l, uint32_t k, int32_t d);
void lab_lfo_choice_turn(live* l, int32_t d);
uint32_t lab_lfo_shape_of(const live* l, uint32_t n);
uint32_t lab_lfo_dest_of(const live* l, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
