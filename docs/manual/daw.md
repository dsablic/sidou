<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU from a DAW

SIDOU is a MIDI sound module as well as an instrument: a sequencer on a computer or an iPad can
play it over USB, the TRS jack or Bluetooth, follow or lead its clock, and record what it plays
over the same USB cable. This page sets a DAW up for each way of playing it. Every message, range
and default is in [the MIDI reference](../reference/midi.md).

## 1. Connect it

- **USB:** one cable carries everything. SIDOU shows up as a class-compliant MIDI device,
  **SIDOU FM-1**, and as a USB audio input, no driver needed.
- **TRS MIDI IN:** notes, CCs and clock in from a hardware sequencer or an interface. The jack is
  MIDI in only: nothing comes back that way.
- **Bluetooth:** SETTINGS, BLUETOOTH **ON**, and a Mac, an iPad or a phone finds
  **SIDOU XXXX** as a Bluetooth MIDI device, no pairing (on a Mac: Audio MIDI Setup, MIDI
  Studio, the Bluetooth button). Notes, CCs and clock in, the panel out, as over USB. It is off by
  default ([controls](../reference/controls.md#settings-and-store)).

All three ports are merged: a message counts the same from any of them.

## 2. Pick a mode

HOME, **MIDI**, MODE. N is the page's CHANNEL (1 at power-on); N + 1 and on wrap from 16 to 1.

| Mode | For | What plays |
|---|---|---|
| **OMNI** (power-on) | one keyboard, one track | every channel plays the instrument, three voices, on the chip the keys play |
| **LIVE** | one track among other synths | channel N plays the instrument; other channels are left to other gear |
| **MULTI** | a song in four tracks | N, N+1 and N+2 a monophonic part each on chip A, each with its own sound; N+3 the instrument, polyphonic, on chip B |
| **WHOLE B** | the instrument beside SIDOU's own song | channel N plays the instrument on chip B; chip A plays the song |
| **WHOLE A** | a polyphonic track, chords or an arpeggio on chip A | channel N plays one LAB sound on chip A's three voices (POLY, CHORD or ARP); N+3 the instrument on chip B |

A change of MODE or CHANNEL releases what MIDI was playing.

## 3. MULTI: four tracks

Make four MIDI tracks and send them to SIDOU on channels N to N + 3 (1 to 4 with CHANNEL at 1):

| Track | Channel | Plays |
|---|---|---|
| 1 | N | chip A's voice 1, a part: SAW LEAD at power-on |
| 2 | N + 1 | chip A's voice 2, a part: PWM BASS |
| 3 | N + 2 | chip A's voice 3, a part: NOISE PERC |
| 4 | N + 3 | the instrument on chip B, three voices, the keys' sound and effects |

The MIDI page shows them as **CH1** to **CH4**, each with its sound; turning ALGORITHM on a CH
row picks the sound there, and the big popup names it.

- **Each part is one voice:** a new note cuts the one before (the last note held sounds, and
  letting it go returns to the note still held under it). LEGATO **ON** on the page moves to the
  next note with no new attack, for slides. Write the parts as monophonic lines: bass, lead,
  drums.
- **Its sound by program change:** put a program change at the start of each track:

  | Program | Sound |
  |---|---|
  | 0, 1, 2, 3 | SAW LEAD, PWM BASS, SYNC LEAD, NOISE PERC |
  | 4 | the instrument's sound as it is then: a DICE roll or a sound you are shaping, copied |
  | 16 to 31 | USER 1 to 16, the sounds LAB keeps ([LAB](lab.md#4-keep-it-and-name-it)) |

  An empty USER slot, or another number, keeps the part's sound. A part plays its sound's first
  layer: waveform, envelope, pulse width and its sweep, tuning, vibrato, the table, and what
  velocity, the wheel and aftertouch move. A sound's other layers, its LFOs, filter envelope,
  arpeggio and effects stay with the instrument (track 4); hard sync and ring modulation need a
  second voice, so a part plays without them. Program changes on track 4 pick the instrument's
  sound: 0 to 3 and 16 to 31, as above.
- **Per track:** velocity (where the sound uses it), pitch bend (the sound's range, 2 semitones
  on the presets, until RPN 0 sets the channel's own), the mod wheel and aftertouch (both add
  vibrato on the factory sounds), CC 120 and 123 (release the part) and CC 121 (resets its
  bend, wheel and aftertouch).
- **One filter for chip A:** the SID has one filter a chip. Its type, cutoff and resonance are
  channel N's sound's; each part goes through it if its own sound routes its first voice. CC 74
  (cutoff) and CC 71 (resonance) on any of the three channels move it, until CC 121. Give track 1
  the sound whose filter you want; a dry sound on track 2 or 3 stays dry.
- **Chip A's chip:** CC 25 (model), 26 (envelope mode), 85 (profile) and 86, 87, 89, 90 (age,
  DAC, thump, bleed) on any of the three channels set chip A; the same CCs on track 4 set the
  instrument's chip.
- **With a SIDOU song:** PLAY still plays the song on chip A; a part's held note takes its voice
  from the song until it is released, and the filter stays the song's. A `.sid` tune owns chip
  A: the parts are silent until it stops.

## 4. WHOLE A: one sound, three voices

One track on channel N plays chip A's three voices with one sound, the one MULTI gives channel
N (SAW LEAD at power-on; MULTI's other two parts keep theirs). A fourth track on N + 3 plays the
instrument on chip B, as in MULTI. The MIDI page's **CH** row (CH1 with CHANNEL at 1) shows the
sound and picks it, and a program change on N does the same, with MULTI's numbers (0 to 3, 4,
16 to 31). **OPTION** says how the notes share the voices:

| OPTION | Plays |
|---|---|
| **POLY** (power-on) | up to three notes at once; a fourth takes the voice of the oldest |
| **CHORD** | each key plays a chord: the note, CHORD 2 and CHORD 3 semitones above it (+4 and +7, a major triad); LEGATO ON moves to the next chord with no new attack |
| **ARP** | the held notes in turn on voice 1, in the order pressed: a step every ARP RATE frames (3, 60 ms), or at CLOCK every 16th of the MIDI clock |

Velocity, pitch bend (the sound's range until RPN 0), the mod wheel and aftertouch move all
three voices as on a MULTI part, the filter is the sound's (CC 74 and 71 move it), and CC 120
and 123 release the voices. With a SIDOU song, held notes take their voices from it as in MULTI.

## 5. Clock and transport

The MIDI page's **CLOCK** (AUTO at power-on) makes SIDOU's song follow the DAW's MIDI clock:
start plays from the top, continue goes on (or from a song position), stop stops, and each row
lands 8.0 ms after its clock reaches the jack, a constant to put in the DAW's latency
compensation for SIDOU's MIDI port. **ROWS/BEAT** says how many of the song's rows make a beat (4,
a row a 16th). FOLLOW stops the song with the clock; OFF ignores clock and transport.

**CLOCK OUT ON** sends the song's tempo as MIDI clock over USB, with start, stop and continue,
while no clock comes in: the DAW can follow SIDOU instead. The parts and the instrument need no
clock: they play the notes as they come.

## 6. Record it

SIDOU is a USB audio input: select it as the DAW's input and record the line out, stereo at
44.1 kHz, both chips and the effects, over the same cable as the MIDI. Record the four MULTI
tracks in one pass, or a part at a time with the others muted in the DAW.

## 7. Bank select

CC 0 and 32 before a program change pick its bank on that channel, and the bank is kept until
they change: bank 0 (CC 0 = 0, CC 32 = 0) is the sounds above; bank 1 loads the store's songs by
number (on the instrument's channel or N); bank 2 picks the chip's profile. A DAW that sends a
bank with every program change should send bank 0 for sounds
([Program change and banks](../reference/midi.md#program-change-and-banks)).
