// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SAVE's gesture (docs/reference/controls.md, "SAVE"): its press saves, and holding it while
 * a knob turns opens MIDI learn for that knob. save_key decides which from the panel's events,
 * with app/views.c reading its held bit directly for cycle savings while routing it:
 * - SAVE pressed and released with no knob turned and no other button pressed while it was
 *   down: a save (save_key_buttons returns SAVE_KEY_SAVE at the release; the views decide what
 *   it saves, app/views.h);
 * - a knob turned or another button pressed while SAVE is down spoils that save
 *   (save_key_spoil, or the press itself): the release saves nothing.
 * SAVE's press never reaches the views or the panel.
 *
 * Portable C with no hardware access and no allocation. */
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
    SAVE_KEY_SAVE = 1 /* SAVE released after a clean press: save */
} save_key_action;

typedef struct {
    uint32_t down;   /* SAVE's button bit while held (since its press) */
    uint8_t spoiled; /* something else happened while it was: no save at its release */
} save_key;

void save_key_init(save_key* k);
/* One input frame's press edges and held buttons (as panel_buttons takes them): SAVE's press,
 * the other presses while it is down (they spoil its save), and its release. SAVE_KEY_SAVE at
 * a clean release. */
save_key_action save_key_buttons(save_key* k, uint32_t pressed, uint32_t held);
/* A knob turned while SAVE is down: the release saves nothing. */
void save_key_spoil(save_key* k);
/* 1 while SAVE is held. */
int save_key_held(const save_key* k);

#ifdef __cplusplus
}
#endif

#endif
