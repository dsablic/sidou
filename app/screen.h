// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screen: a stack of views drawn on the cell grid (app/grid.h), the panel's events routed
 * to the view on top, a toast line, and two transitions, a wipe and a raster.
 *
 * A view is a table of functions in flash: enter, exit, input and compose, each called with
 * the view's state (a pointer given at push). The view on top of the stack has the focus: it
 * is composed every frame and gets the events. Pushing a view calls its enter; popping it
 * calls its exit, and the view under it has the focus again with its state as it left it.
 *
 * Push and pop are for input handlers (the views' input, the capture, the global handler)
 * and the code that drives the screen. A view's enter and exit must not push or pop: the
 * screen refuses (-1) while it runs them.
 *
 * Events: a button press or an encoder's detents, with the buttons held at the time (so a
 * view can read "hold a button, turn a knob"). screen_input hands an event to the capture
 * first (screen_capture, a modal grab such as MIDI learn), then to the global handler
 * (screen_global, HOME in app/views.c), then to the view on top. Whoever consumes it returns
 * 1; 0 means nobody did, and the caller falls back on the panel's own mapping (app/views.c).
 * The capture sees every event first, HOME included, so a capturing view can take HOME as
 * "cancel". A capture belongs to the stack as it was when it was set: when a pop takes the
 * stack below that depth (the view that set it, or one under it, goes), it is released.
 *
 * Every frame (screen_frame, 25 fps) the view on top composes, then the screen draws its own
 * layers over it in this order: the toast, the raster, the wipe. They change the grid the way
 * any compose does, so they cost what they change and no more. The view composes when
 * something it shows may have changed: its changed function says so (or it has none), or the
 * screen forces it: after a view switch or screen_redraw, while a wipe, a raster or a toast
 * runs and on the frame after one ends, so that what the layers drew goes. An idle screen
 * composes nothing and sends nothing. The layers:
 * - wipe: a view switch reveals the new view SCREEN_WIPE_ROWS rows a frame (top first on a
 *   push, bottom first on a pop) behind a one-row white edge; the rows not reached yet are
 *   held (grid_hold), so the panel keeps the old view there. A full screen is spread over
 *   8 frames instead of the 77 ms of SPI of one.
 * - raster: the border (the outer ring of cells) takes the raster ramp's colours in bands
 *   that move one row every SCREEN_RASTER_FRAMES frames, then gets the view's own colours
 *   back. It waits while a wipe runs.
 * - toast: a line of text on SCREEN_TOAST_ROW for a while.
 *
 * Portable C with no hardware access and no allocation; the host tests drive it directly. */
#ifndef SIDOU_SCREEN_H
#define SIDOU_SCREEN_H

#include <stdint.h>

#include "grid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Views on the stack at most, the root included. */
#define SCREEN_DEPTH 4u
/* The toast's row and its text: columns 2..27, inside the frame and its margin
 * (docs/art/README.md). */
#define SCREEN_TOAST_ROW 27u
#define SCREEN_TOAST_COL 2u
#define SCREEN_TOAST_CHARS 26u
#define SCREEN_TOAST_MS 1500u
/* The block cursor is on for this many frames, then off as long (320 ms at 25 fps). */
#define SCREEN_BLINK_FRAMES 8u
/* Rows a wipe reveals per frame. Each frame sends at most these rows and the edge: 5 rows of
 * 30 cells, 19.3 KB, 12.8 ms of SPI at 12 MHz, inside the 20 ms budget. */
#define SCREEN_WIPE_ROWS 4u
/* The raster moves every other frame: a step recolours the 116 border cells, 15.5 KB and
 * 10.3 ms of SPI, so 5.2 ms a frame on average. */
#define SCREEN_RASTER_FRAMES 2u
/* The raster ramp's length (docs/art/README.md): white, yellow, light green, cyan, light blue,
 * purple, light red. */
#define SCREEN_RAMP 7u

typedef enum {
    SCREEN_PRESS = 0, /* button id pressed */
    SCREEN_TURN = 1   /* encoder id turned by steps */
} screen_event_kind;

