// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LIVE_DICE_H
#define SIDOU_LIVE_DICE_H

#include <stdint.h>

#include "live.h"

void live_dice_roll(live* l, uint32_t seed);
void live_dice_shape(live* l, uint32_t s);

#endif
