// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The MASTER pot's volume, a digital gain on the final stereo output (platform/fm1/audio.c,
 * M2 hardware fix): after the chips, the effect slots and chip B's mix, so it scales
 * everything the instrument plays (the keys, a song on either chip, UI sounds) and the
 * chips' own volume registers keep their musical meaning: a song's fades and its 6581 volume
 * digi trick play relative to MASTER, live's volume register is always 15.
 *
 * Q15 gain (MASTER_UNITY is 1.0): the pot's 0..LIVE_MASTER_MAX maps through an audio taper,
 * -60 dB to 0 dB in even steps of dB (a pot's travel feels even), so full is exactly unity and
 * the stage does nothing at all there (the output is the chips' bit for bit); 0 is silence. A change ramps linearly over the block it lands in, so the pot never
 * zippers. Integers only: the host mixes bit for bit as the device does. Portable C with no
 * hardware access; master_gain_apply runs in the audio interrupt (platform/fm1/app.ld puts
 * it in SRAM). */
#ifndef SIDOU_MASTER_GAIN_H
#define SIDOU_MASTER_GAIN_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MASTER_UNITY 32768u
/* The block it scales: the audio block (platform/fm1/audio.h AUDIO_HALF_FRAMES). */
#define MASTER_BLOCK_LOG2 7u
#define MASTER_BLOCK (1u << MASTER_BLOCK_LOG2)

/* The Q15 gain for a MASTER value, 0..max (live.h LIVE_MASTER_MAX, 1023; larger is max): 0 for 0,
 * unity for max, and between them a 64-entry table in dB, interpolated linearly in gain. */
uint32_t master_gain_of(uint32_t value, uint32_t max);

/* Scales 2^frames_log2 interleaved stereo frames (L, R) in place from gain `from` to gain `to`:
 * frame i by from + (to - from) * (i + 1) / 2^frames_log2, so the block ends on `to`. The
 * firmware's block is MASTER_BLOCK_LOG2; the tail gate (tail_gate.h) fades a mono block of 128
 * samples as 64 frames with one less. Nothing is touched when both gains are MASTER_UNITY; a
 * constant gain (from == to) is one multiply per sample. Rounds to nearest (half up), so 0 stays
 * 0: a silent tail stays exactly silent. At least 2 frames. */
void master_gain_apply(int16_t* lr, uint32_t frames_log2, uint32_t from, uint32_t to);

#ifdef __cplusplus
}
#endif

#endif
