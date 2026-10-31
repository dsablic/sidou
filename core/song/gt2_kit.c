// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "gt2_kit.h"

#include <string.h>

#define N(n) (0x80u + (n))
#define END 0xffu

#define DRUM(ad, wave, pulse, name) {{ad, 0x00, wave, pulse, 0, 0, 0, 2, 0x08}, name}

const gt2_kit_drum gt2_kit_instr[GT2_KIT_DRUMS] = {
    DRUM(0x19, 1, 0, "KICK"),
    DRUM(0x16, 9, 1, "SNARE"),
    DRUM(0x12, 15, 0, "CL HAT"),
    DRUM(0x18, 18, 0, "OP HAT"),
    DRUM(0x15, 21, 0, "CLAP"),
    DRUM(0x18, 28, 0, "TOM LO"),
    DRUM(0x17, 35, 0, "TOM HI"),
    DRUM(0x12, 42, 1, "RIM"),
};

#define SR 0xf6u

#define ROWS(...) __VA_ARGS__
const uint8_t gt2_kit_wave[2][GT2_KIT_WAVE_ROWS] = {
    {ROWS(0x11, 0x11, SR, 0x11, 0x11, 0x11, 0x11, END,
          0x41, 0x81, SR, 0x81, 0x81, END,
          0x81, SR, END,
          0x81, SR, END,
          0x81, SR, 0xe1, 0x81, 0xe1, 0x81, END,
          0x11, 0x11, SR, 0x11, 0x11, 0x11, END,
          0x11, 0x11, SR, 0x11, 0x11, 0x11, END,
          0x41, 0x81, SR, END)},
    {ROWS(N(48), N(29), 0x09, N(26), N(24), N(23), N(22), 0,
          N(50), N(90), 0x06, N(82), N(76), 0,
          N(95), 0x02, 0,
          N(92), 0x08, 0,
          N(84), 0x05, N(84), N(84), N(84), N(82), 0,
          N(45), N(42), 0x08, N(40), N(38), N(37), 0,
          N(52), N(49), 0x07, N(47), N(45), N(44), 0,
          N(70), N(88), 0x01, 0)},
};

const uint8_t gt2_kit_pulse[2][GT2_KIT_PULSE_ROWS] = {{0x88, END}, {0x00, 0}};

uint32_t gt2_kit_find(const gt2_song* s, uint32_t drum) {
    for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++)
        if (!memcmp(s->instr[i - 1u], gt2_kit_instr[drum].head, GT2_INS_NAME))
            return i;
    return 0u;
}
