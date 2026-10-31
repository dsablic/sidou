// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LEARN_VIEW_H
#define SIDOU_LEARN_VIEW_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "panel.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LEARN_VIEW_COL 0u
#define LEARN_VIEW_ROW 20u
#define LEARN_VIEW_W 30u
#define LEARN_VIEW_H 6u
#define LEARN_VIEW_CHARS 14u
#define LEARN_VIEW_FRAME_CELLS 200u
#define LEARN_VIEW_NAME 10u

typedef struct {
    live* l;
    midi_learn* ml;
    screen* s;
    const panel* p;
    uint8_t drawn[3];
} learn_view;

extern const view learn_view_view;

void learn_view_init(learn_view* lv, live* l, midi_learn* ml);
int learn_view_open(learn_view* lv, screen* s, const panel* p, uint32_t enc, uint32_t held);
int learn_view_is_open(const learn_view* lv);
void learn_view_save(learn_view* lv);
void learn_view_ask_clear(learn_view* lv);
void learn_view_frame(learn_view* lv, screen* s, uint32_t now_ms);
void learn_view_param_name(uint32_t param, char* buf);

#ifdef __cplusplus
}
#endif

#endif
