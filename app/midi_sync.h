// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_SYNC_H
#define SIDOU_MIDI_SYNC_H

#include <stdint.h>

#include "midi_clock.h"
#include "midi_in.h"
#include "song_transport.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    MIDI_SYNC_AUTO = 0,
    MIDI_SYNC_FOLLOW = 1,
    MIDI_SYNC_OFF = 2,
    MIDI_SYNC_MODES = 3
} midi_sync_mode;

#define MIDI_SYNC_TICKS_PER_SAMPLE 544u
#define MIDI_SYNC_LATENCY 128u
#define MIDI_SYNC_DELAY 352u
#define MIDI_SYNC_ALIGN_SAMPLES 88u
#define MIDI_SYNC_OUT_MAX 4u
#define MIDI_SYNC_CABLE 0u

enum { MIDI_SYNC_IDLE = 0, MIDI_SYNC_LOADING = 1, MIDI_SYNC_PLAYING = 2, MIDI_SYNC_STOPPED = 3 };

enum {
    MIDI_SYNC_WANT_START = 1u,
    MIDI_SYNC_WANT_RESUME = 2u,
    MIDI_SYNC_WANT_STOP = 4u,
    MIDI_SYNC_WANT_SEEK = 8u,
    MIDI_SYNC_WANT_FREE = 16u
};

#define MIDI_SYNC_TUNE_ASK 0u
typedef int (*midi_sync_tune_fn)(void* ctx, uint32_t realtime);

typedef struct midi_sync {
    midi_clock clock;
    const song_transport* t;
    uint8_t mode;
    uint8_t clock_out;
    uint8_t rows_per_beat;
    uint8_t subtune;
    uint8_t want;
    uint8_t held;
    uint8_t aligning;
    uint8_t synced;
    uint8_t fresh;
    uint32_t seek_call;
    uint32_t seek_pos;
    uint8_t seek_on;
    uint32_t c_anchor, p_anchor;
    uint32_t next_row;
    uint32_t a_ticks, a_samples;
    uint8_t anchored;
    uint32_t r_ticks, r_base;
    uint8_t out_state;
    uint8_t out_common_wait;
    uint8_t out_room;
    uint32_t out_clock;
    uint32_t sent;
    uint32_t aligns, periods, lost;
    midi_sync_tune_fn tune;
    void* tune_ctx;
} midi_sync;

void midi_sync_init(midi_sync* s);
void midi_sync_bind(midi_sync* s, const song_transport* t);
void midi_sync_tune(midi_sync* s, midi_sync_tune_fn fn, void* ctx);
void midi_sync_song_changed(midi_sync* s);
void midi_sync_set(midi_sync* s, uint32_t mode, uint32_t clock_out);
void midi_sync_rows(midi_sync* s, uint32_t rows_per_beat);
void midi_sync_anchor(midi_sync* s, uint32_t ticks, uint32_t samples);
uint32_t midi_sync_sample_of(const midi_sync* s, uint32_t ticks);
void midi_sync_held(midi_sync* s, uint32_t from, uint32_t to);
void midi_sync_msg(midi_sync* s, const midi_msg* msg, uint32_t ticks);
int midi_sync_following(const midi_sync* s);
uint32_t midi_sync_poll(midi_sync* s, uint32_t ticks, uint32_t out[MIDI_SYNC_OUT_MAX]);

const char* midi_sync_mode_name(uint32_t mode);

#ifdef __cplusplus
}
#endif

#endif
