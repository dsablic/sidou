// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LIVE, the root view: what the instrument is doing, after the browser prototype's LIVE
 * (docs/art/README.md "LIVE").
 *
 * - a header line: the chip, the preset and the octave, in reverse video;
 * - each voice in a box (rows 1-6, 7-12, 13-18): its oscilloscope (160 x 32 pixels, the
 *   trace an overlay on each cell it crosses, over a dotted centre line), its envelope as a
 *   bar of lower eighths (ENV3), its note, waveform, pulse width and envelope stage. The
 *   trace's colour is the waveform's: pulse cyan, saw yellow, triangle light green, noise light
 *   grey;
 * - FILTER: the response of the filter's mode at its cutoff and resonance, 16 columns over
 *   30 Hz..12 kHz on a log scale, -36..+12 dB, the cutoff's column in yellow; recomputed only
 *   when one of them (or the chip) changes;
 * - VU: each voice's level and the output's left and right peaks after the effect slots, on
 *   a log scale (6 dB a cell), falling back slowly;
 * - the three effect slots: algorithm and Mix, '>' at the one the panel edits.
 *
 * The scopes come from the audio interrupt's tap (app/scope.h): the newest points, folded by
 * the oscillator's phase (scope_trace), `cycles` periods across the width, more for higher
 * notes; a hard-synced voice is folded by its sync source's phase, so the sync shape shows.
 * The trace's height is the waveform times the voice's envelope: flat at rest.
 *
 * It composes every frame (no changed function), within a budget of LIVE_VIEW_FRAME_CELLS
 * cells: the scopes get what the rest of the frame leaves, and a trace that does not fit waits
 * for a later frame (three voices of noise take turns). While a wipe or a raster runs it draws
 * what it drew last (screen_animating), as About does: the transitions are sized for a still
 * view. The tap runs only while LIVE is on top (live_view_tap): the audio interrupt pays for
 * it only then.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LIVE_VIEW_H
#define SIDOU_LIVE_VIEW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "scope.h"
#include "screen.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Each voice's box: from row LIVE_VIEW_BOX_ROW + LIVE_VIEW_BOX_ROWS * voice, 22 cells wide;
 * its scope from LIVE_VIEW_SCOPE_COL, one row in. */
#define LIVE_VIEW_BOX_ROW 1u
#define LIVE_VIEW_BOX_ROWS 6u
#define LIVE_VIEW_SCOPE_COL 1u
#define LIVE_VIEW_SCOPE_COLS (SCOPE_W / 8u)
#define LIVE_VIEW_SCOPE_ROWS 4u
#define LIVE_VIEW_SCOPE_PX (LIVE_VIEW_SCOPE_ROWS * 8u)
/* The trace's amplitude at full scale and envelope, in pixels from the centre line. */
#define LIVE_VIEW_AMP 13
/* The cells a frame of LIVE sends at most, its scopes held to what the rest leaves: 200 cells
 * and their windows are about 17.5 ms of SPI, inside the 20 ms budget (docs/art/README.md). */
#define LIVE_VIEW_FRAME_CELLS 200u
/* The filter's columns, and the VU meters' fall per frame in meter steps (scope_meter). */
#define LIVE_VIEW_FILTER_COLS 16u
#define LIVE_VIEW_FALL 3u
/* VU rows: the voices, then left and right. */
#define LIVE_VIEW_METERS (SCOPE_VOICES + 2u)
/* What the canvas shows besides the scopes, as bytes (live_view.c look). */
#define LIVE_VIEW_LOOK 40u

/* A frame composed off the grid, each cell then written to it once (live_view.c). */
typedef struct {
    uint8_t ch[GRID_ROWS][GRID_COLS];
    uint8_t attr[GRID_ROWS][GRID_COLS];
} live_view_canvas;

typedef struct {
    const live* l;
    scope* sc;      /* NULL: no tap, flat scopes and empty meters */
    uint32_t since; /* the tap's head when it went on: older points are stale */
    uint8_t on;     /* the tap is on */
    /* What the last update saw, drawn every frame. */
    uint8_t y[SCOPE_VOICES][SCOPE_W]; /* the traces, pixel rows of the scope (0 the top) */
    uint8_t env[SCOPE_VOICES];
    uint8_t state[SCOPE_VOICES];
    uint8_t sustain[SCOPE_VOICES];
    uint8_t meter[LIVE_VIEW_METERS]; /* 0..SCOPE_METER_MAX */
    /* The filter curve and what it was computed for. */
    uint8_t height[LIVE_VIEW_FILTER_COLS]; /* pixels, 0..48 */
    uint8_t mark;                          /* the cutoff's column */
    uint8_t curve_valid;
    uint16_t curve_cutoff;
    uint8_t curve_resonance;
    uint8_t curve_mode;
    uint8_t curve_model;
    uint16_t curve_hz;                /* the cutoff in Hz */
    uint8_t color[SCOPE_VOICES];      /* each drawn trace's colour */
    uint8_t drawn;                    /* the traces have been drawn once */
    uint8_t stale;                    /* bit v: voice v's overlays are to be drawn again */
    uint8_t ovl_free;                 /* the grid's free overlay slots after they were */
    uint8_t next_voice; /* the scope that goes first in the next frame's budget */
    uint32_t updates;   /* frames that read the tap (not held by a transition) */
    uint32_t held;      /* traces held back by the budget, voice by frame (diagnostic) */
    uint8_t prepared;   /* pend and the meters are this frame's (live_view_prepare) */
    uint8_t built;      /* the canvas has been built */
    uint8_t drawn_look[LIVE_VIEW_LOOK]; /* what it was built from */
    /* Working space of a compose, here rather than on the main loop's stack. */
    scope_point pts[SCOPE_READ_MAX];
    uint8_t pend[SCOPE_VOICES][SCOPE_W];
    live_view_canvas cv;
} live_view;

extern const view live_view_view;

/* Shows l, and the tap sc (may be NULL). Flat scopes until the tap has points. */
void live_view_init(live_view* lv, const live* l, scope* sc);
/* The tap on or off; on: only points written from now on are used. Every frame, before the
 * screen's (views_frame): on while LIVE is on top. */
void live_view_tap(live_view* lv, int on);
/* Reads the tap and works out the next frame's traces and meters now, half way between two
 * frames, so that the frame's compose is short: the main loop drains MIDI between the two
 * (docs/decisions/0005). Only while the tap is on; a compose with nothing prepared does it
 * itself. */
void live_view_prepare(live_view* lv);

#ifdef __cplusplus
}
#endif

#endif
