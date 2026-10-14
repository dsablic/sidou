// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "panel.h"

void panel_init(panel* p) {
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        p->key_note[k] = LIVE_NO_NOTE;
    }
    p->master_raw = -1;
}

static void key_up(panel* p, live* l, uint32_t k) {
    live_note_off(l, p->key_note[k]);
    p->key_note[k] = LIVE_NO_NOTE;
}

void panel_note_keys(panel* p, live* l, uint32_t down, uint32_t pressed) {
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        uint32_t bit = 1u << k;
        if (p->key_note[k] != LIVE_NO_NOTE && (pressed & bit)) {
            key_up(p, l, k); /* let go and pressed again since the last call */
        }
        if (p->key_note[k] == LIVE_NO_NOTE && ((down | pressed) & bit)) {
            uint8_t n = live_key_note(l, (uint8_t)(PANEL_BASE_NOTE + k));
            if (n != LIVE_NO_NOTE) {
                live_note_on(l, n, PANEL_VELOCITY);
                p->key_note[k] = n;
            }
        }
        if (p->key_note[k] != LIVE_NO_NOTE && !(down & bit)) {
            key_up(p, l, k);
        }
    }
}

void panel_buttons(panel* p, live* l, uint32_t pressed) {
    (void)p;
    if (pressed & (1u << PANEL_BTN_OCT_DOWN)) {
        live_button(l, LIVE_BUTTON_OCT_DOWN);
    }
    if (pressed & (1u << PANEL_BTN_OCT_UP)) {
        live_button(l, LIVE_BUTTON_OCT_UP);
    }
    if (pressed & (1u << PANEL_BTN_FX)) {
        live_button(l, LIVE_BUTTON_FX);
    }
}

void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps) {
    (void)p;
    if (steps != 0 && enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4) {
        live_knob(l, (live_knob_id)(enc - PANEL_ENC_K1), steps);
    }
}

void panel_master(panel* p, live* l, int32_t raw) {
    if (raw < 0) {
        return;
    }
    if (raw > (int32_t)LIVE_MASTER_MAX) {
        raw = LIVE_MASTER_MAX;
    }
    int32_t d = raw - p->master_raw;
    /* the ends always land, so the volume can reach 0 and 15 */
    if (p->master_raw < 0 || d >= PANEL_MASTER_HYSTERESIS || d <= -PANEL_MASTER_HYSTERESIS ||
        ((raw == 0 || raw == (int32_t)LIVE_MASTER_MAX) && d != 0)) {
        p->master_raw = (int16_t)raw;
        live_set_master(l, (uint16_t)raw);
    }
}

panel_leds panel_get_leds(const panel* p, const live* l) {
    panel_leds o = {0, 0};
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        if (p->key_note[k] != LIVE_NO_NOTE) {
            o.notes |= 1u << k;
        }
    }
    if (l->octave < 0) {
        o.buttons |= 1u << PANEL_BTN_OCT_DOWN;
    }
    if (l->octave > 0) {
        o.buttons |= 1u << PANEL_BTN_OCT_UP;
    }
    if (l->model == SID_MODEL_8580) {
        o.buttons |= 1u << PANEL_BTN_FX;
    }
    return o;
}
