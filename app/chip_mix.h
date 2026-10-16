// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Chip B's output into chip A's, before the effect slots (platform/fm1/audio.c, M2b Task 3).
 *
 * Chip B renders only while it is in use (the keys play it, a UI sound, a song track): out of
 * use it adds nothing and chip A's output passes bit for bit. Going in or out of use happens
 * at a block boundary, and chip B's share fades linearly over that block, so the level a
 * silent chip has of its own (its voices' DC, the thump, its output filter settling) never
 * steps: sample i of a fade-in takes (i + 1) / CHIP_MIX_FADE of B, of a fade-out
 * (CHIP_MIX_FADE - 1 - i) / CHIP_MIX_FADE. Integers only, so the host mixes bit for bit as
 * the device does: b * w >> 7, an arithmetic shift (GCC and Clang on every target here).
 *
 * Headroom: the sum saturates to int16 (sat16(a + b)). Two chips at full scale would clip;
 * the SID's output peaks well under it (one chip's worst on the bench scenarios is measured
 * in sidou-tests' chip_mix.rs and recorded in decision record 0004).
 *
 * Portable C with no hardware access. chip_mix runs in the audio interrupt (platform/fm1/
 * app.ld puts it in SRAM). */
#ifndef SIDOU_CHIP_MIX_H
#define SIDOU_CHIP_MIX_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The fade's length: one audio block (platform/fm1/audio.h AUDIO_HALF_FRAMES). */
#define CHIP_MIX_FADE 128u

typedef enum {
    CHIP_MIX_FULL = 0, /* chip B in use all the block */
    CHIP_MIX_IN = 1,   /* going in use: B fades in over the block */
    CHIP_MIX_OUT = 2   /* going out of use: B fades out over the block */
} chip_mix_fade;

/* a[i] = sat16(a[i] + b[i] * the fade's weight), i < n (n <= CHIP_MIX_FADE for a fade). */
void chip_mix(int16_t* a, const int16_t* b, uint32_t n, chip_mix_fade fade);

#ifdef __cplusplus
}
#endif

#endif
