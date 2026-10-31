// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_UI_SOUND_H
#define SIDOU_UI_SOUND_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    UI_SOUND_BLIP = 0,
    UI_SOUND_CONFIRM = 1,
    UI_SOUND_CHIME = 2,
    UI_SOUND_BUZZ = 3,
    UI_SOUND_CHUNK = 4,
    UI_SOUND_COUNT = 5
} ui_sound_id;

#define UI_SOUND_NONE 0xffu
#define UI_SOUND_MAX_BYTES 128u

typedef struct {
    live* l;
    live_sink_fn sink;
    void* sink_ctx;
    uint8_t on;
    uint8_t hush;
    uint8_t id;
    uint8_t pos;
    uint8_t voice;
    uint8_t wave;
    uint8_t phase;
    uint8_t keys;
    uint16_t tail;
    uint16_t free_in[LIVE_VOICES];
    uint32_t stamp[LIVE_VOICES];
    live_sink_fn live_sink;
    void* live_ctx;
} ui_sound;

void ui_sound_init(ui_sound* us, live* l, live_sink_fn sink, void* sink_ctx);
void ui_sound_set(ui_sound* us, int on);
void ui_sound_hush(ui_sound* us, int on);
void ui_sound_play(ui_sound* us, ui_sound_id id);
void ui_sound_tick(ui_sound* us);
int ui_sound_busy(const ui_sound* us);
const uint8_t* ui_sound_data(ui_sound_id id);

#ifdef __cplusplus
}
#endif

#endif
