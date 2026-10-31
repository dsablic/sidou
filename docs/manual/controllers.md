<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Shape sounds from a MIDI controller

SIDOU answers a fixed set of CCs on every channel, from the TRS jack or USB, and any knob's
parameter can also learn a CC of your choice. A controller with eight knobs is enough to shape a
sound without reaching for the FM-1's encoders. The full map, with every range and curve, is
[docs/reference/midi.md](../reference/midi.md); the CCs below are taken from it.

## An eight-knob map

Set your controller's knobs to these CCs (its own editor or learn mode does it once):

| Knob | CC | Parameter | On the FM-1 |
|---|---|---|---|
| 1 | 74 | Cutoff | K1 on LIVE and LAB's FILTER |
| 2 | 71 | Resonance | K2 on LIVE and FILTER |
| 3 | 73 | Attack | LAB's ENVELOPE K1 |
| 4 | 75 | Decay | ENVELOPE K2 |
| 5 | 79 | Sustain | ENVELOPE K3 |
| 6 | 72 | Release | ENVELOPE K4 |
| 7 | 1 | Pulse width | LIVE's K3, OSC's K1 |
| 8 | 31 | Filter LFO | FILTER's K4 |

A second bank, for movement and character:

| Knob | CC | Parameter |
|---|---|---|
| 1 | 76 | Vibrato rate |
| 2 | 77 | Vibrato depth |
| 3 | 78 | Vibrato delay |
| 4 | 28 | ARP rate |
| 5 | 86 | Age/heat |
| 6 | 87 | DAC sloppiness |
| 7 | 106 | Distortion Mix |
| 8 | 116 | FX Mix |

Switches and steps, if the controller has buttons or a second row: 20 waveform, 21 ring
modulation, 22 sync, 23 filter mode, 24 filter routing, 25 chip model, 27 the chord arpeggio
on or off, 29 its order, 30 its span. The effect slots answer 102 to 116 (each slot's
algorithm, Amount, A, B and Mix) and 117 the chip spread.

## Parameters with no CC of their own

PWM depth, PWM rate, fine tune and MACRO's two links have an NRPN (0/5, 0/6, 0/7, 0/10, 0/11)
and no CC. Give them one with MIDI learn: then a controller knob reaches them like the others.

## MIDI learn

1. Hold **SAVE** and turn the FM-1's knob for the parameter (for an effect slot's control,
   EDIT held too). The learn frame names the parameter.
2. Turn the controller's knob. The binding is made, and a learned CC wins over the map's own.
3. **Forget one:** hold SAVE and turn the knob, release SAVE, then press and release SAVE again.
4. **Clear all:** in the learn frame, hold SAVE and press HOME, release SAVE, then press and
   release it again to confirm (`MIDI LEARN CLEARED`).

Learned CCs are omni (any channel, either port), last across power cycles, and go out on MIDI
when the panel changes the parameter, so a DAW records them and plays them back. The rules in
full: [docs/reference/midi.md](../reference/midi.md), MIDI learn.

## Presets for a controller

There is no preset format shared across controller makers, so set the CCs in your controller's
own editor once and save it there. Print the tables above and keep them beside it.
