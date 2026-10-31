// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
#define THEME_ACCENT 0xFFu

typedef struct {
    const char* name;
    const char* menu_name;
    uint8_t bg;
    uint8_t value;
    uint8_t label;
    uint8_t dim;
    uint8_t hint;
    uint8_t hot;
    uint8_t alert;
    uint8_t cursor;
    uint8_t voice[3];
    uint8_t knob[4];
    uint8_t knob_text[4];
    uint8_t voice_text[3];
    uint8_t title_fg, title_bg;
    uint8_t menu_item, menu_value;
    uint8_t sel_fg, sel_bg;
    uint8_t toast_fg, toast_bg;
    uint8_t popup_bg;
    uint8_t popup_frame, popup_name, popup_bar, popup_track;
} theme;

const theme* theme_get(uint32_t id);

#define THEME_ATTR(t, role) GRID_ATTR((t)->role, (t)->bg)

#ifdef __cplusplus
}
#endif

#endif
