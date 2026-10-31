// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LINK_H
#define SIDOU_LINK_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LINK_VERSION 1u
#define LINK_PAYLOAD_MAX 48u
#define LINK_FRAME_MAX (7u + LINK_PAYLOAD_MAX + (LINK_PAYLOAD_MAX + 6u) / 7u + 2u)
#define LINK_FRAME_PACKETS ((LINK_FRAME_MAX + 2u) / 3u)
#define LINK_TIMEOUT_MS 3000u
#define LINK_RELEASE_MS 500u
#define LINK_RATE 22
#define LINK_BURST 96
#define LINK_RING_KEEP 32u
#define LINK_IN_MAX 16u

enum {
    LINK_START = 0x01,
    LINK_HEARTBEAT = 0x02,
    LINK_STOP = 0x03,
    LINK_KEYFRAME = 0x04,
    LINK_FONT = 0x05,
    LINK_PALETTE = 0x06,
    LINK_PANEL = 0x07
};
enum {
    LINK_HELLO = 0x41,
    LINK_BEAT = 0x42,
    LINK_STOPPED = 0x43,
    LINK_COLOURS = 0x44,
    LINK_GLYPHS = 0x45,
    LINK_CELLS = 0x46,
    LINK_PIXELS = 0x47
};
#define LINK_HAS_CELLS 1u
#define LINK_HAS_PIXEL_CELLS 2u
#define LINK_HAS_PANEL 4u
#define LINK_OVERLAY 0x10u

typedef struct {
    uint8_t dirty[(GRID_CELLS + 7u) / 8u];
    uint32_t rows;
    uint8_t on;
    uint8_t want;
    uint8_t font_next;
    uint8_t stage, cmd, n;
    uint8_t in[LINK_IN_MAX];
    uint16_t scan;
    uint16_t held;
    uint16_t pressed;
    int8_t steps[8];
    int32_t credit;
    uint32_t heard;
    uint32_t panel_at;
    uint32_t last;
    uint32_t frames;
    uint32_t bytes;
    const char* name;
} link_state;

void link_init(link_state* l, const char* name);
void link_byte(link_state* l, uint8_t b, uint32_t now_ms);
void link_mark(link_state* l, uint32_t row, uint32_t col, uint32_t len);
uint32_t link_poll(link_state* l, const grid* g, uint32_t now_ms, uint32_t room, int busy,
                   uint8_t* out);
void link_drop(link_state* l);
uint32_t link_take(link_state* l, uint32_t* pressed);
int32_t link_steps(link_state* l, uint32_t enc);

#ifdef __cplusplus
}
#endif

#endif
