// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "audio.h"

#include "sid/sid.h"

static sid chip;

void audio_init(void) {
    sid_init(&chip, SID_MODEL_6581, SID_CLOCK_PAL, AUDIO_SAMPLE_RATE);
    /* The boot test tone: voice 1 sawtooth at A4, 0x1D45 * 985248 / 2^24 = 440.03 Hz,
     * instant attack, full sustain, filter off, full volume. */
    sid_write(&chip, 0x00, 0x45);
    sid_write(&chip, 0x01, 0x1D);
    sid_write(&chip, 0x05, 0x00);
    sid_write(&chip, 0x06, 0xF0);
    sid_write(&chip, 0x18, 0x0F);
    sid_write(&chip, 0x04, 0x21);
}

void audio_fill(int32_t* out) {
    int16_t s[AUDIO_HALF_FRAMES];
    sid_render(&chip, s, AUDIO_HALF_FRAMES);
    for (uint32_t i = 0; i < AUDIO_HALF_FRAMES; i++) {
        int32_t v = (int32_t)s[i] * 256;
        out[2 * i] = v;
        out[2 * i + 1] = v;
    }
}
