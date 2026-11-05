<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# LAB's sounds on the store

LAB keeps a LIVE sound in one of sixteen USER slots, USER 1..16, and the sound playing as the
unsaved edit, so it comes back at power-on ([the plan](https://github.com/dsablic/sidou/blob/v1.6.0/docs/plans/2026-10-25-lab-plan.md)). Both
hold a sound in one packed image, `app/lab_sound.h`; the slots are one file of the
[store](store-format.md), the unsaved edit one small value. Multibyte values are little-endian.

## A sound

109 bytes (`LAB_SOUND_BYTES`): the voice, the arpeggio, the three effect slots, the filter LFO,
the macro links, up to three oscillator layers, the filter envelope, the step table and two LFOs. Not the chip's profile or character, the chip model, the envelope mode, the
octave or the volume: those are the instrument's ([settings-format.md](settings-format.md)).

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
| 45 | 1 | The filter LFO's depth, K1's cutoff walk's detents each way, `0..32` (`0` off); since 1.1 |
| 46 | 1 | RESO LINK, signed, `-8..8` (`0` none): the resonance following the cutoff; since 1.1 |
| 47 | 1 | PWM LINK, signed, `-8..8` (`0` none): the PWM's depth following the width; since 1.1 |
| 48 | 1 | Extra layers, `0..2`: a note plays `1 +` this many oscillators on the chip's voices, one note at a time; `0` is one oscillator and three-voice poly, as before. Never with one-voice sync (offset 10); since 1.6 |
| 49 | 6 | Each layer's tuning, layers 1 to 3: transpose in semitones, signed, `-24..24`, then detune in cents, signed, `-99..99`; layer 1's applies to every voice of a one-layer sound; since 1.6 |
| 55 | 10 | Layers 2 and 3, 5 bytes each: control (as offset 0), attack/decay, sustain/release, pulse width `0..4095`; offsets 0 to 4 are layer 1's; since 1.6 |
| 65 | 1 | Filter envelope depth, signed, `-63..63` (`0` off): the cutoff moves by the depth times 32 at the envelope's peak; since 1.6 |
| 66 | 1 | Filter envelope attack/decay, as `$d405`'s nibbles, with the SID envelope's timing; since 1.6 |
| 67 | 1 | Filter envelope sustain/release, as `$d406`'s nibbles; since 1.6 |
| 68 | 1 | Step table: bit 7 on, bits 5-6 the layer it plays on (`0..2`; a one-layer sound plays it on every voice), bits 0-4 its steps (`0..16`; on needs at least 1); off keeps the steps; since 1.7 |
| 69 | 1 | Table loop: `0` plays once and holds the last step, `n` (`1..steps`) goes back to step n after the last; since 1.7 |
| 70 | 1 | Table rate: each step's frames of 50 Hz less one, `0..15`; since 1.7 |
| 71 | 32 | 16 steps of 2 bytes: the control (`$d404` without the gate: the waveform, test, ring and sync bits) and the note, GoatTracker's wave-table right byte (`$00..$5F` up, `$60..$7F` down from `-32`, `$80` the key, `$81..$DF` an absolute note, `$80 + n` is GoatTracker's note n); since 1.7 |
| 103 | 6 | Two LFOs of 3 bytes: the shape (bits 0-2: `0` off, `1` triangle, `2` saw up, `3` saw down, `4` square, `5` sample and hold) and the destination (bits 4-5: `0` pitch, `1` pulse width, `2` cutoff, `3` volume), the rate (the PWM HZ walk's detent, `0..31`), the depth (signed, `-63..63`); since 1.7 |

**At least 45 bytes; a missing tail is the defaults.** 1.0 wrote sounds of 45 bytes (offsets
0..44). A reader takes a sound of 45 bytes or more, and the bytes it does not have are `0`: the
filter LFO off, no links, the sound as 1.0 played it. Fewer than 45 bytes is no sound. Each later
byte is added so that its `0` is what the firmware before it did.

The values are the [MIDI map's](midi.md) where a parameter has one. A sound with a field out of
its range, the gate or the test bit set, or an algorithm its slot does not have is not a sound:
a reader treats it as an empty slot.

## The USER sounds

A store file named `USER SOUNDS`, type `4`, 1,955 bytes. It is the firmware's own: no list shows
it, and SAVE AS and SysEx refuse its name.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Version, `1` |
| 1 | 1 | The bytes of a sound that follow each name, `109` (1.0's files: `45`, 1.1 to 1.5's: `48`, 1.6's: `68`) |
| 2 | 1 | Slots, `16` |
| 3 | 16 x 122 | Slot n (USER n + 1) at `3 + 122 n`: used (`1`; `0` empty), the name (12 bytes, the font's upper case `0x20..0x5F`, NUL-padded), the sound |

A keep rewrites the file whole: the slot kept, the others as the file had them. The store
commits a file whole, so a power cut leaves the old file or the new one. The stride is
`1 + 12 +` the header's sound bytes. An older file (1.0's, 45; 1.1 to 1.5's, 48; 1.6's, 68) is read with each
sound's missing bytes at their defaults, and a keep writes this firmware's file (109), the other
slots with the defaults appended. A later version appends to each sound and may hold more slots:
this firmware reads the first 109 bytes of each of its 16, and a keep writes its own version, dropping what a
later one appended. A file of version `0`, with fewer than 45 sound bytes or 16 slots, or of
another type, is no USER sounds (every slot empty); a slot whose used byte is not `1`, whose name
has a character out of the range or whose sound is not one is empty.

## The unsaved edit

A store value, key `0x109`, type `4`, 125 bytes (1.0's: 61, 1.1 to 1.5's: 64, 1.6's: 84): the sound playing, written by the
settings' debounce before the [settings record](settings-format.md) and applied at boot after it.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Version, `1` |
| 1 | 1 | Preset, the settings record's byte: `0..4` a built-in or DICE, `5 + n` USER sound n + 1 |
| 2 | 1 | Edited: `1` while the sound differs from its base (the preset as loaded, or the slot as kept) |
| 3 | 12 | The base's name, NUL-padded |
| 15 | 1 | The bytes of the sound that follow, `109` (1.0's: `45`, 1.1 to 1.5's: `48`, 1.6's: `68`) |
| 16 | 109 | The sound |

At boot the settings record loads its preset (a USER sound's from the file: its base), then
the unsaved edit's sound is applied over it when the record's preset is the one loaded, so the
edited mark comes back with it. 1.0's record (45 sound bytes) applies with the missing bytes at
their defaults. A record of another preset, of version `0`, of fewer than 45 sound bytes,
shorter than its sound bytes, or whose sound is not one, is not applied. A later version appends
to the sound. 1.0 reads the first 45 bytes of a 1.1 record's sound: it plays the sound without
the filter LFO and the links. 1.1 to 1.5 read the first 48 bytes of a 1.6 sound: they play its
first layer without the filter envelope. 1.6 reads the first 68 bytes of a 1.7 sound: it plays the
sound without its table and LFOs. An older firmware never reads the key.
