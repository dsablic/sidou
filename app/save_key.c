// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "save_key.h"

void save_key_init(save_key* k) {
    k->down = 0;
    k->spoiled = 0;
}

save_key_action save_key_buttons(save_key* k, uint32_t pressed, uint32_t held) {
    uint32_t bit = 1u << SAVE_KEY_BUTTON;
    if (pressed & bit) {
        k->down = bit;
        k->spoiled = 0;
    }
    if (k->down && (pressed & ~bit)) {
        k->spoiled = 1; /* another button while SAVE is down */
    }
    if (k->down && !(held & bit)) {
        int clean = !k->spoiled;
        k->down = 0;
        k->spoiled = 0;
        return clean ? SAVE_KEY_SAVE : SAVE_KEY_NONE;
    }
    return SAVE_KEY_NONE;
}

void save_key_spoil(save_key* k) {
    if (k->down) {
        k->spoiled = 1;
    }
}

int save_key_held(const save_key* k) {
    return k->down != 0;
}
