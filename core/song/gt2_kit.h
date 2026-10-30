// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The SID drum kit (decision record 0009): eight drums on the chip's own voices, as plain
 * GoatTracker 2 instruments and table rows, so a song that uses them opens and plays in
 * GoatTracker unchanged.
 *
 * The kit's table pointers assume its rows start at row 1 of the wavetable and of the pulse
 * table: a song that holds the kit holds its rows first (the NEW song, app/song_io.c). Every
 * drum's first-frame waveform is $08 (the test bit, the gate off), which resets the
 * oscillator and the noise register, so every hit is the same; its sound starts at the next
 * frame, from its wavetable's absolute notes, whatever the row's note. None uses the filter:
 * the song's filter stays the song's.
 *
 * Portable C11, constant data. */
#ifndef SIDOU_GT2_KIT_H
#define SIDOU_GT2_KIT_H

#include <stdint.h>

#include "gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

enum {
    GT2_KIT_KICK, GT2_KIT_SNARE, GT2_KIT_CLOSED_HAT, GT2_KIT_OPEN_HAT, GT2_KIT_CLAP,
    GT2_KIT_LOW_TOM, GT2_KIT_HIGH_TOM, GT2_KIT_RIM, GT2_KIT_DRUMS
};

/* A drum's instrument as the file holds it: AD, SR, the wave and pulse pointers into the
 * kit's rows, the gate-off timer, the first-frame waveform (head, GT2_INS_AD on), then the
 * name, zero-padded. 25 bytes, no padding. */
typedef struct {
    uint8_t head[GT2_INS_NAME];
    char name[GT2_INSTR_NAME];
} gt2_kit_drum;
_Static_assert(sizeof(gt2_kit_drum) == GT2_INSTR_BYTES, "a kit drum is an instrument's bytes");
/* The drums, in GT2_KIT_* order. */
extern const gt2_kit_drum gt2_kit_instr[GT2_KIT_DRUMS];
/* The kit's wavetable and pulse table rows, from row 1: left bytes, then right bytes. */
#define GT2_KIT_WAVE_ROWS 45u
#define GT2_KIT_PULSE_ROWS 2u
extern const uint8_t gt2_kit_wave[2][GT2_KIT_WAVE_ROWS];
extern const uint8_t gt2_kit_pulse[2][GT2_KIT_PULSE_ROWS];

/* The first instrument (1-63) of song s whose bytes before the name are drum's (GT2_KIT_*),
 * the kit's drum as a NEW song holds it (its rows from row 1); 0: none. */
uint32_t gt2_kit_find(const gt2_song* s, uint32_t drum);

#ifdef __cplusplus
}
#endif

#endif
