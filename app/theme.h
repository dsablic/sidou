// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screens' colour roles, defined once (docs/art/README.md "Theme"). A view never names a
 * colour of the 16 for a purpose of its own: it asks the theme for the role (`t->value`,
 * `t->knob[0]`) and builds its cell attributes with THEME_ATTR.
 *
 * HIGH CONTRAST (the default) is black with white values, grey labels and dark grey for what
 * is inactive, one accent per thing: voice 1 light red, voice 2 cyan, voice 3 light green;
 * K1 yellow, K2 cyan, K3 light green, K4 light red (LIVE, CHIP and the popup's bar alike).
 * C64 keeps the old look (light blue on blue) with the same accents, and the menu's, the
 * toast's and the popup's old reverse-video colours.
 *
 * The id is the settings' THEME byte (live.theme: app/settings.h, the menu's THEME).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_THEME_H
#define SIDOU_THEME_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define THEME_HIGH_CONTRAST 0u
#define THEME_C64 1u
#define THEME_COUNT 2u
/* A popup colour that takes the accent of the control (the knob's colour). */
#define THEME_ACCENT 0xFFu

typedef struct {
    const char* name;       /* the popup's: HI-CONTRAST, C64 */
    const char* menu_name;  /* the menu's value: HIGH, C64 */
    uint8_t bg;             /* the screen */
    uint8_t value;          /* values and what has the focus: white */
    uint8_t label;          /* names and labels: grey */
    uint8_t dim;            /* inactive things: dark grey */
    uint8_t hot;            /* the focused tab, the current row's number: yellow */
    uint8_t alert;          /* mute, a refusal, REC: light red */
    uint8_t cursor;         /* the current row's background */
    uint8_t voice[3];       /* voice 1, 2, 3 */
    uint8_t knob[4];        /* K1, K2, K3, K4 */
    /* The menu: its title bar, its items, a second colour for their values, the selected line. */
    uint8_t title_fg, title_bg;
    uint8_t menu_item, menu_value;
    uint8_t sel_fg, sel_bg;
    /* The toast. */
    uint8_t toast_fg, toast_bg;
    /* The popup: its box's background, and its frame, name and bar's fill (THEME_ACCENT: the
     * control's accent) over the bar's track. */
    uint8_t popup_bg;
    uint8_t popup_frame, popup_name, popup_bar, popup_track;
} theme;

/* The theme with that id; an unknown id is the default. */
const theme* theme_get(uint32_t id);

/* A cell attribute: the role's colour on the theme's background. */
#define THEME_ATTR(t, role) GRID_ATTR((t)->role, (t)->bg)

#ifdef __cplusplus
}
#endif

#endif
