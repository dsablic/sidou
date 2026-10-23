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
 * With chip B out of use the side is chip A: A moves left.
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
 * Portable C with no hardware access; chip_spread_side and chip_spread_apply run in the audio
 * interrupt (platform/fm1/app.ld puts them in SRAM). */
#ifndef SIDOU_CHIP_SPREAD_H
#define SIDOU_CHIP_SPREAD_H

#include <stdint.h>

#include "chip_mix.h"
#include "live.h"

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

/* SPREAD set (the menu, a CC or an NRPN; clamped to CHIP_SPREAD_MAX): saved with the settings. */
void chip_spread_set(live* l, uint32_t spread);

#ifdef __cplusplus
}
#endif

#endif
