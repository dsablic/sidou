// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_SLOTS_H
#define SIDOU_FX_SLOTS_H

/* The effect slots after the chip mix: Distortion -> Modulation -> FX -> stereo out.
 * Each slot runs one algorithm (docs/decisions/0004-effect-algorithms.md) and has the
 * same five controls: algorithm, Amount, Param A, Param B and Mix (dry/wet).
 *
 * - Mono up to Modulation; Modulation makes stereo; FX feeds its wet path (L + R) / 2 and
 *   keeps the dry path stereo.
 * - Amount, A, B and Mix are smoothed (SID_FX_SMOOTH_MS) on every sample: the one-pole is
 *   evaluated every 16 samples (on the slots' own clock) and followed as a straight line
 *   in between; each snaps onto its target on a 32-sample grid. The clock restarts
 *   whenever the slots start from rest, so block sizes never change the result.
 * - Switching algorithm fades the old wet path out over SID_SLOT_FADE_MS, starts the new
 *   one from rest (the FX slot first clears its share of the pool, a little per sample),
 *   runs it silently until its delay line holds its longest delay (so no tap reads the
 *   cleared part while audible), and fades it in: the dry path never stops, so there is
 *   no click.
 * - Exact bypass: a slot whose Mix is 0 and settled, with no switch pending, is out of
 *   the path and its state is not touched. With every slot out, sid_slots_process
 *   writes the input to both channels unchanged and touches no state.
 * - A slot coming back from rest starts its algorithm from rest too (no stale tails).
 *
 * What Amount, A and B do per algorithm is listed with the enums below. */

#include <stdint.h>

#include "fx_chorus.h"
#include "fx_crt.h"
#include "fx_crush.h"
#include "fx_echo.h"
#include "fx_flanger.h"
#include "fx_fuzz.h"
#include "fx_noise.h"
#include "fx_overdrive.h"
#include "fx_phaser.h"
#include "fx_plate.h"
#include "fx_reverb.h"
#include "fx_tape.h"
#include "fx_tremolo.h"
#include "fx_wow.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_SLOT_FADE_MS 20.0f
/* The FX pool: delay and both reverbs share it (one runs at a time). 128 KiB, seen as
 * int16 samples (delay, light reverb) or floats (lush reverb). */
#define SID_SLOT_POOL_LEN 65536u /* int16 samples */

enum { SID_SLOT_DIST, SID_SLOT_MOD, SID_SLOT_FX, SID_SLOT_COUNT };

/* Distortion (mono). */
enum {
    SID_DIST_NONE,      /* wet = dry */
    SID_DIST_TAPE,      /* Amount drive */
    SID_DIST_OVERDRIVE, /* Amount drive, A tone (dark -> bright), B bias */
    SID_DIST_FUZZ,      /* Amount drive, A octave-up blend, B starved bias */
    SID_DIST_CRUSH,     /* Amount bit depth, A sample rate */
    SID_DIST_LOFI,      /* Amount TV tone, A hum, B noise */
    SID_DIST_COUNT
};

/* Modulation (mono in, stereo out). */
enum {
    SID_MOD_NONE,
    SID_MOD_CHORUS,  /* Amount depth, A rate, B ensemble (adds the fast LFO) */
    SID_MOD_FLANGER, /* Amount depth, A rate, B feedback (-0.9 .. 0.9) */
    SID_MOD_PHASER,  /* Amount depth, A rate, B feedback */
    SID_MOD_TREMOLO, /* Amount depth, A rate, B shape (sine -> square) */
    SID_MOD_AUTOPAN, /* Amount width, A rate, B shape */
    SID_MOD_WOW,     /* Amount wow, A flutter */
    SID_MOD_COUNT
};

/* FX (stereo). */
enum {
    SID_FXSLOT_NONE,
    SID_FXSLOT_DELAY,         /* Amount feedback, A time, B damping */
    SID_FXSLOT_PINGPONG,      /* Amount feedback, A time, B damping */
    SID_FXSLOT_REVERB_LIGHT,  /* Amount decay, A damping, B diffusion */
    SID_FXSLOT_REVERB_LUSH,   /* Amount decay, A damping, B bandwidth (input brightness) */
    SID_FXSLOT_COUNT
};

typedef struct {
    uint32_t algorithm;
    float amount, a, b, mix; /* each 0..1 */
} sid_slot_params;

/* Switch phases. */
enum { SID_SLOT_RUN, SID_SLOT_FADE_OUT, SID_SLOT_CLEAR, SID_SLOT_FADE_IN };

typedef union {
    int16_t s16[SID_SLOT_POOL_LEN];
    float f32[SID_SLOT_POOL_LEN / 2u];
} sid_slot_pool;

typedef struct {
    sid_slot_params target; /* target.algorithm is the one asked for */
    float amount, a, b, mix; /* smoothed */
    float inc[4];            /* their slopes over the current chunk */
    uint32_t algorithm;      /* the one running */
    uint32_t phase;
    uint32_t awake;     /* in the path on the last sample processed */
    uint32_t clear_pos; /* FX: pool words cleared so far */
    uint32_t warm;      /* samples left to run the new algorithm silently before fading in */
    float fade;         /* 0 .. 1 position of the wet path's fade */
} sid_slot;

typedef union {
    sid_fx_tape tape;
    sid_fx_overdrive overdrive;
    sid_fx_fuzz fuzz;
    sid_fx_crush crush;
    struct {
        sid_fx_crt crt;
        sid_fx_noise noise;
    } lofi;
} sid_slot_dist_state;

typedef union {
    sid_fx_chorus chorus;
    sid_fx_flanger flanger;
    sid_fx_phaser phaser;
    sid_fx_tremolo tremolo;
    sid_fx_wow wow;
} sid_slot_mod_state;

typedef union {
    sid_fx_echo echo;
    sid_fx_reverb reverb;
    sid_fx_plate plate;
} sid_slot_fx_state;

/* Fields are private to fx_slots.c; the struct is public so callers can allocate it. */
typedef struct {
    sid_slot slot[SID_SLOT_COUNT];
    uint32_t sample_rate;
    uint32_t seed;
    uint32_t clock; /* samples processed since the slots last started from rest */
    float smooth_ahead, fade_step; /* (1 - k)^16 for the chunked smoothing */
    uint32_t clear_per_sample;
    sid_slot_pool* pool; /* owned by the caller */
    sid_slot_dist_state dist;
    sid_slot_mod_state mod;
    sid_slot_fx_state fx;
} sid_slots;

/* sample_rate in [8000, 96000]. `pool` must stay valid; it need not be cleared. Every
 * slot starts off (algorithm NONE, Mix 0). */
void sid_slots_init(sid_slots* s, uint32_t sample_rate, uint32_t seed, sid_slot_pool* pool);
/* New targets for one slot: the algorithm (clamped to the slot's range) and the four
 * controls (clamped to [0, 1]; NaN gives 0). */
void sid_slots_set(sid_slots* s, uint32_t slot, const sid_slot_params* p);
/* Mono in, interleaved stereo out (out[2i] left, out[2i + 1] right); in and out must not
 * overlap. */
void sid_slots_process(sid_slots* s, const int16_t* in, int16_t* out, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
