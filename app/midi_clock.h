// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The MIDI clock follower: timing clocks (F8) in, the tempo, the clocks' phase and the
 * player's frame period out; start, continue, stop and song position as transport events.
 *
 * A pure state machine: no allocation, no libm, single-precision float only (the pi32v2 has
 * a single-precision FPU). It runs in the main loop, one call per message.
 *
 * **Stamps** are in samples (the audio clock), unsigned and wrapping: the ring's stamps
 * (fm1_ticks(), 24 MHz) converted by the caller. Only differences are used, so a stamp may
 * wrap; two stamps compared must be within 2^31 samples (13 hours).
 *
 * **The fit.** The tempo and phase come from a least-squares line through the last
 * MIDI_CLOCK_WINDOW clocks (four beats): stamp = a + b * clock number. Why a fit and not a
 * second-order loop (a PLL):
 * - The fit's slope is a weighted average, with positive weights, of the intervals between
 *   consecutive clocks in the window. So it never leaves the range of the intervals it saw:
 *   it cannot overshoot a tempo step or a ramp, where a second-order loop rings unless it is
 *   overdamped (and then it is slow to lock).
 * - It locks in a known time, with no state to settle: the line is exact for a steady clock
 *   however few clocks it has. It reports a tempo from MIDI_CLOCK_LOCK clocks (one beat).
 * - The input's jitter (the TRS poll's 0.5 ms; USB's 1 ms frames) is white and bounded, and
 *   for that a straight line's least-squares fit is the best unbiased estimate there is.
 *
 * Four beats, not the two the plan suggested: the bounds (0.1 % tempo, 1 ms phase) must hold
 * from two beats after the clock starts, when the growing window holds 48 clocks, and a
 * 48-clock fit is then at its limit. Clocks that land anywhere in USB's 1 ms frames give it up
 * to 0.085 % tempo error at 200 BPM (midi_clock.rs; the slope's error falls as the window's
 * length to the power 1.5). Kept at 48, every later estimate is as close to the bound; grown
 * to 96, the error halves once the clock has run four beats. The cost: the fit lags a ramp
 * by about two beats (the window's middle), settles a step within four, and its ring takes
 * 768 bytes.
 *
 * The x axis is the clock's number since the clock started, so a clock left out of the fit
 * (a hold's, below) leaves a hole rather than shifting the clocks after it.
 *
 * **Holds:** the store's held erases (prog, M2a Task 4) mask interrupts for up to 35 ms, so
 * the TRS bytes that arrive meanwhile are stamped late, together, at the first poll after.
 * The caller reports each hold (midi_clock_hold; on the device platform/fm1/store_fm1.c's
 * operate, through midi.c's midi_clock_held, after every erase or program with interrupts
 * masked) before it feeds the messages that came in during it; clocks stamped from its start to one clock period after its end count (for the
 * song position) but stay out of the fit, as their stamps say nothing of when they were sent.
 *
 * **Lost:** a clock that does not come for MIDI_CLOCK_LOST_MS (300 ms; below 8.3 BPM) is
 * lost: the fit is dropped, and the next clock starts a new one.
 *
 * **Dropped clocks** (a gap in the MIDI ring, a cable pulled for a moment) are not inferred
 * from the gap: a halved tempo looks the same as every other clock missing. The fit takes a
 * missed clock as one long interval (its tempo is off by up to 1.5/96, 1.6 %, for four beats),
 * and the song position falls one clock behind until the next start or song position.
 *
 * **The song position** counts clocks while running, 24 a beat, 6 a 16th (SPP's unit): FA
 * sets it to 0, F2 to its value times 6, and the next clock after FA or FB is the clock at
 * that position; FC stops it. Clocks while stopped keep the tempo and leave the position
 * alone, as MIDI says. */
#ifndef SIDOU_MIDI_CLOCK_H
#define SIDOU_MIDI_CLOCK_H

#include <stdint.h>

#include "midi_in.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The clocks in the fit: four beats. */
#define MIDI_CLOCK_WINDOW 96u
/* The clocks the fit needs before it reports a tempo: one beat. */
#define MIDI_CLOCK_LOCK 24u
/* No clock for this long and the clock is lost. */
#define MIDI_CLOCK_LOST_MS 300u
/* Clocks a beat, and a song position's 16th. */
#define MIDI_CLOCK_PER_BEAT 24u
#define MIDI_CLOCK_PER_16TH 6u
/* The phase correction at most 1/MIDI_CLOCK_CORRECTION of a beat a beat (1 %). */
#define MIDI_CLOCK_CORRECTION 100u

typedef enum {
    MIDI_CLOCK_EV_NONE = 0,
    MIDI_CLOCK_EV_START = 1,    /* FA: play from position 0 on the next clock */
    MIDI_CLOCK_EV_CONTINUE = 2, /* FB: play on from `position` on the next clock */
    MIDI_CLOCK_EV_STOP = 3,     /* FC */
    MIDI_CLOCK_EV_POSITION = 4, /* F2: `position` changed */
    MIDI_CLOCK_EV_LOST = 5      /* no clock for MIDI_CLOCK_LOST_MS; the fit is dropped */
} midi_clock_event;

typedef struct {
    uint32_t stamp[MIDI_CLOCK_WINDOW]; /* the fitted clocks, a ring: their stamps */
    uint32_t number[MIDI_CLOCK_WINDOW]; /* and their numbers since the clock started */
    uint32_t sample_rate;
    uint32_t lost_after; /* MIDI_CLOCK_LOST_MS in samples */
    uint32_t clocks;     /* the next clock's number */
    uint32_t last;       /* the last clock's stamp, fitted or not */
    float period;        /* the fit: samples a clock; 0 until MIDI_CLOCK_LOCK clocks */
    float at_ref;        /* the fit at clock number ref_number, from ref_stamp */
    uint32_t ref_stamp, ref_number; /* the newest fitted clock */
    uint32_t hold_start, hold_end;
    uint32_t position;  /* the song position, in clocks: the next clock's while running */
    uint32_t base_number, base_position; /* the last clock while running and its position */
    uint8_t head, count; /* the ring's oldest entry; its entries */
    uint8_t active;      /* a clock has come and is not lost */
    uint8_t running;     /* between FA or FB and FC */
    uint8_t has_hold;
    uint8_t based;       /* a clock has come while running: base_* hold */
} midi_clock;

void midi_clock_init(midi_clock* c, uint32_t sample_rate);

/* A message from midi_in, stamped: F8, FA, FB and FC (MIDI_REALTIME) and song position
 * (MIDI_SONG_POSITION); anything else is ignored. Returns the transport event it is, or
 * MIDI_CLOCK_EV_LOST for a clock that came after the clock was lost (the fit restarts with
 * it) and was not yet reported by midi_clock_poll. */
midi_clock_event midi_clock_msg(midi_clock* c, const midi_msg* msg, uint32_t stamp);

/* A hold from start to end (stamps). Report it before the messages it stamped late. */
void midi_clock_hold(midi_clock* c, uint32_t start, uint32_t end);

/* Call with the time now, at least every 100 ms: MIDI_CLOCK_EV_LOST once when the clock
 * has not come for MIDI_CLOCK_LOST_MS, else MIDI_CLOCK_EV_NONE. */
midi_clock_event midi_clock_poll(midi_clock* c, uint32_t now);

/* The clock period, samples a clock in Q16; 0 when no tempo is known (fewer than
 * MIDI_CLOCK_LOCK clocks since the clock started). */
uint32_t midi_clock_period_q16(const midi_clock* c);

/* The fitted time of the clock at song position `position` (clocks), past or future: 1 and
 * *stamp, or 0 while stopped, before a clock has come since FA or FB, or with no tempo. */
int midi_clock_time_of(const midi_clock* c, uint32_t position, uint32_t* stamp);

/* The player's frame period, samples a frame in Q16, that lands its rows on the clock: a
 * beat is rows_per_beat rows of `tempo` frames (the song's MIDI chunk; GoatTracker's tempo),
 * and funktempo alternates two tempos, so a beat is (tempo0 + tempo1) / 2 * rows_per_beat
 * frames (pass the tempo twice without funktempo). `late` is how far, in samples, the player
 * is behind the clock (its beat against midi_clock_time_of's; negative when ahead): it is
 * taken off the next beat, at most 1/MIDI_CLOCK_CORRECTION of it, so drift never builds up.
 * 0 when no tempo is known or a tempo or rows_per_beat is 0. */
uint32_t midi_clock_frame_q16(const midi_clock* c, uint8_t tempo0, uint8_t tempo1,
                              uint8_t rows_per_beat, int32_t late);

#ifdef __cplusplus
}
#endif

#endif
