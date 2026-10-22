// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The LIVE view's tap on the audio: the voices' waveforms, envelopes and the output's peaks,
 * from the audio interrupt to the main loop (app/live_view.h draws them).
 *
 * The producer is the audio interrupt (platform/fm1/audio.c): scope_render renders a block
 * of the chip in one sid_render_tap, which calls it back after every SCOPE_DECIMATE samples
 * to write a point: each voice's waveform output and its oscillator's phase. Once a block it
 * copies each voice's envelope (ENV3, its state and sustain level), and scope_peaks raises
 * the peaks of the block's stereo output after the effect slots. A tapped render gives
 * exactly sid_render's samples, so the tap changes nothing in the audio; one render a block
 * keeps a render's per-call setup out of the tap's cost. It runs only while the consumer asks for it (`on`): nothing is added to the
 * interrupt while LIVE is not showing.
 *
 * The consumer is the main loop: scope_read copies the newest points, oldest first.
 *
 * Single producer, single consumer, one core: the producer is an interrupt that preempts the
 * consumer, never the other way round. The ring is a history, not a queue: the producer
 * never waits and never looks at the consumer; it overwrites the oldest point. The consumer
 * takes the newest points it wants each frame, and drops what it has no use for. A copy the
 * producer overtook while it ran (it wrote over the oldest points copied) is dropped whole.
 * The envelope bytes are single bytes, read whole; a peak is an aligned 16-bit word.
 *
 * The scope is drawn phase-folded (an equivalent-time display): a point's waveform value is
 * placed by its oscillator's phase, not by its time, so the trace stays still, shows a
 * waveform whole at any pitch, and needs far fewer points than a time-base display would
 * (scope_trace).
 *
 * Portable C with no hardware access and no allocation. scope_render and scope_peaks run in
 * the audio interrupt (platform/fm1/app.ld puts them in SRAM); everything else in the main
 * loop. */
#ifndef SIDOU_SCOPE_H
#define SIDOU_SCOPE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCOPE_VOICES 3u
/* Samples per point: 2757 points a second, 110 a frame (40 ms). */
#define SCOPE_DECIMATE 16u
/* Points kept, a power of two: 93 ms at 2757 points a second. */
#define SCOPE_RING 256u
/* The most points scope_read copies: the ring less a margin for the points the producer may
 * write while the copy runs (a block's SCOPE_DECIMATE-sample chunks; a copy takes far less
 * than a block). */
#define SCOPE_READ_MAX (SCOPE_RING - 32u)
/* scope_peaks looks at every SCOPE_PEAK_STRIDE-th frame of a block. */
#define SCOPE_PEAK_STRIDE 8u
/* scope_trace's width in pixels. */
#define SCOPE_W 160u
/* scope_meter's full scale: 9 cells of bar eighths. */
#define SCOPE_METER_MAX 72u

#ifndef __cplusplus
_Static_assert((SCOPE_RING & (SCOPE_RING - 1u)) == 0u, "the ring's size is a power of two");
#endif

typedef struct {
    uint8_t wave[SCOPE_VOICES];  /* each voice's waveform output: the top 8 of its 12 bits */
    uint8_t phase[SCOPE_VOICES]; /* each voice's oscillator phase: its accumulator's top 8 bits */
} scope_point;

typedef struct {
    scope_point point[SCOPE_RING];
    volatile uint32_t head; /* producer: points written (wraps); the newest is head - 1 */
    /* Producer, once a block: each voice's envelope as the last sample left it. */
    volatile uint8_t env[SCOPE_VOICES];     /* ENV3: the envelope counter, 0..255 */
    volatile uint8_t state[SCOPE_VOICES];   /* a sid_env_state */
    volatile uint8_t sustain[SCOPE_VOICES]; /* the sustain level, in both nibbles */
    /* The largest |sample| of the left and right output since the last take, in two slots:
     * the producer raises peak[peak_slot]; the consumer (scope_take_peaks) moves peak_slot to
     * the other slot, then reads and clears the one it left, which no block touches any more,
     * so a block's peak is never cleared unseen. */
    volatile uint16_t peak[2][2];
    volatile uint8_t peak_slot;
    volatile uint8_t on; /* consumer: 1 while it reads; the producer taps only then */
} scope;

/* Off, with no points. Before the producer runs. */
void scope_init(scope* sc);

/* Producer. frames samples of chip into out, as sid_render renders them; while sc->on, in
 * chunks of SCOPE_DECIMATE samples (frames a multiple of it), each followed by a point, and
 * then the envelopes. */
void scope_render(scope* sc, sid* chip, int16_t* out, uint32_t frames);
/* Producer. While sc->on, raises the peaks with every SCOPE_PEAK_STRIDE-th frame of lr
 * (frames frames, interleaved L, R). */
void scope_peaks(scope* sc, const int16_t* lr, uint32_t frames);

/* Consumer. Copies the newest of the points written since `since` (a head value), at most n
 * and SCOPE_READ_MAX, into out, oldest first. Returns how many, or 0 when the producer wrote
 * over the copy while it ran (the copy is dropped). */
uint32_t scope_read(const scope* sc, uint32_t since, scope_point* out, uint32_t n);
/* The rule scope_read checks after its copy: the copy from point `oldest` on is intact if the
 * producer, now at head, has not come round to it. */
static inline int scope_intact(uint32_t oldest, uint32_t head) {
    return head - oldest < SCOPE_RING;
}
/* Consumer. The peaks since the last call into out (left, right), and clears them. */
void scope_take_peaks(scope* sc, uint16_t out[2]);

#ifdef SIDOU_REG_RING_RACE_HOOK
/* Host tests only (the device never defines it): hook(ctx) runs inside scope_take_peaks where
 * the audio interrupt can come. Per thread; NULL turns it off. */
void scope_set_race_hook(void (*hook)(void* ctx), void* ctx);
#endif

/* Consumer. Voice `voice`'s trace across SCOPE_W pixels, `cycles` (1..8) periods of the
 * oscillator of voice `by` (the voice itself, or its hard-sync source): each point's waveform
 * value lands in the column its phase belongs to, a column showing the mean of its points (so
 * a note that holds still draws the same trace frame after frame), and the columns no point
 * reached take a straight line between their neighbours, round the period. out[x] is a
 * waveform value, 128 the middle; with no points, all 128. */
void scope_trace(const scope_point* p, uint32_t n, uint32_t voice, uint32_t by, uint32_t cycles,
                 uint8_t out[SCOPE_W]);
/* An amplitude (0..32768, full scale 32768) on a meter of SCOPE_METER_MAX steps, 6 dB per 8
 * steps: 32768 is full, 64 (-54 dB) and below empty. */
uint32_t scope_meter(uint32_t amplitude);

#ifdef __cplusplus
}
#endif

#endif
