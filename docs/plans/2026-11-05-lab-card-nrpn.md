<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# An NRPN for every LAB card: design

**Status:** design for 1.7, 2026-11-05 (the owner's request, 2026-11-04).

## Why

A DAW (fm1-emu's plugin, a controller) should be able to automate every card LAB shows. The
cards of OSC, ENVELOPE, FILTER, MOD, FX and MACRO's links are instrument parameters with an NRPN
(and some a CC); FILTER ENV, LAYERS and TABLE (and 1.7's LFO page) have none.

## What it is

A new NRPN group, **MSB 7, LAB**: one NRPN a card, its value the card's own position (what its
bar shows), set as if the card were turned to it. The numbers are fixed by card, not by page, so
a page added later moves nothing:

| LSB | Card | Range |
|---|---|---|
| 0..4 | FILTER ENV: DEPTH (ALGORITHM), ATTACK, DECAY, SUS, RELEASE | 0..126 (= -63..+63), 0..15 each |
| 5..9 | LAYERS: the oscillator count (ALGORITHM), EDIT, the edited layer's TRANS, DETUNE, FILT | 0..2, 0..2, 0..48 (= -24..+24), 0..198 (= -99..+99), 0..1 |
| 10..14 | TABLE: the ending (ALGORITHM: 0 off, 1 once, 1 + n loop from n), STEP, the edited step's WAVE, NOTE, FRAMES | 0..17, 0..15, 0..12 (the card's list, 0 `END`), 0..48 (= -24..+24), 0..15 (= 1..16 frames) |

The cards that depend on another (EDIT past the layer count, a step past the table's end, `END`
only from `OFF`) clamp as the card does when turned. NRPN only: no CC is free in MIDI's
undefined blocks for 15 more, and MIDI learn binds any CC. MIDI out does not send them (its masks
are 64 parameters wide and full); they are not instrument parameters (`live_param`), so a program
change does not reset them beyond what loading the sound does. MACRO's K3 and K4 are unused cards:
nothing to map.

## Budgets (estimate)

Flash about 1 KB (the table of rows, the getters and setters); RAM none; RAMTEXT none.
