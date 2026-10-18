// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "theme.h"

static const theme THEMES[THEME_COUNT] = {
    {
        .name = "HI-CONTRAST",
        .menu_name = "HIGH",
        .bg = GRID_BLACK,
        .value = GRID_WHITE,
        .label = GRID_GREY,
        .dim = GRID_DARK_GREY,
        .hot = GRID_YELLOW,
        .alert = GRID_LIGHT_RED,
        .cursor = GRID_DARK_GREY,
        .voice = {GRID_LIGHT_RED, GRID_CYAN, GRID_LIGHT_GREEN},
        .knob = {GRID_YELLOW, GRID_CYAN, GRID_LIGHT_GREEN, GRID_LIGHT_RED},
        .title_fg = GRID_GREY,
        .title_bg = GRID_BLACK,
        .menu_item = GRID_WHITE,
        .menu_value = GRID_CYAN,
        .sel_fg = GRID_YELLOW,
        .sel_bg = GRID_DARK_GREY,
        .toast_fg = GRID_BLACK,
        .toast_bg = GRID_YELLOW,
        .popup_bg = GRID_BLACK,
        .popup_frame = THEME_ACCENT,
        .popup_name = THEME_ACCENT,
        .popup_bar = THEME_ACCENT,
        .popup_track = GRID_DARK_GREY,
    },
    {
        .name = "C64",
        .menu_name = "C64",
        .bg = GRID_BLUE,
        .value = GRID_WHITE,
        .label = GRID_LIGHT_BLUE,
        .dim = GRID_GREY,
        .hot = GRID_YELLOW,
        .alert = GRID_LIGHT_RED,
        .cursor = GRID_DARK_GREY,
        .voice = {GRID_LIGHT_RED, GRID_CYAN, GRID_LIGHT_GREEN},
        .knob = {GRID_YELLOW, GRID_CYAN, GRID_LIGHT_GREEN, GRID_LIGHT_RED},
        .title_fg = GRID_BLUE,
        .title_bg = GRID_LIGHT_BLUE,
        .menu_item = GRID_LIGHT_BLUE,
        .menu_value = GRID_LIGHT_BLUE,
        .sel_fg = GRID_BLUE,
        .sel_bg = GRID_LIGHT_BLUE,
        .toast_fg = GRID_BLUE,
        .toast_bg = GRID_LIGHT_BLUE,
        .popup_bg = GRID_BLACK,
        .popup_frame = GRID_LIGHT_BLUE,
        .popup_name = GRID_YELLOW,
        .popup_bar = GRID_LIGHT_GREEN,
        .popup_track = GRID_DARK_GREY,
    },
};

const theme* theme_get(uint32_t id) {
    return &THEMES[id < THEME_COUNT ? id : THEME_HIGH_CONTRAST];
}
