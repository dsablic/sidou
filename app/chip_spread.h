// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* CHIP SPREAD (SETTINGS' SPREAD): chip A to the left and chip B to the right, as on a stereo
 * two-SID setup (platform/fm1/audio.c, after the effect slots; decision record 0004, "The
 * output stage: CHIP SPREAD").
 *
 * SPREAD is 0..CHIP_SPREAD_MAX percent. At 0 nothing runs and the output is the mono one, bit
 * for bit. Otherwise, with phi = SPREAD / 100 x pi / 4, the mid (A + B, the slots' input as
 * ever) takes cos phi and the side (A - B) sin phi: L = mid cos phi + side sin phi, R = mid
 * cos phi - side sin phi. So chip A sits at (cos phi + sin phi, cos phi - sin phi) and B
 * mirrored: equal power at every SPREAD (cos^2 + sin^2 = 1 for each chip), and at 100 % A is
 * on the left alone, B on the right, each sqrt 2 (+3 dB) where mono gave it 1 on both sides.
 * SPREAD separates the chips, so it acts only while both sound (the user, 2026-10-23: "SPREAD
 * should do channel separation"): a chip sounding alone stays in the centre, as mono, at any
 * SPREAD. The separation glides in as the second chip starts sounding and out as one goes quiet
 * (chip_spread_glide_step): phi above is SPREAD's angle times the glide's share g, 0 to 1 over
 * CHIP_SPREAD_GLIDE blocks (about 0.1 s), and the gains move linearly between the mono pair and
 * SPREAD's, mid 1 + g (cos phi - 1) and side g sin phi (within 0.7 dB of equal power half way,
 * equal at either end). At g = 0 the stage does not run: mono, bit for bit. A chip sounds while
 * one of its envelopes is at CHIP_SPREAD_ON or more; both stay sounding until one has had all
 * three under CHIP_SPREAD_OFF for CHIP_SPREAD_HOLD blocks: the hysteresis, in level and time, keeps
 * the gaps between a phrase's notes from fluttering the image. Chip B out of use is quiet.
 *
 * The effects: every slot's wet path takes the mono mix (the FX slot's always took (L + R) /
 * 2), so the side passes the slots' dry paths only, at their gain (fx_slots.h sid_slots_dry,
 * the product of 1 - Mix): a reverb or a delay keeps the dry spread and adds its own stereo
 * tail; a slot fully wet hears the chips as one. Each slot taking a stereo input instead would
 * run Distortion and Modulation twice, about 300 instructions a sample at their heaviest:
 * over the slots' budget (decision record 0004).
 *
 * It takes the slots' output before their int16 conversion (fx_slots.h
 * sid_slots_process_float) and converts it once, through the soft limiter (core/fx/fx_limit.h):
 * at a gain of 1 it gives what the slots alone give, bit for bit, so the stage starts and ends
 * without a step. The gains move linearly over the block a change lands in (and the slots'
 * dry gain from the last block's to this one's), so SPREAD and a slot's Mix never zipper.
 *
 * Portable C with no hardware access; chip_spread_side, chip_spread_apply and
 * chip_spread_glide_step run in the audio interrupt (platform/fm1/app.ld puts them in SRAM). */
#ifndef SIDOU_CHIP_SPREAD_H
#define SIDOU_CHIP_SPREAD_H

#include <stdint.h>

#include "chip_mix.h"
#include "live.h"
#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define CHIP_SPREAD_MAX 100u
/* The block the stage runs on: the audio block (platform/fm1/audio.h AUDIO_HALF_FRAMES). */
#define CHIP_SPREAD_BLOCK CHIP_MIX_FADE

/* SPREAD's gains (main loop): the mid's cos phi and the side's sin phi, for 0..CHIP_SPREAD_MAX
 * (more is CHIP_SPREAD_MAX). 0 gives exactly 1 and 0. */
void chip_spread_gains(uint32_t spread, float* mid, float* side);

/* The side of a block, halved to fit int16: side[i] = (a[i] - b[i] x the fade's weight) >> 1,
 * the weight chip_mix's for the same fade (chip_mix.h). Before chip_mix, which overwrites a. */
void chip_spread_side(int16_t* side, const int16_t* a, const int16_t* b, chip_mix_fade fade);

/* A block of CHIP_SPREAD_BLOCK interleaved stereo frames: out from the slots' float output in
 * (sid_slots_process_float) and the side, frame f at mid gain m and side gain k (side[f] in LSBs
 * times k), each moving from its first value to its second as f + 1 over the block, converted
 * through the limiter as the slots convert. */
void chip_spread_apply(int16_t* out, const float* in, const int16_t* side, float m0, float m1,
                       float k0, float k1);

/* The glide (above): 32 blocks of CHIP_SPREAD_BLOCK, 93 ms. */
#define CHIP_SPREAD_GLIDE 32u
/* A chip sounds while an envelope counter (0..255) is at CHIP_SPREAD_ON (-24 dB) or more; the
 * two stop sounding together once one has had all three under CHIP_SPREAD_OFF (-36 dB) for
 * CHIP_SPREAD_HOLD blocks (46 ms). Powers of two: one test on the counters' OR. */
#define CHIP_SPREAD_ON 16u
#define CHIP_SPREAD_OFF 4u
#define CHIP_SPREAD_HOLD 16u
_Static_assert((CHIP_SPREAD_ON & (CHIP_SPREAD_ON - 1u)) == 0u &&
                   (CHIP_SPREAD_OFF & (CHIP_SPREAD_OFF - 1u)) == 0u,
               "the thresholds are powers of two");

/* The glide's state (the audio interrupt's); zeroed: both chips quiet, centred. */
typedef struct {
    uint8_t held; /* blocks both chips stay sounding: CHIP_SPREAD_HOLD while both are loud */
    uint8_t g;    /* the glide, 0 (centred) to CHIP_SPREAD_GLIDE (SPREAD's separation) */
} chip_spread_glide;

/* One block's step: whether both chips sound, from each one's level, the OR of its three
 * envelope counters (chip_spread_level; 0 for chip B out of use: quiet), and the glide one block
 * towards CHIP_SPREAD_GLIDE while they do, else towards 0. Returns the glide the block ends on. */
uint32_t chip_spread_glide_step(chip_spread_glide* g, uint32_t a, uint32_t b);

/* A chip's level for chip_spread_glide_step: its envelope counters' OR. */
static inline uint32_t chip_spread_level(const sid* c) {
    return (uint32_t)c->env[0].counter | c->env[1].counter | c->env[2].counter;
}

/* SPREAD set (the menu, a CC or an NRPN; clamped to CHIP_SPREAD_MAX): saved with the settings. */
void chip_spread_set(live* l, uint32_t spread);

#ifdef __cplusplus
}
#endif

#endif
