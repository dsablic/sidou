<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Generated from app/midi_map.c by host/crates/sidou-tests/tests/app/midi_map.rs; do not edit. SIDOU_MIDI_DOC_UPDATE=1 regenerates it. -->
# MIDI: the CC and NRPN map

Every parameter of the live instrument has a CC (7-bit) and an NRPN (14-bit, full
resolution). The map is fixed: the same numbers on every channel, on TRS and USB. Notes
play on every channel (omni); program change 0..3 selects the preset.

**How the numbers were chosen.** CCs that MIDI gives a meaning are kept for it:
General MIDI's where they fit (1 modulation wheel: pulse width; 7 volume; 71 resonance;
74 cutoff, i.e. brightness; 72, 73 and 75 release, attack and decay time) and left alone
where they do not (0 and 32 bank select, 6 and 38 data entry, 64..69 the pedals, 88
high-resolution velocity, 96..101 increment, decrement, NRPN and RPN, 120..127 channel
mode). 76..78 (vibrato) stay free for expression. The rest is in blocks MIDI leaves
undefined: the SID's patch in 20..25 and 79, the character layer in 85..87 and 89..90,
the effect slots in 102..116. NRPNs are grouped by MSB: 0 the performance controls, 1 the
SID, 2 the character layer, 3..5 the effect slots.

## Receiving values

- **CC, linear curve:** 0..127 spans the whole range, value * max / 127 rounded, so 0 and
  127 are the ends.
- **CC, steps curve:** 0..127 is divided evenly among the parameter's values,
  value * (max + 1) / 128: a switch is off below 64 and on from 64.
- **NRPN:** CC 99 and 98 select the parameter (MSB, LSB). The data is the parameter's own
  value, MSB * 128 + LSB, clamped to its range: CC 6 sets the MSB (the LSB taken as 0),
  CC 38 the LSB (with the MSB of the last CC 6, or, if none came since the selection or
  the last step, the current value's). CC 96 and 97 step the value by one (their data byte
  is ignored).
- **NRPN, parameters up to 0..127:** the value is all in the LSB, with an MSB of 0. CC 6
  only stores the MSB and CC 38 applies the value, so a sender's 6 = 0, 38 = v goes
  straight to v and never through 0 (which would reset the chip for the chip model, or
  silence the waveform). A controller that sends CC 6 alone ("7-bit NRPN") changes
  nothing here: use CC 38, or the parameter's plain CC.
- **Null and RPN:** NRPN 127/127 and RPN 127/127 select nothing. An RPN (CC 101, 100)
  takes data entry away from the NRPN until CC 99 or 98 comes again; no RPN is
  implemented yet. CC 121 (reset all controllers) and a gap in the input select nothing.
- The selection is kept per channel and per input (TRS, USB).
- CC 120 and 123 release every note.

## The map

In NRPN order.

