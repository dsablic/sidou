<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# LAB's sounds on the store

LAB keeps a LIVE sound in one of sixteen USER slots, USER 1..16, and the sound playing as the
unsaved edit, so it comes back at power-on ([the plan](../plans/2026-10-25-lab-plan.md)). Both
hold a sound in one packed image, `app/lab_sound.h`; the slots are one file of the
[store](store-format.md), the unsaved edit one small value. Multibyte values are little-endian.

## A sound

45 bytes (`LAB_SOUND_BYTES`): the voice, the arpeggio and the three effect slots. Not the chip's
profile or character, the chip model, the envelope mode, the octave or the volume: those are
the instrument's ([settings-format.md](settings-format.md)).

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Control: the waveform (bits 4-7), ring (bit 2) and sync (bit 1) of `$d404`; never the gate or the test bit |
| 1 | 1 | Attack/decay, `$d405` |
| 2 | 1 | Sustain/release, `$d406` |
| 3 | 2 | Pulse width, `0..4095` |
| 5 | 2 | Cutoff, `0..2047` |
| 7 | 1 | Resonance, `0..15` |
| 8 | 1 | Filter routing, `$d417` bits 0-2 |
| 9 | 1 | Filter mode, `$d418` bits 4-6, in place (`0x10` low-pass, `0x20` band-pass, `0x40` high-pass) |
| 10 | 1 | One-voice sync, `1` for SYNC LEAD's (voice 1 hard-synced to a silent voice 3) |
| 11 | 2 | The sync ratio's base in 1/256, `0..4095` (SYNC LEAD's 704 is 2.75) |
| 13 | 1 | The sync ratio's offset in semitones, signed, `-12..12` |
| 14 | 2 | PWM depth, pulse-width steps, `0..2048` |
| 16 | 2 | PWM rate, the LFO's phase step a 10 ms tick (65,536 a cycle), `0..8356` |
| 18 | 1 | Fine tune in cents, signed, `-50..50` |
| 19 | 1 | Bend range, semitones, `0..24` |
| 20 | 1 | Velocity target, `0..3` |
| 21 | 1 | Mod wheel target, `0..2` |
| 22 | 1 | Aftertouch target, `0..3` |
| 23 | 3 | Vibrato rate, depth and delay, `0..127` each |
| 26 | 4 | The chord arpeggio: on (`0..1`), rate (`0..8`), order (`0..3`), span (`1..2`) |
| 30 | 15 | The effect slots, Distortion, Modulation, FX: each its algorithm, Amount, A, B, Mix (`0..127`) |

The values are the [MIDI map's](midi.md) where a parameter has one. A sound with a field out of
its range, the gate or the test bit set, or an algorithm its slot does not have is not a sound:
a reader treats it as an empty slot.

## The USER sounds

A store file named `USER SOUNDS`, type `4`, 931 bytes. It is the firmware's own: no list shows
it, and SAVE AS and SysEx refuse its name.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Version, `1` |
| 1 | 1 | The bytes of a sound that follow each name, `45` |
| 2 | 1 | Slots, `16` |
| 3 | 16 x 58 | Slot n (USER n + 1) at `3 + 58 n`: used (`1`; `0` empty), the name (12 bytes, the font's upper case `0x20..0x5F`, NUL-padded), the sound |

A keep rewrites the file whole: the slot kept, the others as the file had them. The store
commits a file whole, so a power cut leaves the old file or the new one. A later version
appends to each sound (the stride is `1 + 12 +` the header's sound bytes) and may hold more
slots: this firmware reads the first 45 bytes of each of its 16, and a keep writes its own
version, dropping what a later one appended. A file of version `0`, with fewer than 45 sound
bytes or 16 slots, or of another type, is no USER sounds (every slot empty); a slot whose used
byte is not `1`, whose name has a character out of the range or whose sound is not one is empty.

## The unsaved edit

A store value, key `0x109`, type `4`, 61 bytes: the sound playing, written by the settings'
debounce before the [settings record](settings-format.md) and applied at boot after it.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Version, `1` |
| 1 | 1 | Preset, the settings record's byte: `0..4` a built-in or DICE, `5 + n` USER sound n + 1 |
| 2 | 1 | Edited: `1` while the sound differs from its base (the preset as loaded, or the slot as kept) |
| 3 | 12 | The base's name, NUL-padded |
| 15 | 1 | The bytes of the sound that follow, `45` |
| 16 | 45 | The sound |

At boot the settings record loads its preset (a USER sound's from the file: its base), then
the unsaved edit's sound is applied over it when the record's preset is the one loaded, so the
edited mark comes back with it. A record of another preset, of version `0`, shorter than its
sound bytes, or whose sound is not one, is not applied. A later version appends to the sound.
An older firmware never reads the key.
