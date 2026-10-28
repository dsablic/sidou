// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LAB's drawings (app/lab_view.h; docs/art/README.md "LAB"): what a page shapes, drawn from the
 * live values in its area of the screen.
 *
 * The kit: a drawing is up to LAB_DRAW_LAYERS layers, sampled from a function (lab_draw_fn) every
 * LAB_DRAW_STEP pixel columns (x 0, 2, .. 238), each sample as wide: for column x, each layer's
 * vertical span (LAB_DRAW_NONE: nothing there), and which of them are lit. Layers 0 and 1 are
 * traces (each sample joined to the one before it and 2 pixels thick, as LIVE's scopes), layer 2
 * a marker (as given). A sample every 2 pixels (0.2 mm on the panel) halves the drawing's cost
 * a frame (fm1-emu: ENVELOPE's 70k instructions to about 35k). The spans are turned a band
 * of 8 columns at a time, on the stack, into 8 x 8 masks and set as the cells' pixel overlays
 * (app/grid.h grid_overlay): no bitmap is kept. A cell shows one colour: the marker's if it has a
 * marker's pixel, else the lit colour if it has a lit pixel, else layer 0's, else layer 1's. Only
 * the cells whose overlay differs from what is drawn are set (grid_overlay_same), within a budget
 * of cells; a cell the drawing leaves is cleared. What the budget cuts short is set by the next
 * call, which draws the same again (the grid is the record).
 *
 * The drawings, each from the instrument as it sounds now:
 * - OSC: one cycle of the waveform at its pulse width (the pulse width a sounding voice has now:
 *   the LFO moves the edge), the combined waveforms as the 6581's AND of the shapes, noise as a
 *   fixed scribble; with ring or sync, one cycle of the modulator, the carrier at the sync ratio
 *   (SYNC LEAD's; 1.5 for a sound of four voices, whose modulator is another key's), and the
 *   modulator as a second trace; the cycle is as long as the fine tune makes it. The PWM's range
 *   is a dotted band behind the pulse's edge (lab_draw_guides). Lit: K1 the pulse's edge, K2 and
 *   K3 the band, K4 the note's own cycle end (a marker).
 * - ENVELOPE: the ADSR to scale on a time axis of decades (1 ms to a minute, LAB_DRAW_DECADE
 *   pixels a decade, ticks at 10 ms, 0.1 s, 1 s and 10 s), from the SID's rate periods: the attack
 *   linear, the decay and release by the envelope's exponential steps. The key is let go at the
 *   middle (a held note of about a quarter second), or once the sustain is reached and held half
 *   as long again where that is later (at LAB_DRAW_GATE_MAX_X at the latest, so the release has
 *   room): every segment is in sight, a dim line marks the key's. Lit: the segment of the card
 *   turned. While a key sounds, the voice's envelope (the tap's) is a dot on the curve at its level.
 * - FILTER: the mode's response (lowpass, bandpass, highpass, notch and the mixes) on a log axis of
 *   FILT_PX_OCT columns an octave from 30 Hz (x FILT_X0) to 12 kHz, in dB (+6 at the top, -54 at
 *   the foot), from the filter's own transfer function with the damping the chip has (the
 *   resonance's 1 / Q, sid_resonance_k) and the cutoff the chip has now (cutoff_out through its
 *   curve, sid_cutoff_hz): an LFO, the wheel or velocity move the line. The cutoff is a line down
 *   the area; V1 V2 V3 under it at 2x in the voices' colours, dark when not routed. Lit: K1 the
 *   cutoff's line, K2 the curve about it (the peak), K3 the voices.
 * - MOD: one window of MOD_TICKS ticks (about 1.3 s) in three bands: the vibrato (its delay a flat
 *   start; its rate, depth and delay as set), the PWM's wave (its rate, its depth as a swing; dark
 *   without the pulse waveform) and, with ARP on, its staircase for a C major chord at its order,
 *   span and rate (a step is a rate's frames; clock rates at one clock a frame, 125 BPM). Lit: K1
 *   the vibrato's first cycle, K2 its whole trace, K3 its delay.
 * - FX: the three slots as a chain at 2x (the selected one marked, a slot that is off dark) and,
 *   under it, the selected slot's shape: the distortion's transfer curve against the straight line
 *   (the drive's gain law, the clip's cubic; CRUSH's steps), the modulation's LFO over two seconds
 *   at its rate and depth (the tremolo's and the auto-pan's shape from sine to square; WOW's slow
 *   wave and flutter), the delay's taps over 1.5 s (alternating sides on PING-PONG) and the
 *   reverbs' decay. A slot without a drawing (OFF, LO-FI) shows its name at 2x. Lit: K1..K3 the
 *   shape, K4 the slot's mark.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_LAB_DRAW_H
#define SIDOU_LAB_DRAW_H

#include <stdint.h>

#include "grid.h"
#include "live.h"
#include "scope.h"
#include "screen.h"
#include "theme.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The drawing's area in pixels: the full width, LAB_DRAW_ROWS cell rows. */
#define LAB_DRAW_W 240u
#define LAB_DRAW_ROWS 14u
#define LAB_DRAW_H (LAB_DRAW_ROWS * GRID_CELL_PX)
#define LAB_DRAW_LAYERS 3u
#define LAB_DRAW_STEP 2u
#define LAB_DRAW_MARKER 2u
#define LAB_DRAW_NONE 0xFFu
/* ENVELOPE's time axis: pixels a decade, from LAB_DRAW_T0 (1 ms in); the key let go at x
 * LAB_DRAW_GATE_X at the earliest, LAB_DRAW_GATE_MAX_X at the latest (lab_draw_gate_us). */
#define LAB_DRAW_DECADE 48u
#define LAB_DRAW_T0 4u
#define LAB_DRAW_GATE_X 120u
#define LAB_DRAW_GATE_MAX_X 200u

/* A column of a drawing: each layer's span from row lo to row hi of the area's pixels (lo
 * LAB_DRAW_NONE: nothing), and bit l of lit: layer l's pixels here are lit. */
typedef struct {
    uint8_t lo[LAB_DRAW_LAYERS];
    uint8_t hi[LAB_DRAW_LAYERS];
    uint8_t lit;
} lab_draw_col;

typedef void (*lab_draw_fn)(const void* ctx, uint32_t x, lab_draw_col* out);

typedef struct {
    lab_draw_fn fn;
    const void* ctx;
    uint8_t color[LAB_DRAW_LAYERS];
    uint8_t lit_color;
} lab_draw;

/* d's columns 0..LAB_DRAW_W - 1 as overlays on the cells of rows row0 .. row0 + LAB_DRAW_ROWS - 1,
 * at most *left cells changed (*left counts them down); cells the screen covers this frame are
 * left alone. 1 when the grid shows the whole drawing, 0 when something is left for the next
 * call. */
int lab_draw_apply(const lab_draw* d, grid* g, const screen* s, uint32_t row0, uint32_t* left);

/* The page that draws nothing and leaves the area clear (LAB's name page). */
#define LAB_DRAW_BLANK 0xFFu

/* A hash of what page's drawing shows now (the instrument's values it reads, the tap's envelope,
 * what is lit: hl, app/lab_view.h), so that an unchanged drawing costs this alone. */
uint32_t lab_draw_look(const live* l, const scope* sc, uint32_t page, uint32_t hl);
/* page's drawing (hl lit) onto the grid's area from row0, as lab_draw_apply. A page with no
 * drawing yet clears the area's overlays. */
int lab_draw_page(const live* l, const scope* sc, uint32_t page, uint32_t hl, const theme* th,
                  grid* g, const screen* s, uint32_t row0, uint32_t* left);
/* The glyphs under page's drawing, into the canvas from row0: OSC's PWM band, ENVELOPE's ticks
 * and the key's line. */
void lab_draw_guides(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                     uint32_t row0);

/* ENVELOPE: the time at pixel column x in microseconds from the note-on, and the column of a
 * time (the last whose time is at most t). The attack's and a decay's or release's full span
 * (255 to 0) in microseconds for a setting (0..15). */
uint32_t lab_draw_time_us(uint32_t x);
uint32_t lab_draw_x_of_us(uint32_t t);
uint32_t lab_draw_attack_us(uint32_t rate);
uint32_t lab_draw_decay_us(uint32_t rate);
/* When ENVELOPE's note of the ADSR ad, sr ($d405, $d406) is let go, in microseconds. */
uint32_t lab_draw_gate_us(uint8_t ad, uint8_t sr);
/* The envelope's level (0..255) at t microseconds of a note of the sound's ADSR (ad, sr: $d405,
 * $d406) let go at gate_us. */
uint32_t lab_draw_level(uint8_t ad, uint8_t sr, uint32_t gate_us, uint32_t t);

#ifdef __cplusplus
}
#endif

#endif
