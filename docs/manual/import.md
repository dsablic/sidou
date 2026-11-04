<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Import SidStation and MIDIbox SID patches

`sidou-sysex import` reads a bank of patches made for another SID synthesizer and turns it into
songs you send to the FM-1. Each patch becomes a LAB sound with its oscillators as layers, its
filter and its filter envelope, kept in an instrument you pull into LAB and keep as a USER sound.

It reads two formats:

- **Elektron SidStation** `.syx` banks: the factory bank and the bonus packs on Elektron's
  support site, and patches saved from a SidStation.
- **MIDIbox SID V2** `.syx` banks of Lead-engine patches. Bassline, Drum and Multi patches are
  refused with the reason; the rest of the bank still imports.

## 1. Make the songs

From `host/`:

```sh
cargo run --release -p sidou-sysex -- import SidStation_Presets_r1.syx --out syx
```

It writes `SIDSTN-1.syx` to `SIDSTN-6.syx` (sixteen patches a song; `MBSID-1.syx` and on for a
MIDIbox SID bank), the same songs as `.sng` files for GoatTracker, and `SIDSTN.txt`, the report.
`--8580` makes the sounds for the 8580; `--name` names the songs (up to 13 characters).

## 2. Send them and pull a sound into LAB

Send a `.syx` the way you send a song ([the SysEx reference](../reference/sysex.md)). On the
FM-1, load the song, go to **INSTR** (SEQ until it shows), turn PRESETS to the patch's
instrument (the report lists them: `I09 VELOBASS`), and press **ENV**: LAB opens on the sound,
layers and all (`PULLED I09`). **SAVE** keeps it as a USER sound.

A patch with a table (a SidStation oscillator's step table) keeps it on the LAB sound: the first
oscillator that plays a table brings it to its layer (LAB's [TABLE page](lab.md#play-a-step-table-table)),
up to 16 steps. The patch also gets one instrument per oscillator that plays its table as a
GoatTracker wave program, and a pattern that plays them together: PLAY auditions the song's table
patches one after another, every oscillator's table included. A LAB sound holds one table, so a
second oscillator's table plays only from the song; the report says so.

## What comes across, and what does not

| Comes across | Does not (the report names it) |
|---|---|
| Each oscillator's waveform, ring, sync, ADSR and pulse width, as a layer | A second or third oscillator's table: an instrument in the song (`OSC 2 TABLE: SONG INSTRUMENT`) |
| The first playing oscillator's step table, on its layer | Its steps past 16 (`OSC 1 TABLE CUT TO 16`); a speed slower than 16 frames a step (`OSC 1 TABLE SPEED CLAMPED`) |
| Transpose and detune | A fixed-note oscillator: it plays the key |
| The filter's mode, cutoff, resonance and routing | A transpose beyond two octaves or a detune beyond 99 cents: clamped |
| The first oscillator's vibrato, PWM and filter LFOs, the first two of them, as LAB's two LFOs (shape, speed, depth and inversion; read as best known) | A third of those (`FILTER LFO: DROPPED (TWO LFOS)`), the modulation matrix, the arpeggiator, portamento (MIDIbox SID: its modulation paths, wavetables and right SID) |

Two SidStation details are not documented and are read as best known: a table's fixed notes as
MIDI note numbers, and a table's speed as a divider of the patch's update rate. Both sound right
on the factory bank; a table that plays at the wrong pitch or speed is worth reporting.
