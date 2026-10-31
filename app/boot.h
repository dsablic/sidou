// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_BOOT_H
#define SIDOU_BOOT_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "logo.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

#define BOOT_SCENE_VERSION_ROW 1u
#define BOOT_SCENE_KEY_ROW 28u
#define BOOT_TITLE_LINE_TOP 6u
#define BOOT_LOGO_ROW 8u
#define BOOT_LOGO_ROWS 5u
#define BOOT_TITLE_LINE_BOTTOM 14u

#define BOOT_TYPE_MS 800u
#define BOOT_TYPE_CHAR_MS 70u
#define BOOT_SEARCH_MS 2200u
#define BOOT_LOAD_MS 3000u
#define BOOT_READY_MS 5600u
#define BOOT_RUN_MS 6200u
#define BOOT_RUN_CHAR_MS 100u
#define BOOT_TITLE_MS 7000u
#define BOOT_FULL_TITLE_MS 3000u
#define BOOT_SHORT_TITLE_MS 1200u
#define BOOT_STRIPE_MS 80u

typedef enum {
    BOOT_PROMPT = 0,
    BOOT_TYPING = 1,
    BOOT_SEARCHING = 2,
    BOOT_LOADING = 3,
    BOOT_READY = 4,
    BOOT_TITLE = 5,
    BOOT_DONE = 6
} boot_phase;

typedef enum {
    BOOT_GO_ON = 0,
    BOOT_REVEAL = 1,
    BOOT_END = 2
} boot_step;

typedef struct {
    const live* l;
    const char* version;
    uint32_t start;
    uint32_t at;
    uint32_t note_ons;
    uint8_t started;
    uint8_t full;
    uint8_t skip;
    uint8_t home_edge;
    uint8_t title;
    uint32_t logo_frame;
    const logo* logo;
} boot;

extern const view boot_view;

void boot_init(boot* b, const live* l, const char* version, int full, int home_held);
boot_phase boot_phase_at(const boot* b, uint32_t ms);
boot_step boot_update(boot* b, uint32_t now_ms);
int boot_capture(void* ctx, const screen_event* e);

#ifdef __cplusplus
}
#endif

#endif
