// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LIVE, the root view: what the instrument is doing, drawn to be read at 23 mm
 * (docs/art/README.md "LIVE"), after the screen-readability prototype's liveNew.
 *
 * - rows 0-1: the preset's name at 2 times the font (a song's mutes and solos in red in its
 *   place: "M13 S2", voices 1 and 3 muted, voice 2 soloed), the chip's model and the octave
 *   small on the right;
 * - rows 2-13, three voice lanes of 4 rows: the voice's number at 2x in its colour with the
 *   waveform's name under it, its oscilloscope (160 x 32 pixels from column 3, the trace an
 *   overlay on each cell it crosses, 2 pixels thick, over a dotted centre line) in the voice's
 *   colour (dark grey and flat when the voice is silent), and the note at 2x on the right with
 *   the envelope's stage under it;
 * - rows 15-19, the four knobs in their left-to-right order, what K1..K4 turn on LIVE: each a
 *   label small in the knob's colour (CUTOFF, RESO, PULSE, PRESET), its value at 2x in white
 *   (11K, 6, 50%, SAW), a bar in the knob's colour and its name K1..K4;
 * - rows 21-26: FILTER's mode at 2x (and KEYS:B while the keys play chip B), the three effect
 *   slots' algorithms (> at the one the panel edits), and VU: each voice's level in its colour
 *   and the output's left and right peaks after the effect slots, 6 dB a cell.
 * The colours are the theme's roles (app/theme.h).
 *
 * The scopes come from the audio interrupt's tap (app/scope.h): the newest points, folded by
 * the oscillator's phase (scope_trace), `cycles` periods across the width, more for higher
 * notes; a hard-synced voice is folded by its sync source's phase, so the sync shape shows.
 * The trace's height is the waveform times the voice's envelope: flat at rest.
 *
 * It composes every frame (no changed function), within a budget of LIVE_VIEW_FRAME_CELLS
 * cells: the scopes get what the rest of the frame and the screen's toast, raster and cover
 * (screen_layer_cells: MIDI learn's frame over it) leave, and a trace that does not fit waits
 * for a later frame (three voices of noise take turns). The canvas (small text and bars) is
 * built whole once; after that each part (the header, a voice's lane, the knob row, the bottom
 * band) is drawn again only when what it shows changed, and a meter only its own cells: a
 * frame of moving meters costs a few cells, not the 900 of the canvas. The large text is
 * drawn on the grid itself (grid_text_big) for the rows that were copied. While a wipe or a
 * raster runs it draws what it drew last (screen_animating), as About does: the transitions
 * are sized for a still view. The tap runs only while LIVE (or SCENE) shows (live_view_tap): the
 * audio interrupt pays for it only then.
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

/* Each voice's lane: from row LIVE_VIEW_LANE_ROW + LIVE_VIEW_LANE_ROWS * voice; its scope from
 * LIVE_VIEW_SCOPE_COL, on the lane's rows; the voice's number and the note on the lane's
 * middle two. */
#define LIVE_VIEW_LANE_ROW 2u
#define LIVE_VIEW_LANE_ROWS 4u
#define LIVE_VIEW_SCOPE_COL 3u
#define LIVE_VIEW_SCOPE_COLS (SCOPE_W / 8u)
#define LIVE_VIEW_SCOPE_ROWS 4u
#define LIVE_VIEW_SCOPE_PX (LIVE_VIEW_SCOPE_ROWS * 8u)
/* The trace's amplitude at full scale and envelope, in pixels from the centre line. */
#define LIVE_VIEW_AMP 13
/* The cells a frame of LIVE sends at most, its scopes held to what the rest leaves: 200 cells
 * and their windows are about 17.5 ms of SPI, inside the 20 ms budget (docs/art/README.md). */
#define LIVE_VIEW_FRAME_CELLS 200u
/* The knob row: four columns of 8, 7, 7 and 8 cells side by side (a bar a cell shorter than its
 * column); a label on LIVE_VIEW_KNOB_ROW, the value at 2x on the two rows after it, the bar and
 * the knob's name K1..K4 on the rows after those. */
#define LIVE_VIEW_KNOB_ROW 15u
#define LIVE_VIEW_KNOB_VALUE_ROW 16u
#define LIVE_VIEW_KNOB_BAR_ROW 18u
#define LIVE_VIEW_KNOB_TAG_ROW 19u
/* The bottom band: FILTER's label, the mode at 2x on the rows after it; the effect slots'
 * lines from LIVE_VIEW_SLOT_ROW; VU's meters from LIVE_VIEW_METER_ROW, a row each. */
#define LIVE_VIEW_BAND_ROW 21u
#define LIVE_VIEW_MODE_ROW 22u
#define LIVE_VIEW_SLOT_ROW 24u
#define LIVE_VIEW_METER_ROW 22u
#define LIVE_VIEW_BAND_ROWS 6u
/* The VU meters' fall per frame in meter steps (scope_meter). */
#define LIVE_VIEW_FALL 3u
/* VU rows: the voices, then left and right. */
#define LIVE_VIEW_METERS (SCOPE_VOICES + 2u)
/* What the canvas shows besides the scopes, as bytes (live_view.c look): the header 5, each
 * voice's lane 6, the knob row 4, the bottom band's mode and slots 5 and the meters. */
#define LIVE_VIEW_LOOK (5u + 6u * SCOPE_VOICES + 4u + 5u + LIVE_VIEW_METERS)

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
    /* The song's mutes (bits 0-2) and solos (bits 3-5) for the header (song_view_mutes): the
     * views set it before each frame. */
    uint8_t mutes;
    /* What the last update saw, drawn every frame. */
    uint8_t y[SCOPE_VOICES][SCOPE_W]; /* the traces, pixel rows of the scope (0 the top) */
    uint32_t cols[SCOPE_VOICES][SCOPE_W]; /* the traces as columns (live_view.c columns) */
    uint8_t env[SCOPE_VOICES];
    uint8_t state[SCOPE_VOICES];
    uint8_t sustain[SCOPE_VOICES];
    uint8_t meter[LIVE_VIEW_METERS]; /* 0..SCOPE_METER_MAX */
    uint8_t color[SCOPE_VOICES];      /* each drawn trace's colour */
    uint8_t drawn;                    /* the traces have been drawn once */
    uint8_t stale;                    /* bit v: voice v's overlays are to be drawn again */
    uint8_t ovl_free;                 /* the grid's free overlay slots after they were */
    uint8_t next_voice; /* the scope that goes first in the next frame's budget */
    uint32_t updates;   /* frames that read the tap (not held by a transition) */
    uint32_t held;      /* traces held back by the budget, voice by frame (diagnostic) */
    uint32_t rebuilds;  /* the whole canvas built: once, when nothing was (diagnostic) */
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
/* The tap is LIVE's or not; on: only points written from now on are used. Every frame, before
 * the screen's (views_frame): on while LIVE shows (on top, or under MIDI learn's frame); the
 * tap itself (scope.on) is switched by views_frame, for SCENE as well. Off, the
 * traces go flat and the envelopes and meters to zero: what LIVE shows when it comes back,
 * until the tap has points, is the instrument at rest, not what it was when it went. */
void live_view_tap(live_view* lv, int on);
/* The filter's cutoff in Hz as the popup says it, "1234" or "11.4K" for the cutoff the chip has
 * now (cutoff_out), into buf (8 characters). */
void live_view_cutoff_text(const live* l, char* buf);
/* Knob k's value (0 K1: the cutoff, 1 the resonance, 2 the pulse width in percent, 3 the preset's
 * first three letters) as LIVE's knob row says it at 2x: "980", "1.2K", "12K", "6", "50%", "SAW",
 * into buf (5 bytes). */
void live_view_knob_text(const live* l, uint32_t k, char* buf);
/* The control register voice v plays with, without the gate: 0 for a voice that makes no sound
 * (SYNC LEAD's voices 2 and 3, voice 3 only clocking the sync). */
uint8_t live_view_voice_control(const live* l, uint32_t v);
/* A control register's waveform colour (docs/art/README.md): pulse cyan, saw yellow, triangle
 * light green, noise light grey, combined white, none light blue. */
uint8_t live_view_wave_color(uint8_t control);
/* Reads the tap and works out the next frame's traces and meters now, half way between two
 * frames, so that the frame's compose is short: the main loop drains MIDI between the two
 * (docs/decisions/0005). Only while the tap is on; a compose with nothing prepared does it
 * itself. */
void live_view_prepare(live_view* lv);

#ifdef __cplusplus
}
#endif

#endif
