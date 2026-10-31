// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CREDITS_H
#define SIDOU_CREDITS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CREDITS_COLS 26u
#define CREDITS_LINES 8u

typedef struct {
    const char* heading;
    const char* line[CREDITS_LINES];
    uint8_t names;
} credits_page;

extern const credits_page credits_pages[];
extern const uint32_t credits_page_count;

#ifdef __cplusplus
}
#endif

#endif
