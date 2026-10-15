// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The boot sequence on the screen (docs/art/README.md "Boot"): a view over the status screen
 * at power-on (app/views.h views_boot).
 *
 * The full sequence, after the browser prototype's: the banner and READY., then
 * LOAD"SIDOU",8,1 typed, SEARCHING FOR SIDOU, LOADING with raster stripes in the border,
 * READY. and RUN, then the title card: the logo (app/logo.h) on black. The short one is the
 * title card alone. Then the status screen wipes in.
 *
 * Short is the default: without storage (M2) the firmware cannot tell the first boot after
 * an install from the others. HOME held while powering on plays the full one instead: held
 * (or pressed) within BOOT_HOLD_MS of the start. A key held at power-on gives the key scan no
 * press edge, so the held buttons are what tells (boot_held).
 *
 * It never holds the instrument up: audio and MIDI start before the screen, and the main loop
 * plays the keys and MIDI while it runs. Anything ends it: a button or encoder (the capture
 * takes it, so it does nothing else: no octave step, no chip toggle), and a note from the keys
 * or MIDI, which also plays.
 *
 * Times are in ms from the boot's first frame. The view composes every frame.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_BOOT_H
#define SIDOU_BOOT_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The full sequence (the prototype's timing). */
#define BOOT_TYPE_MS 800u     /* LOAD"SIDOU",8,1 starts */
#define BOOT_TYPE_CHAR_MS 70u /* a character typed */
#define BOOT_SEARCH_MS 2200u  /* SEARCHING FOR SIDOU */
#define BOOT_LOAD_MS 3000u    /* LOADING, stripes in the border */
#define BOOT_READY_MS 5600u   /* READY., the stripes stop */
#define BOOT_RUN_MS 6200u     /* RUN starts */
#define BOOT_RUN_CHAR_MS 100u
#define BOOT_TITLE_MS 7000u       /* the title card */
#define BOOT_FULL_TITLE_MS 3000u  /* the title card shows this long in the full sequence */
#define BOOT_SHORT_TITLE_MS 1200u /* and in the short one */
/* The stripes change every other frame at most: a change recolours the border's 116 cells,
 * about 10 ms of SPI. */
#define BOOT_STRIPE_MS 80u
/* A HOME press this soon after the start comes from HOME held at power-on: the full boot. */
#define BOOT_HOLD_MS 300u

typedef enum {
    BOOT_PROMPT = 0,    /* the banner, READY. and the cursor */
    BOOT_TYPING = 1,    /* LOAD"SIDOU",8,1 */
    BOOT_SEARCHING = 2, /* SEARCHING FOR SIDOU */
    BOOT_LOADING = 3,   /* LOADING, the border striped */
    BOOT_READY = 4,     /* READY. and RUN */
    BOOT_TITLE = 5,     /* the title card */
    BOOT_DONE = 6
} boot_phase;

/* What boot_update asks of the screen. */
typedef enum {
    BOOT_GO_ON = 0,
    BOOT_REVEAL = 1, /* the look changed (to or from the title card): screen_reveal */
    BOOT_END = 2     /* done or skipped: pop the view */
} boot_step;

typedef struct {
    const live* l;
    const char* version; /* "V" and the version, at most 14 characters */
    uint32_t start;      /* the first frame's time, ms */
    uint32_t at;         /* ms from the start at the last update */
    uint8_t started;
    uint8_t full;
    uint8_t skip;
    uint8_t title;       /* the look shown: 1 the title card, 0 the prompt */
    uint32_t logo_frame; /* the title card's logo: its frame, still while a wipe runs */
} boot;

extern const view boot_view;

/* full: the whole sequence, else the title card alone. */
void boot_init(boot* b, const live* l, const char* version, int full);
/* The phase at ms from the start. */
boot_phase boot_phase_at(const boot* b, uint32_t ms);
/* One frame at now_ms, before the screen's: the time, the notes playing (they end it), and
 * what the screen must do. */
boot_step boot_update(boot* b, uint32_t now_ms);
/* Its capture (screen_capture): takes every event. HOME within BOOT_HOLD_MS of the start makes
 * it the full one (from the start); anything else ends it at the next frame. */
int boot_capture(void* ctx, const screen_event* e);
/* The buttons held (bit i: button i), at every key scan: HOME held within BOOT_HOLD_MS of a
 * short boot's start makes it the full one. */
void boot_held(boot* b, uint32_t held);

#ifdef __cplusplus
}
#endif

#endif
