// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_ABOUT_H
#define SIDOU_ABOUT_H

#include <stdint.h>

#include "grid.h"
#include "logo.h"
#include "screen.h"
#include "song_io.h"

#ifdef __cplusplus
extern "C" {
#endif

#define ABOUT_PAGE_FRAMES 120u
#define ABOUT_ROW_DELAY 4u
#define ABOUT_ROW_FRAMES 86u
#define ABOUT_STARS 24u
#define ABOUT_BARS 2u
#define ABOUT_LOGO_ROW 0u
#define ABOUT_LOGO_ROWS 9u
#define ABOUT_RAMP_TOP 9u
#define ABOUT_HEADING_ROW 10u
#define ABOUT_LINES_ROW 12u
#define ABOUT_RAMP_BOTTOM 20u
#define ABOUT_BAR_ROW 21u
#define ABOUT_BAR_ROWS 9u
#define ABOUT_SCROLL_ROW 25u
#define ABOUT_SCROLL_PX 2u

typedef struct {
    const char* version;
    uint32_t frame;
    uint8_t bar[ABOUT_BARS];
    const logo* logo;
    song_io* io;
    uint8_t silent;
} about;

extern const view about_view;

void about_init(about* a, const char* version);

#ifdef __cplusplus
}
#endif

#endif
