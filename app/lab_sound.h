// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* A LIVE sound as one packed image (LAB, docs/plans/2026-10-25-lab-design.md "What a sound
 * holds"): the voice and the three effect slots, LAB_SOUND_BYTES bytes, little-endian, at the
 * offsets below. What a USER sound slot keeps on the store, what live keeps as the current
 * sound's base (live.base) and what LAB compares, undoes and rolls.
 *
 * Not in it: the chip's profile and character (they stay the instrument's, CHIP and USER 1-8),
 * the model, the envelope mode (instrument-wide), the octave and the volume.
 *
 * live_sound_get and live_sound_apply (live.h) read a sound from the instrument and set one
 * on it. Portable C with no hardware access. */
#ifndef SIDOU_LAB_SOUND_H
#define SIDOU_LAB_SOUND_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define LAB_SOUND_BYTES 48u
/* 1.0's sounds (docs/reference/sound-format.md): a reader takes any sound of at least these
 * bytes, the bytes it does not have at their defaults (0). */
#define LAB_SOUND_BYTES_1_0 45u
/* A sound's name: up to 12 characters (with its terminator, 13 bytes in RAM). */
#define LAB_SOUND_NAME 12u

/* The offsets (a 16-bit field low byte first). */
#define LAB_SOUND_CONTROL 0u       /* waveform, ring and sync bits of $d404 (never the gate) */
#define LAB_SOUND_AD 1u            /* attack/decay, $d405 */
#define LAB_SOUND_SR 2u            /* sustain/release, $d406 */
#define LAB_SOUND_PULSE 3u         /* 2: the pulse width, 0..4095 */
#define LAB_SOUND_CUTOFF 5u        /* 2: the cutoff, 0..2047 */
#define LAB_SOUND_RESONANCE 7u     /* 0..15 */
#define LAB_SOUND_FILTER_VOICES 8u /* $d417 bits 0-2 */
#define LAB_SOUND_FILTER_MODE 9u   /* $d418 bits 4-6, in place */
#define LAB_SOUND_MONO_SYNC 10u    /* 1: SYNC LEAD's one-voice sync */
#define LAB_SOUND_SYNC_BASE 11u    /* 2: the sync ratio's base, in 1/256 */
#define LAB_SOUND_SYNC_SEMIS 13u   /* the sync ratio's offset in semitones, signed */
#define LAB_SOUND_PWM_DEPTH 14u    /* 2: the pulse-width LFO's depth, pulse-width steps */
#define LAB_SOUND_PWM_RATE 16u     /* 2: its phase step a tick (65536 a cycle) */
#define LAB_SOUND_TUNE 18u         /* the fine tune in cents, -50..+50, signed */
#define LAB_SOUND_BEND_RANGE 19u
#define LAB_SOUND_VELOCITY 20u /* the velocity target */
#define LAB_SOUND_MOD 21u      /* the mod wheel's target */
#define LAB_SOUND_PRESSURE 22u /* the pressure target */
#define LAB_SOUND_VIBRATO_RATE 23u
#define LAB_SOUND_VIBRATO_DEPTH 24u
#define LAB_SOUND_VIBRATO_DELAY 25u
#define LAB_SOUND_ARP 26u   /* 4: the arpeggio's on, rate, order and span (live_arp_field) */
#define LAB_SOUND_SLOTS 30u /* 15: slot s's field f at LAB_SOUND_SLOTS + 5 s + f */
/* 1.1's: the filter LFO's depth (0..LIVE_FILTER_LFO_MAX), the macro links (signed,
 * -LIVE_LINK_MAX..LIVE_LINK_MAX: the resonance's, the PWM's) */
#define LAB_SOUND_FILTER_LFO 45u
#define LAB_SOUND_RESO_LINK 46u
#define LAB_SOUND_PWM_LINK 47u

/* The sync ratio's base at most (16 times the pitch): the ratio's product stays in 32 bits. */
#define LAB_SOUND_SYNC_BASE_MAX 4095u

/* 1 when a and b are the same sound. */
uint32_t lab_sound_equal(const uint8_t a[LAB_SOUND_BYTES], const uint8_t b[LAB_SOUND_BYTES]);
/* 1 when every field of s is in its range (the gate and the test bit off, each parameter
 * within live_param_max, the arpeggio's within live_arp_set's, each slot's algorithm one its
 * slot has): a stored sound is checked before it is applied. */
uint32_t lab_sound_valid(const uint8_t s[LAB_SOUND_BYTES]);

#ifdef __cplusplus
}
#endif

#endif
