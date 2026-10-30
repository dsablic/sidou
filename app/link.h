// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* FM-1 Live Link, the device's side (docs/reference/live-link.md): a computer mirrors the
 * screen and drives the panel over USB-MIDI SysEx.
 *
 * A session is the host's: START begins it (the device answers HELLO, its capabilities, and
 * marks every cell to be sent), a HEARTBEAT every 2 s keeps it, STOP or LINK_TIMEOUT_MS
 * without either ends it. Never on by itself.
 *
 * The mirror (cells mode): every run the panel is sent marks its cells (link_mark, a bit
 * per cell). link_poll hands out one frame at a time: the replies first, then the palette and
 * the font when asked for, then the marked cells, in screen order from where it stopped, as
 * the grid holds them now (the code and colour the panel shows, a large-text piece's
 * character, the overlay), a pixel region's cells with their 64 background pixels as
 * colours. A cell marked again before it went out goes once, as it is then: the newest state
 * wins. A frame carries at most LINK_PAYLOAD_MAX bytes and goes only while the platform's
 * output has room for it and LINK_RING_KEEP slots more and nothing else is under way there
 * (`busy`: a SysEx reply or transfer), within LINK_RATE bytes a millisecond.
 *
 * Remote control: a PANEL frame's buttons are held until the next one, or LINK_RELEASE_MS
 * after the last; its presses and encoder steps wait for link_take and link_steps.
 *
 * Portable C with no hardware access: platform/fm1/midi.c feeds and drains it, the host
 * tests drive it directly. */
#ifndef SIDOU_LINK_H
#define SIDOU_LINK_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define LINK_VERSION 1u
/* A frame's decoded payload at most, and its whole SysEx frame (F0, 7D, FM1L, the command,
 * the packed payload, the checksum, F7). */
#define LINK_PAYLOAD_MAX 48u
#define LINK_FRAME_MAX (7u + LINK_PAYLOAD_MAX + (LINK_PAYLOAD_MAX + 6u) / 7u + 2u)
/* USB-MIDI event packets of the longest frame (3 bytes each). */
#define LINK_FRAME_PACKETS ((LINK_FRAME_MAX + 2u) / 3u)
#define LINK_TIMEOUT_MS 3000u
#define LINK_RELEASE_MS 500u
/* Payload bytes a millisecond (22 KB/s), and the credit a quiet spell may save. */
#define LINK_RATE 22
#define LINK_BURST 96
/* Output slots a frame leaves free: MIDI out's tick (16) and clock out's (4) and a margin. */
#define LINK_RING_KEEP 32u
/* A host frame's packed payload and checksum at most (PANEL's 11 bytes are 13 packed). */
#define LINK_IN_MAX 16u

/* Host to device. */
enum {
    LINK_START = 0x01,
    LINK_HEARTBEAT = 0x02,
    LINK_STOP = 0x03,
    LINK_KEYFRAME = 0x04,
    LINK_FONT = 0x05,
    LINK_PALETTE = 0x06,
    LINK_PANEL = 0x07
};
/* Device to host. */
enum {
    LINK_HELLO = 0x41,
    LINK_BEAT = 0x42,
    LINK_STOPPED = 0x43,
    LINK_COLOURS = 0x44,
    LINK_GLYPHS = 0x45,
    LINK_CELLS = 0x46,
    LINK_PIXELS = 0x47
};
/* HELLO's feature bits. */
#define LINK_HAS_CELLS 1u
#define LINK_HAS_PIXEL_CELLS 2u
#define LINK_HAS_PANEL 4u
/* A cell record's third byte, for a cell that is no piece: an overlay follows (its colour
 * in the low nibble, then its 8 mask rows). */
#define LINK_OVERLAY 0x10u

typedef struct {
    uint8_t dirty[(GRID_CELLS + 7u) / 8u]; /* bit per cell: to be sent */
    uint32_t rows;                         /* bit per row: it may have cells to be sent */
    uint8_t on;                            /* a session runs */
    uint8_t want;                          /* replies to send (LINK_WANT_*, link.c) */
    uint8_t font_next;                     /* the next glyph to send, 0xFF: none */
    uint8_t stage, cmd, n;                 /* the host frame being read */
    uint8_t in[LINK_IN_MAX];
    uint16_t scan;      /* the cell the next search starts from */
    uint16_t held;      /* the host's buttons (bit i: button i) */
    uint16_t pressed;   /* its presses not yet taken */
    int8_t steps[8];    /* its encoder steps not yet taken */
    int32_t credit;     /* payload bytes the rate allows now */
    uint32_t heard;     /* the last START or HEARTBEAT */
    uint32_t panel_at;  /* the last PANEL */
    uint32_t last;      /* link_poll's last clock */
    uint32_t frames;    /* frames sent */
    uint32_t bytes;     /* their payload bytes */
    const char* name;   /* the firmware's name and version, for HELLO */
} link_state;

void link_init(link_state* l, const char* name);
/* A byte of USB SysEx as it came (F0, the data, F7 or the status that cut it), now_ms the
 * main loop's clock. */
void link_byte(link_state* l, uint8_t b, uint32_t now_ms);
/* Cells [col, col + len) of row went to the panel (clipped at the row's end). */
void link_mark(link_state* l, uint32_t row, uint32_t col, uint32_t len);
/* The next frame into out (LINK_FRAME_MAX bytes, F0 to F7), or 0: nothing to send, or not
 * now. room: the output's free event-packet slots; busy: something else is going out
 * there. Ends the session LINK_TIMEOUT_MS after the last heartbeat (a STOPPED follows) and
 * lets the host's buttons go LINK_RELEASE_MS after its last PANEL. */
uint32_t link_poll(link_state* l, const grid* g, uint32_t now_ms, uint32_t room, int busy,
                   uint8_t* out);
/* Ends a session at once, with nothing sent (the host went away). */
void link_drop(link_state* l);
/* The host's held buttons; its presses since the last call into *pressed (cleared). */
uint32_t link_take(link_state* l, uint32_t* pressed);
/* Encoder enc's steps from the host since the last call (cleared). */
int32_t link_steps(link_state* l, uint32_t enc);

#ifdef __cplusplus
}
#endif

#endif
