<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MIDI controller maps

SIDOU listens to fixed CCs (docs/reference/midi.md) and supports MIDI learn (SAVE held + a knob
on any view). This page offers ready-made controller maps for common 8-knob MIDI controllers,
designed for LAB's pages (OSC, ENVELOPE, FILTER, MOD, FX, MACRO) and LIVE, so you can shape
sounds on the go without touching the FM-1.

## 8-knob controller map for LAB

This map covers LAB's six pages with one knob left for MASTER. Assign these CCs to your
controller's eight knobs:

| Knob | CC | Parameter | Page | Notes |
|------|-----|-----------|------|-------|
| 1 | 74 | Cutoff | FILTER, LIVE | K1 everywhere |
| 2 | 71 | Resonance | FILTER, LIVE | K2 everywhere |
| 3 | 72 | Release | ENVELOPE | K4 on ENVELOPE |
| 4 | 7 | Volume | - | MASTER on the panel |
| 5 | 1 | Pulse width | OSC, LIVE | Modulation wheel (K1 on OSC) |
| 6 | 26 | PWM depth | OSC | K2 on OSC |
| 7 | 77 | Vibrato depth | MOD | K2 on MOD |
| 8 | 76 | Vibrato rate | MOD | K1 on MOD |

### Detailed breakdown by page

**LIVE (the playing sound):**
- CC 74 (knob 1): Cutoff (K1)
- CC 71 (knob 2): Resonance (K2)
- CC 1 (knob 5): Pulse width (K3 on a pulse sound)
- CC 7 (knob 4): Volume (MASTER)

**LAB OSC:**
- CC 1 (knob 5): Pulse width (K1)
- CC 26 (knob 6): PWM depth (K2)
- CC 20 (or MIDI learn): PWM rate (K3)
- CC 79 (or MIDI learn): Fine tune (K4)

**LAB ENVELOPE:**
- CC 73 (or MIDI learn): Attack (K1)
- CC 75 (or MIDI learn): Decay (K2)
- CC 70 (or MIDI learn): Sustain (K3)
- CC 72 (knob 3): Release (K4)

**LAB FILTER:**
- CC 74 (knob 1): Cutoff (K1)
- CC 71 (knob 2): Resonance (K2)
- CC 79 (or MIDI learn): Voices routed (K3, 0=none, 1=voice 1, 7=all three)
- CC 31 (or MIDI learn): Filter LFO depth (K4)

**LAB MOD:**
- CC 76 (knob 8): Vibrato rate (K1)
- CC 77 (knob 7): Vibrato depth (K2)
- CC 78 (or MIDI learn): Vibrato delay (K3)
- CC 28 (or MIDI learn): Arpeggio rate (K4)

**LAB FX (for the selected effect slot):**
- Use MIDI learn (SAVE held + K1-K4 on the FX page) to bind the selected slot's controls
- Each slot's algorithm has different controls (see midi.md, Effect slots)

**LAB MACRO:**
- NRPN 0/11 (or MIDI learn): Resonance link (K1)
- NRPN 0/12 (or MIDI learn): PWM link (K2)

## Complete CC reference

See docs/reference/midi.md for the full CC and NRPN map (generated from the device's table).
Key CCs for live performance:

| CC | Parameter | Range | Notes |
|----|-----------|-------|-------|
| 1 | Modulation wheel → Pulse width | 0-127 | Expression target, changes with NRPN 6/2 |
| 7 | Volume (MASTER) | 0-127 | Audio taper, -60 dB to full |
| 20 | PWM rate | 0-127 | 0.05 to 12.75 Hz, walk curve |
| 25 | Chip model | 0-63 = 6581, 64-127 = 8580 | Resets chip |
| 26 | PWM depth | 0-127 | Walk curve |
| 27 | Arpeggio on/off | 0-63 = off, 64-127 = on | - |
| 28 | Arpeggio rate | 0-127 | 1F to /8, walk curve |
| 29 | Arpeggio order | 0-31 = UP, 32-63 = DN, 64-95 = UD, 96-127 = AS | - |
| 30 | Arpeggio span | 0-63 = 1 octave, 64-127 = 2 octaves | - |
| 31 | Filter LFO depth | 0-127 | 0 = off, then ±K1's detents |
| 70 | Sustain level | 0-15 | 4-bit envelope parameter |
| 71 | Resonance | 0-15 | 4-bit filter parameter |
| 72 | Release | 0-15 | 4-bit envelope parameter |
| 73 | Attack | 0-15 | 4-bit envelope parameter |
| 74 | Cutoff | 0-127 | K1's walk, 64 detents |
| 75 | Decay | 0-15 | 4-bit envelope parameter |
| 76 | Vibrato rate | 0-127 | 0.5 to 12.7 Hz, walk curve |
| 77 | Vibrato depth | 0-127 | ±4.7 to ±99 cents, walk curve |
| 78 | Vibrato delay | 0-127 | 0 to 1.27 s, walk curve |
| 79 | Fine tune | 0-127 | ±50 cents, 64 = 0 |
| 85 | Profile | 0-4 = built-in, 5-12 = USER 1-8 | Resets chip |
| 86 | Age/heat | 0-127 | Character macro |
| 87 | DAC sloppiness | 0-127 | Character macro |
| 89 | Output thump | 0-127 | Character macro |
| 90 | Filter bleed | 0-127 | Character macro |
| 102-116 | Effect slots | - | See midi.md, Effect slots |
| 117 | CHIP SPREAD | 0-127 | 0-100%, stereo separation |

## Using MIDI learn

1. Press and hold SAVE
2. Turn a knob (K1-K4 on any view, MASTER)
3. SIDOU listens for the next CC
4. Turn your controller's knob
5. SIDOU binds that CC to the parameter
6. A toast confirms: `LEARNED CC 21 -> PWM DEPTH`

To clear all learned CCs: SAVE held + HOME (a toast asks `CLEAR ALL?`; SAVE again clears).

The learned map is saved with the settings and persists across power-off.

## Generic controller setup

If your controller can import a preset file:
- Most controllers use their own format (Korg nanoKONTROL: .nktrl2_data, Arturia BeatStep: .beatstep)
- No generic MIDI controller preset format exists across brands
- Set up manually using the CC table above, or save your controller's preset once configured

For controllers without preset import:
1. Enter learn mode on your controller (consult its manual)
2. Assign each knob to the CC from the table above
3. Save the preset on your controller
4. On SIDOU, use the map as-is, or rebind with MIDI learn for a different layout

## Printable reference card

Print this page or the CC table above and keep it beside your controller. The 8-knob map is
designed to fit on one side of a card: knobs 1-8, the CC for each, and which LAB page uses it.
