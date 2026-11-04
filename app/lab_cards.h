// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LAB_CARDS_H
#define SIDOU_LAB_CARDS_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    LAB_CARD_FENV_DEPTH = 0,
    LAB_CARD_FENV_ATTACK,
    LAB_CARD_FENV_DECAY,
    LAB_CARD_FENV_SUSTAIN,
    LAB_CARD_FENV_RELEASE,
    LAB_CARD_LAYERS,
    LAB_CARD_LAYER_EDIT,
    LAB_CARD_LAYER_TRANSPOSE,
    LAB_CARD_LAYER_DETUNE,
    LAB_CARD_LAYER_FILTER,
    LAB_CARD_TABLE_END,
    LAB_CARD_TABLE_STEP,
    LAB_CARD_TABLE_WAVE,
    LAB_CARD_TABLE_NOTE,
    LAB_CARD_TABLE_FRAMES,
    LAB_CARD_LFO_SHAPE,
    LAB_CARD_LFO_EDIT,
    LAB_CARD_LFO_DEST,
    LAB_CARD_LFO_RATE,
    LAB_CARD_LFO_DEPTH,
    LAB_CARDS
} lab_card;

uint32_t lab_card_get(const live* l, uint32_t card);
void lab_card_set(live* l, uint32_t card, uint32_t value);
uint32_t lab_card_max(uint32_t card);

#ifdef __cplusplus
}
#endif

#endif
