// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PUNCH_VIEW_H
#define SIDOU_PUNCH_VIEW_H

#include <stdint.h>

#include "screen.h"
#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PUNCH_VIEW_ALL ((1u << SONG_T_PUNCH_PADS) - 1u)
#define PUNCH_VIEW_FRAME_CELLS 200u
#define PUNCH_VIEW_TITLE_ROW 1u
#define PUNCH_VIEW_HOLD_ROW 4u
#define PUNCH_VIEW_PAD_ROW 6u
#define PUNCH_VIEW_PAD_STEP 3u
#define PUNCH_VIEW_HINT_ROW 25u
#define PUNCH_VIEW_BACK_ROW 28u

typedef struct {
    song_view* sv;
    uint8_t held;
    uint8_t sent;
    uint8_t playing;
    uint8_t built;
    uint8_t drawn_pads;
    uint8_t drawn_playing;
    uint8_t drawn_dim;
    uint8_t drawn_theme;
} punch_view;

extern const view punch_view_view;

void punch_view_init(punch_view* pv, song_view* sv);
void punch_view_keys(punch_view* pv, uint32_t down);
void punch_view_frame(punch_view* pv, int shows);
uint32_t punch_view_lit(const punch_view* pv);
const char* punch_view_pad_name(uint32_t pad);

#ifdef __cplusplus
}
#endif

#endif
