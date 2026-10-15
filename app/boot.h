// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The boot sequence on the screen (docs/art/README.md "Boot"): a view over the status screen
 * at power-on (app/views.h views_boot).
 *
 * The full sequence, after the browser prototype's: the banner and READY., then
 * LOAD"SIDOU",8,1 typed, SEARCHING FOR SIDOU, LOADING with raster stripes in the border,
 * READY. and RUN, then the title card: the logo's scene (app/logo.h), the version and PRESS ANY
 * KEY. The short one is the title card alone. Then the status screen wipes in.
 *
 * Short is the default: without storage (M2) the firmware cannot tell the first boot after
 * an install from the others. HOME held while powering on plays the full one instead:
 * platform/fm1/main.c reads the debounced buttons once the key scan runs (FM1_DEB_PRESS + 1
 * scan frames, about 3 ms) and passes HOME's level to views_boot. The key scan also reports
 * a key held at power-on as one press edge when its debounce ends (fm1_input.h), which reaches
 * the boot's capture with the first input_poll: that first HOME press is part of the hold,
 * not a skip.
 *
 * It never holds the instrument up: audio, the tick, MIDI and the interrupts start before the
 * screen's first frame (platform/fm1/main.c), and the main loop plays the keys and MIDI while
 * it runs. Anything ends it: a button or encoder (the capture takes it, so it does nothing
 * else: no octave step, no chip toggle), and a note from the keys or MIDI, which also plays,
 * even one shorter than a frame (live's note-on count, not only the gates, tells).
 *
 * While it shows, changes the instrument makes (MIDI can change the chip or the octave) show
 * no toast and no raster over it (app/views.c).
 *
 * Times are in ms from the boot's first frame. The view composes every frame.
 *
 * Portable C with no hardware access and no allocation. */
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

/* The title card is the logo's full-screen scene (app/logo.h, Metal Chrome's) with the
 * version line and PRESS ANY KEY over it, on rows that are black in the scene, above its top
 * raster bar and below its bottom one. */
#define BOOT_SCENE_VERSION_ROW 1u
#define BOOT_SCENE_KEY_ROW 28u
/* For a logo without a scene (Pulse Train), the card puts it between two ramp lines, a blank
 * row from each: its slot is BOOT_LOGO_ROWS rows from BOOT_LOGO_ROW, centred, with the name
 * and the version below. */
#define BOOT_TITLE_LINE_TOP 6u
#define BOOT_LOGO_ROW 8u
#define BOOT_LOGO_ROWS 5u
#define BOOT_TITLE_LINE_BOTTOM 14u

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
    uint32_t note_ons; /* live's note-on count at the start: a note since then ends it */
    uint8_t started;
    uint8_t full;
    uint8_t skip;
    uint8_t home_edge; /* HOME held at power-on: its press edge is still to come */
    uint8_t title;       /* the look shown: 1 the title card, 0 the prompt */
    uint32_t logo_frame; /* the title card's logo: its frame, still while a wipe runs */
    const logo* logo;    /* the title card's: logo_default */
} boot;

extern const view boot_view;

/* home_held: HOME was held at power-on: the whole sequence, and the key scan's press edge of
 * that hold is part of it. Else the title card alone. */
void boot_init(boot* b, const live* l, const char* version, int home_held);
/* The phase at ms from the start. */
boot_phase boot_phase_at(const boot* b, uint32_t ms);
/* One frame at now_ms, before the screen's: the time, the notes played (they end it), and
 * what the screen must do. */
boot_step boot_update(boot* b, uint32_t now_ms);
/* Its capture (screen_capture): takes every event. The first HOME press of a boot started
 * with HOME held does nothing (it is the hold's); anything else ends it at the next frame. */
int boot_capture(void* ctx, const screen_event* e);

#ifdef __cplusplus
}
#endif

#endif
