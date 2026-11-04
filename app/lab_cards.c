// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_cards.h"

#include <stddef.h>

#include "lab_table.h"
#include "midi_map.h"

#define FENV_FIELDS (LIVE_FENV_RELEASE + 1u)
#define TABLE_CARD(card) ((card) - LAB_CARD_TABLE_STEP)

_Static_assert(LAB_CARD_FENV_RELEASE - LAB_CARD_FENV_DEPTH == LIVE_FENV_RELEASE - LIVE_FENV_DEPTH,
               "the filter envelope's cards are its fields in order");

uint32_t lab_card_max(uint32_t card) {
    const midi_map_entry* e = card < LAB_CARDS ? midi_map_for_param(MIDI_MAP_LAB_CARD(card)) : NULL;
    return e ? e->max : 0u;
}

static uint32_t table_end(const live* l) {
    return lab_table_on(l) ? 1u + l->table[LAB_SOUND_TABLE_LOOP - LAB_SOUND_TABLE] : 0u;
}

static uint32_t table_card(const live* l, uint32_t k) {
    char name[16], value[16];
    uint32_t fill, max;
    (void)lab_table_card(l, l->table_edit, k, name, value, &fill, &max);
    return fill;
}

uint32_t lab_card_get(const live* l, uint32_t card) {
    if (card < FENV_FIELDS) {
        return live_filter_env(l, LIVE_FENV_DEPTH + card);
    }
    switch (card) {
    case LAB_CARD_LAYERS: return l->layers;
    case LAB_CARD_LAYER_EDIT: return l->layer_edit;
    case LAB_CARD_LAYER_TRANSPOSE: return (uint32_t)(live_layer_transpose(l) + LIVE_TRANSPOSE_MAX);
    case LAB_CARD_LAYER_DETUNE: return (uint32_t)(live_layer_detune(l) + LIVE_DETUNE_CENTS);
    case LAB_CARD_LAYER_FILTER: return l->filter_voices >> l->layer_edit & 1u;
    case LAB_CARD_TABLE_END: return table_end(l);
    default: return card < LAB_CARDS ? table_card(l, TABLE_CARD(card)) : 0u;
    }
}

void lab_card_set(live* l, uint32_t card, uint32_t value) {
    if (card >= LAB_CARDS) {
        return;
    }
    uint32_t max = lab_card_max(card);
    value = value < max ? value : max;
    int32_t delta = (int32_t)value - (int32_t)lab_card_get(l, card);
    if (card < FENV_FIELDS) {
        live_filter_env_set(l, LIVE_FENV_DEPTH + card, value);
        return;
    }
    switch (card) {
    case LAB_CARD_LAYERS: (void)live_layers_set(l, value); break;
    case LAB_CARD_LAYER_EDIT: live_layer_edit_set(l, value); break;
    case LAB_CARD_LAYER_TRANSPOSE: live_layer_transpose_set(l, (int32_t)value - LIVE_TRANSPOSE_MAX); break;
    case LAB_CARD_LAYER_DETUNE: live_layer_detune_set(l, (int32_t)value - LIVE_DETUNE_CENTS); break;
    case LAB_CARD_LAYER_FILTER: {
        uint32_t bit = 1u << l->layer_edit;
        live_param_set(l, LIVE_PARAM_FILTER_VOICES, value ? l->filter_voices | bit : l->filter_voices & ~bit);
        break;
    }
    case LAB_CARD_TABLE_END: lab_table_choice_turn(l, delta); break;
    default: {
        uint32_t k = TABLE_CARD(card);
        if (k != 0u && k != 3u && l->table_edit >= lab_table_steps(l)) {
            l->table_edit = (uint8_t)lab_table_turn(l, l->table_edit, k, 0);
            delta = (int32_t)value - (int32_t)lab_card_get(l, card);
        }
        l->table_edit = (uint8_t)lab_table_turn(l, l->table_edit, k, delta);
        break;
    }
    }
}
