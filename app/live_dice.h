// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* DICE: the preset after NOISE PERC (live.h LIVE_PRESET_DICE), a new sound at every turn. */
#ifndef SIDOU_LIVE_DICE_H
#define SIDOU_LIVE_DICE_H

#include <stdint.h>

#include "live.h"

/* Loads DICE's base (SAW LEAD's values) and sets, through live_param_set, a sound from `seed`
 * (mixed with the roll's number, which it counts: live.dice_rolls): the waveform, ring on a
 * triangle, sync on a saw or pulse, the envelope, the cutoff (an even step of K1's table, the
 * middle of its range), the resonance, the filter mode, the pulse width on a pulse and now and
 * then a vibrato. Every voice goes through the filter. The effect slots are the instrument's, not
 * a preset's: a roll leaves them. */
void live_dice_roll(live* l, uint32_t seed);

#endif
