// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SAVE_KEY_H
#define SIDOU_SAVE_KEY_H

#include <stdint.h>

#include "panel.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SAVE_KEY_BUTTON PANEL_BTN_SAVE

typedef enum {
    SAVE_KEY_NONE = 0,
    SAVE_KEY_SAVE = 1
} save_key_action;

typedef struct {
    uint32_t down;
    uint8_t spoiled;
} save_key;

void save_key_init(save_key* k);
save_key_action save_key_buttons(save_key* k, uint32_t pressed, uint32_t held);
void save_key_spoil(save_key* k);
int save_key_held(const save_key* k);

#ifdef __cplusplus
}
#endif

#endif
