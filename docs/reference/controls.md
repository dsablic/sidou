<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Controls (M1)

How the live instrument is played from the FM-1's panel and over MIDI. Button names are
fm1-emu's and Felucca's ids (`platform/fm1/VENDORED.md`); the printed labels may differ,
see [hardware-test.md](../hardware-test.md). The effect-slot layer is **temporary**: the
CHIP view (M1c Task 4) replaces it.

## Panel

| Control (id) | Does |
|---|---|
| Note keys (14..40) | play F3..G5 at the current octave |
| OCT- / OCT+ (buttons 0, 1) | octave -1 / +1 (-3..+3); their LEDs show the direction |
| FX (button 2) | chip 6581 / 8580; lit for the 8580 |
| K1 (encoder 2) | cutoff |
| K2 (encoder 3) | resonance |
| K3 (encoder 4) | pulse width |
| K4 (encoder 5) | preset |
| MASTER (pot) | volume |
| **EDIT (button 6) held** + K1 / K2 / K3 / K4 | the selected effect slot's **Amount / A / B / Mix** (32 detents end to end); K1-K4 are the knobs above again once EDIT is let go. EDIT is lit while the selected slot has an algorithm |
| **SCL (button 3)** | selects the next effect slot: Distortion, Modulation, FX |
| **ENV (button 4)** | the selected slot's next algorithm (back to OFF after the last) |

The other buttons (LFO, GLO, HOME, SAVE, ARP, SEQ, PLAY, REC) and encoders do nothing yet.

## Effect slots

Distortion, Modulation and FX run after the chip, in that order, in stereo
(`core/fx/fx_slots.h`; what each algorithm does: [decision record 0004](../decisions/0004-effect-algorithms.md)).

| Slot | Algorithms (ENV order) | Amount | A | B |
|---|---|---|---|---|
| Distortion | OFF, TAPE, OVERDRIVE, FUZZ, CRUSH, LO-FI | drive (CRUSH: bit depth; LO-FI: TV tone) | OVERDRIVE tone, FUZZ octave, CRUSH rate, LO-FI hum | OVERDRIVE bias, FUZZ starved bias, LO-FI noise |
| Modulation | OFF, CHORUS, FLANGER, PHASER, TREMOLO, AUTO-PAN, WOW | depth (AUTO-PAN width; WOW wow) | rate (WOW: flutter) | CHORUS ensemble, FLANGER and PHASER feedback, TREMOLO and AUTO-PAN shape |
| FX | OFF, DELAY, PING-PONG, REVERB, LUSH | DELAY feedback, REVERB and LUSH decay | DELAY time, REVERB and LUSH damping | DELAY damping, REVERB diffusion, LUSH bandwidth |

Mix is dry/wet. Every control is smoothed, and changing algorithm crossfades through
the dry sound (slots take turns, so a full change takes up to ~150 ms). Everything is off
at power-on.

## MIDI

Notes on every channel; program change 0..3 selects the preset. Every parameter has a CC
and an NRPN: the map, generated from the code, is [midi.md](midi.md). The numbers M1a
and M1c set are kept: 1 pulse width, 7 volume, 71 resonance, 74 cutoff, and the effect
slots on 102..116 (Distortion 102..106, Modulation 107..111, FX 112..116: algorithm,
Amount, A, B, Mix). CC 120 and 123 release every note. The chip model (CC 25, NRPN 1/5)
changes at the next 10 ms tick, to the last value received, so a stream of changes resets
the chip at most 100 times a second; the FX button changes it at once.

An algorithm CC picks the algorithm in order (above) by dividing 0..127 evenly
among the slot's algorithms: for the FX slot's five, 0..25 OFF, 26..51 DELAY, 52..76
PING-PONG, 77..102 REVERB, 103..127 LUSH. A control CC (0..127) is the control's whole
range.

## Screen

The status screen shows the chip, preset, octave, cutoff, resonance, pulse width,
volume and the held notes, and under EFFECTS each slot's algorithm and Mix (`>` marks the
selected slot) with the selected slot's Amount, A and B.