typedef struct {
    uint8_t kind;  /* screen_event_kind */
    uint8_t id;    /* button or encoder id (app/panel.h) */
    int16_t steps; /* SCREEN_TURN: detents, + = clockwise */
    uint32_t held; /* the buttons held (bit i: button i), the pressed one included */
} screen_event;

typedef struct screen screen;

/* Any function may be NULL but compose. When compose runs it draws every cell of the grid:
 * what it leaves alone keeps whatever the grid holds, the layers' last frame or another
 * view's. */
typedef struct {
    const char* name;
    void (*enter)(screen* s, void* state);
    void (*exit)(screen* s, void* state);
    /* 1: consumed. */
    int (*input)(screen* s, void* state, const screen_event* e);
    void (*compose)(screen* s, void* state, grid* g);
    /* 1 when compose would draw something other than it did last time (the instrument
     * changed, the cursor blinked). NULL: composed every frame. */
    int (*changed)(screen* s, void* state);
} view;

typedef enum {
    SCREEN_CUT = 0,       /* at once: the whole screen in one frame */
    SCREEN_WIPE_DOWN = 1, /* top row first: a view opens */
    SCREEN_WIPE_UP = 2    /* bottom row first: back to the view below */
} screen_wipe;

/* 1: consumed. The capture and the global handler. */
typedef int (*screen_capture_fn)(void* ctx, const screen_event* e);

typedef struct {
    const view* v;
    void* state;
} screen_entry;

struct screen {
    grid* g;
    screen_entry stack[SCREEN_DEPTH];
    uint8_t depth;
    uint8_t wipe;      /* screen_wipe; SCREEN_CUT: none running */
    uint8_t wipe_rows; /* rows revealed so far */
    uint8_t raster_frames; /* frames of raster left */
    uint8_t toast_on;      /* 0 none, 1 showing, 2 waiting for its first frame */
    uint8_t redraw;        /* compose on the next frame whatever the view says */
    uint8_t busy;          /* in a view's enter or exit: no push or pop */
    uint8_t capture_depth; /* the stack's depth when the capture was set */
    uint32_t frame;        /* frames run */
    uint32_t now;          /* the last frame's time, ms */
    uint32_t toast_until;  /* toast_on 2: its length in ms */
    char toast[SCREEN_TOAST_CHARS + 1u];
    screen_capture_fn capture;
    void* capture_ctx;
    screen_capture_fn global;
    void* global_ctx;
};

/* The raster ramp as GRID_ colours. */
extern const uint8_t screen_ramp[SCREEN_RAMP];

/* root is at the bottom of the stack for good and is entered. */
void screen_init(screen* s, grid* g, const view* root, void* state);
/* v on top, entered and focused. 0, or -1 when the stack is full or from a view's enter or
 * exit (nothing changes). */
int screen_push(screen* s, const view* v, void* state, screen_wipe w);
/* The top view exited and the one below focused; a capture set above the new depth is
 * released. 0, or -1 at the root or from a view's enter or exit. */
int screen_pop(screen* s, screen_wipe w);
const view* screen_top(const screen* s);
/* Events go to fn before everything else until screen_capture(s, NULL, NULL), or until a pop
 * takes the stack below its depth now. */
void screen_capture(screen* s, screen_capture_fn fn, void* ctx);
/* Events the capture passes on go to fn before the view on top. */
void screen_global(screen* s, screen_capture_fn fn, void* ctx);
/* 1 if the capture, the global handler or the view on top consumed e. */
int screen_input(screen* s, const screen_event* e);
/* The next frame composes, whatever the view says (someone else drew on the grid). */
void screen_redraw(screen* s);
/* Shows text (clipped to SCREEN_TOAST_CHARS) for ms from the next frame on, replacing a toast
 * that shows. */
void screen_toast(screen* s, const char* text, uint32_t ms);
/* Sweeps the raster over the border for steps steps (SCREEN_RASTER_FRAMES frames each). */
void screen_raster(screen* s, uint32_t steps);
/* 1 while the block cursor is on. */
int screen_blink(const screen* s);
/* 1 while a wipe or a raster runs. */
int screen_animating(const screen* s);
/* One frame at now_ms: compose (if needed, above), toast, raster, wipe. */
void screen_frame(screen* s, uint32_t now_ms);

#ifdef __cplusplus
}
#endif

#endif