| Group | Parameter | CC | NRPN (MSB/LSB) | Range | Curve | Default | Values |
|---|---|---|---|---|---|---|---|
| Live | Cutoff | 74 | 0/0 (0) | 0..2047 | linear | 1200 | 0..2047, the SID's 11 bits |
| Live | Resonance | 71 | 0/1 (1) | 0..15 | steps | 6 | 0..15 |
| Live | Pulse width | 1 | 0/2 (2) | 0..4095 | linear | 2048 | 0..4095, the SID's 12 bits; the PWM presets move around it |
| Live | Volume | 7 | 0/3 (3) | 0..1023 | linear | 1023 | 0..1023 as the MASTER pot; the chip's volume is its top 4 bits |
| SID | Waveform | 20 | 1/0 (128) | 0..15 | steps | 2 | sum of 1 triangle, 2 saw, 4 pulse, 8 noise; 0 silent |
| SID | Ring modulation | 21 | 1/1 (129) | 0..1 | steps | 0 | 0 off, 1 on (by the voice before) |
| SID | Sync | 22 | 1/2 (130) | 0..1 | steps | 0 | 0 off, 1 on (to the voice before) |
| SID | Filter mode | 23 | 1/3 (131) | 0..7 | steps | 1 | sum of 1 low-pass, 2 band-pass, 4 high-pass |
| SID | Filter routing | 24 | 1/4 (132) | 0..7 | steps | 7 | sum of 1 voice 1, 2 voice 2, 4 voice 3 |
| SID | Chip model | 25 | 1/5 (133) | 0..1 | steps | 0 | 0 6581, 1 8580 |
| SID | Attack | 73 | 1/6 (134) | 0..15 | steps | 0 | 0..15, the SID's rates |
| SID | Decay | 75 | 1/7 (135) | 0..15 | steps | 9 | 0..15, the SID's rates |
| SID | Sustain | 79 | 1/8 (136) | 0..15 | steps | 10 | 0..15, the level |
| SID | Release | 72 | 1/9 (137) | 0..15 | steps | 6 | 0..15, the SID's rates |
| Distortion | Algorithm | 102 | 3/0 (384) | 0..5 | steps | 0 | 0 OFF, 1 TAPE, 2 OVERDRIVE, 3 FUZZ, 4 CRUSH, 5 LO-FI |
| Distortion | Amount | 103 | 3/1 (385) | 0..127 | linear | 0 | 0..127 |
| Distortion | A | 104 | 3/2 (386) | 0..127 | linear | 0 | 0..127 |
| Distortion | B | 105 | 3/3 (387) | 0..127 | linear | 0 | 0..127 |
| Distortion | Mix | 106 | 3/4 (388) | 0..127 | linear | 0 | 0 dry .. 127 wet |
| Modulation | Algorithm | 107 | 4/0 (512) | 0..6 | steps | 0 | 0 OFF, 1 CHORUS, 2 FLANGER, 3 PHASER, 4 TREMOLO, 5 AUTO-PAN, 6 WOW |
| Modulation | Amount | 108 | 4/1 (513) | 0..127 | linear | 0 | 0..127 |
| Modulation | A | 109 | 4/2 (514) | 0..127 | linear | 0 | 0..127 |
| Modulation | B | 110 | 4/3 (515) | 0..127 | linear | 0 | 0..127 |
| Modulation | Mix | 111 | 4/4 (516) | 0..127 | linear | 0 | 0 dry .. 127 wet |
| FX | Algorithm | 112 | 5/0 (640) | 0..4 | steps | 0 | 0 OFF, 1 DELAY, 2 PING-PONG, 3 REVERB, 4 LUSH |
| FX | Amount | 113 | 5/1 (641) | 0..127 | linear | 0 | 0..127 |
| FX | A | 114 | 5/2 (642) | 0..127 | linear | 0 | 0..127 |
| FX | B | 115 | 5/3 (643) | 0..127 | linear | 0 | 0..127 |
| FX | Mix | 116 | 5/4 (644) | 0..127 | linear | 0 | 0 dry .. 127 wet |

## Reserved: not yet implemented

These numbers are kept for the character layer (M1c Task 3) and do nothing yet; a CC or NRPN sent to them is ignored.

| Group | Parameter | CC | NRPN (MSB/LSB) | Range | Values |
|---|---|---|---|---|---|
| Character | Profile | 85 | 2/0 (256) | 0..4 | 6581 R2, 6581 R3, 6581 R4AR, 8580 R5, worn 6581 |
| Character | Age/heat | 86 | 2/1 (257) | 0..127 | 0..127 |
| Character | DAC sloppiness | 87 | 2/2 (258) | 0..127 | 0..127 |
| Character | Output thump | 89 | 2/3 (259) | 0..127 | 0..127 |
| Character | Filter bleed | 90 | 2/4 (260) | 0..127 | 0..127 |

## By CC

| CC | Parameter |
|---|---|
| 1 | Live Pulse width |
| 7 | Live Volume |
| 20 | SID Waveform |
| 21 | SID Ring modulation |
| 22 | SID Sync |
| 23 | SID Filter mode |
| 24 | SID Filter routing |
| 25 | SID Chip model |
| 71 | Live Resonance |
| 72 | SID Release |
| 73 | SID Attack |
| 74 | Live Cutoff |
| 75 | SID Decay |
| 79 | SID Sustain |
| 85 | Character Profile (reserved) |
| 86 | Character Age/heat (reserved) |
| 87 | Character DAC sloppiness (reserved) |
| 89 | Character Output thump (reserved) |
| 90 | Character Filter bleed (reserved) |
| 102 | Distortion Algorithm |
| 103 | Distortion Amount |
| 104 | Distortion A |
| 105 | Distortion B |
| 106 | Distortion Mix |
| 107 | Modulation Algorithm |
| 108 | Modulation Amount |
| 109 | Modulation A |
| 110 | Modulation B |
| 111 | Modulation Mix |
| 112 | FX Algorithm |
| 113 | FX Amount |
| 114 | FX A |
| 115 | FX B |
| 116 | FX Mix |
