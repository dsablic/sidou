// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The SID drum kit's instruments and table rows (gt2_kit.h; decision record 0009 has how
 * each drum was tuned and measured).
 *
 * A wavetable row is a waveform (left) and an absolute note (right, $80 + the note, 0 C-0)
 * for one frame; $FF 00 stops the table, the last row's waveform and note held. A drum's
 * row 1 follows its first frame ($08: the test bit with the gate off, so the envelope starts
 * whole at row 1 and every drum starts one frame after its row). */
#include "gt2_kit.h"

#define N(n) (0x80u + (n)) /* an absolute note in a wavetable row: 0 is C-0 */
#define END 0xffu          /* the jump: with 0, the table stops */

/* AD, SR, wave, pulse, filter, vibrato, vibrato delay, gate-off timer, first wave, name. */
#define DRUM(ad, sr, wave, pulse, name) {{ad, sr, wave, pulse, 0, 0, 0, 2, 0x08}, name}

const gt2_kit_drum gt2_kit_instr[GT2_KIT_DRUMS] = {
    /* a pulse click at C-4, then a triangle falling from C-3 to A#1 in five frames, the
     * body held at 58 Hz while the envelope decays (750 ms) */
    DRUM(0x09, 0x09, 1, 1, "KICK"),
    /* a pulse tone at D-4 for a frame, then noise falling in brightness */
    DRUM(0x06, 0x06, 9, 1, "SNARE"),
    /* the brightest noise, 48 ms */
    DRUM(0x02, 0x02, 14, 0, "CL HAT"),
    /* bright noise, 300 ms */
    DRUM(0x08, 0x08, 16, 0, "OP HAT"),
    /* three bursts of noise a frame apart (between them the gate held and no waveform, the
     * inaudible $E1, so the envelope runs on: a gate off and on would wait out the ADSR
     * delay bug), the last one's tail 168 ms */
    DRUM(0x05, 0x05, 18, 0, "CLAP"),
    /* a triangle falling from A-3 to C#3 (220 to 139 Hz) */
    DRUM(0x08, 0x08, 24, 0, "TOM LO"),
    /* a fifth above: E-4 to G#3 (330 to 208 Hz) */
    DRUM(0x07, 0x07, 30, 0, "TOM HI"),
    /* a high pulse click and a frame of noise, 48 ms */
    DRUM(0x02, 0x01, 36, 1, "RIM"),
};

#define ROWS(...) __VA_ARGS__
const uint8_t gt2_kit_wave[2][GT2_KIT_WAVE_ROWS] = {
    {ROWS(0x41, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, END,  /*  1 KICK */
          0x41, 0x81, 0x81, 0x81, END,                    /*  9 SNARE */
          0x81, END,                                      /* 14 CL HAT */
          0x81, END,                                      /* 16 OP HAT */
          0x81, 0xe1, 0x81, 0xe1, 0x81, END,              /* 18 CLAP */
          0x11, 0x11, 0x11, 0x11, 0x11, END,              /* 24 TOM LO */
          0x11, 0x11, 0x11, 0x11, 0x11, END,              /* 30 TOM HI */
          0x41, 0x81, END)},                              /* 36 RIM */
    {ROWS(N(48), N(36), N(29), N(26), N(24), N(23), N(22), 0,
          N(50), N(90), N(82), N(76), 0,
          N(95), 0,
          N(92), 0,
          N(84), N(84), N(84), N(84), N(82), 0,
          N(45), N(42), N(40), N(38), N(37), 0,
          N(52), N(49), N(47), N(45), N(44), 0,
          N(70), N(88), 0)},
};

/* Row 1: the pulse width $800 (a square), then the table stops. */
const uint8_t gt2_kit_pulse[2][GT2_KIT_PULSE_ROWS] = {{0x88, END}, {0x00, 0}};
