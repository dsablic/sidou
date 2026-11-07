// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_CHARACTER_COLD_H
#define SIDOU_SID_CHARACTER_COLD_H

#include <stdint.h>

#include "sid_character.h"

float sid_character_response(uint8_t v);
int sid_character_dac_custom_for(sid_model model, const sid_character* c);

#endif
