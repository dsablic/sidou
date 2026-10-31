// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The SID drum kit's instruments and table rows (gt2_kit.h; decision record 0009 has how
 * each drum was tuned and measured).
 *
 * A wavetable row is a waveform (left) and an absolute note (right, $80 + the note, 0 C-0)
 * for one frame; $FF 00 stops the table, the last row's waveform and note held. A drum's
 * row 1 follows its first frame ($08: the test bit with the gate off, so the envelope starts
 * whole at row 1 and every drum starts one frame after its row).
 *
 * Two ADSR delay bugs (the envelope's rate counter run past the gate's attack period, so
 * the attack waits out its wrap, about 33 ms) made every drum sound 53 ms after its row, in
 * GoatTracker's own player and on reSIDfp alike (decision record 0009), and are designed
 * out here:
 * - In the gate-off first frame the counter runs at the release's period: with a release of
 *   the drum's own (up to 977 cycles) it ran past attack 0's 9. So each SR is $00 (the
 *   counter below 9 from the hard restart's $0F00 on) until the drum's SR row (the
 *   wavetable command 6XY, its frame holding the row before it) sets its release, at frame
 *   2 or 3: before a staccato key off at tempo 5 and up (the gate goes off the gate-off
 *   timer's two frames before the next row), so a staccato drum fades as a held one.
 * - At the gate the chip compares the counter with the decay's period for a cycle: one
 *   phase in nine it stepped past attack 0's 9. So the attack is 1 (32 cycles, 8 ms to the
 *   top), which a counter below 10 always reaches. */
#include "gt2_kit.h"

#include <string.h>

#define N(n) (0x80u + (n)) /* an absolute note in a wavetable row: 0 is C-0 */
#define END 0xffu          /* the jump: with 0, the table stops */

/* AD (attack 1), wave, pulse, name; SR $00 (its SR row sets it), no filter or vibrato, the
 * gate-off timer 2, the first wave $08. */
#define DRUM(ad, wave, pulse, name) {{ad, 0x00, wave, pulse, 0, 0, 0, 2, 0x08}, name}

const gt2_kit_drum gt2_kit_instr[GT2_KIT_DRUMS] = {
    /* a frame of triangle at C-4 (the click), then a triangle falling from F-2 (two frames,
     * the second its SR row's) to A#1 at frame 7, the body held at 58 Hz while the envelope
     * decays (750 ms) */
    DRUM(0x19, 1, 0, "KICK"),
    /* a pulse tone at D-4 for a frame, then noise falling in brightness (its brightest two
     * frames) */
    DRUM(0x16, 9, 1, "SNARE"),
    /* the brightest noise, 48 ms */
    DRUM(0x12, 15, 0, "CL HAT"),
    /* bright noise, 300 ms */
    DRUM(0x18, 18, 0, "OP HAT"),
    /* three bursts of noise, the first two frames (between them the gate held and no
     * waveform, the inaudible $E1, so the envelope runs on: a gate off and on would wait out
     * the ADSR delay bug), the last one's tail 168 ms */
    DRUM(0x15, 21, 0, "CLAP"),
    /* a triangle falling from A-3 to C#3 (220 to 139 Hz), its second note two frames */
    DRUM(0x18, 28, 0, "TOM LO"),
    /* a fifth above: E-4 to G#3 (330 to 208 Hz) */
    DRUM(0x17, 35, 0, "TOM HI"),
    /* a high pulse click and a frame of noise, 48 ms */
    DRUM(0x12, 42, 1, "RIM"),
};

/* The wavetable command 6XY (set sustain/release) as a row: $F6, the SR byte (sustain 0,
 * the release the drum's decay but RIM's). Its frame changes no waveform or note. */
#define SR 0xf6u

#define ROWS(...) __VA_ARGS__
const uint8_t gt2_kit_wave[2][GT2_KIT_WAVE_ROWS] = {
    {ROWS(0x11, 0x11, SR, 0x11, 0x11, 0x11, 0x11, END,       /*  1 KICK */
          0x41, 0x81, SR, 0x81, 0x81, END,                   /*  9 SNARE */
          0x81, SR, END,                                     /* 15 CL HAT */
          0x81, SR, END,                                     /* 18 OP HAT */
          0x81, SR, 0xe1, 0x81, 0xe1, 0x81, END,             /* 21 CLAP */
          0x11, 0x11, SR, 0x11, 0x11, 0x11, END,             /* 28 TOM LO */
          0x11, 0x11, SR, 0x11, 0x11, 0x11, END,             /* 35 TOM HI */
          0x41, 0x81, SR, END)},                             /* 42 RIM */
    {ROWS(N(48), N(29), 0x09, N(26), N(24), N(23), N(22), 0,
          N(50), N(90), 0x06, N(82), N(76), 0,
          N(95), 0x02, 0,
          N(92), 0x08, 0,
          N(84), 0x05, N(84), N(84), N(84), N(82), 0,
          N(45), N(42), 0x08, N(40), N(38), N(37), 0,
          N(52), N(49), 0x07, N(47), N(45), N(44), 0,
          N(70), N(88), 0x01, 0)},
};

/* Row 1: the pulse width $800 (a square), then the table stops. */
const uint8_t gt2_kit_pulse[2][GT2_KIT_PULSE_ROWS] = {{0x88, END}, {0x00, 0}};

uint32_t gt2_kit_find(const gt2_song* s, uint32_t drum) {
    for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++)
        if (!memcmp(s->instr[i - 1u], gt2_kit_instr[drum].head, GT2_INS_NAME))
            return i;
    return 0u;
}
