<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Controls (M1)

How the live instrument is played from the FM-1's panel and over MIDI. Button names are
fm1-emu's and Felucca's ids (`platform/fm1/VENDORED.md`); the printed labels may differ,
see [hardware-test.md](../hardware-test.md). The effect-slot layer is **temporary**: the
CHIP view (M1c Task 4) replaces it.

## At power-on

The boot animation (the title card, about 1.2 s) plays over LIVE while the instrument
already plays. **Any button or encoder** skips it and does nothing else; **a
note** (from the keys or MIDI) skips it and plays. **HOME held while powering on** plays the
full boot sequence instead (about 10 s); letting HOME go does not skip it, and anything after
that does. Without HOME held, a HOME press on the title card skips it like any button.

## Routing

Each button press and encoder turn goes, in this order (`app/views.h`):

1. **a capture:** a view waiting for one control (M1d's MIDI learn, the boot animation) sees
   everything first, HOME included, so HOME can cancel it; it ends when its view closes;
2. **global:** HOME opens the menu from LIVE and goes back one view anywhere else; SAVE
   opens MIDI learn over any view;
3. **the view on top:** the menu takes SELECT and ALGORITHM, SCENE takes SELECT; LIVE, the
   status screen, the glyph sheet and About take nothing;
4. **the panel:** everything else plays the instrument as below, in every view.

The note keys and MASTER always play, whichever view shows. The effect-slot layer (EDIT,
SCL, ENV) is global until the CHIP view takes the slots over.

## Panel

| Control (id) | Does |
|---|---|
| Note keys (14..40) | play F3..G5 at the current octave, at velocity 127: they sound as velocity off does, whatever the preset's velocity target |
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
| **HOME (button 8)** | the menu, from LIVE; back one view from any other (the menu, the status screen, SCENE, the glyph sheet, About) |
| **SAVE (button 9)** | MIDI learn (below): press it, turn a knob, then send a CC |
| **SELECT (encoder 0)**, in the menu | moves the blinking cursor up and down the items |
| **ALGORITHM (encoder 1)**, in the menu | changes the item under the cursor: CHIP (clockwise the 8580, counter-clockwise the 6581; turning on past either changes nothing), PRESET, OCTAVE; turned clockwise on STATUS, opens the status screen, on SCENE the generative pieces, on CHARSET the glyph sheet, on ABOUT the credits |
| **SELECT (encoder 0)**, in SCENE | the next piece clockwise, the one before counter-clockwise (FIRE, PLASMA, RULE 30, STARS, ANT, and round): it wipes in, and a toast names it |

The other buttons (LFO, GLO, ARP, SEQ, PLAY, REC) and PRESETS (encoder 6) do
nothing yet; SELECT does nothing outside the menu and SCENE, ALGORITHM nothing outside the
menu.

## SCENE

SCENE (the menu's item after STATUS) is M1e's prototype of M2's visuals: a
generative piece on the whole screen that reacts to what the instrument plays
(`app/scene.h`). SELECT steps between the pieces; everything else plays as on
LIVE, so the keys and MIDI move the piece, and K1 (cutoff) and K2 (resonance) set
its mood. The piece starts again from its beginning each time it opens.

| Piece | Note-ons | Envelopes (ENV3) | Pitch | Cutoff, resonance |
|---|---|---|---|---|
| FIRE | flare that voice's flame (voice 1 the left third, 2 the middle, 3 the right) | heat its flame; silent, it dies down to the embers | higher notes burn higher | the cutoff how hot it looks (red, up to yellow and white near the bed); the resonance a little taller |
| PLASMA | drop a bright ring that grows from a place | speed it up (silence runs it at an eighth) and voice 3's warms it | the pitch class sets the ring's place across, the pitch its height | the cutoff its speed, the resonance its twist |
| RULE 30 | step the automaton a generation in the voice's colour; voice 3's flips a cell first | a held voice keeps it stepping | voice 3's pitch picks the cell it flips | a low cutoff darkens the colours |
| STARS | light the voice's third of the stars in its colour | speed the voice's stars | the latest note steers the centre across | the cutoff sets the cruising speed |
| ANT | walk the voice's ant three steps | a sounding voice keeps its ant walking; silent, it stops and greys | | a low cutoff darkens the trail |

A voice that makes no sound (SYNC LEAD's voices 2 and 3) counts as silent. The voices'
colours are their waveforms' (pulse cyan, saw yellow, triangle light green, noise light
grey), as on LIVE.

## Presets

SAW LEAD, PWM BASS, SYNC LEAD and NOISE PERC (K4; program change 0..3). SYNC LEAD plays
one note at a time: voice 3 runs silently at the note's pitch and voice 1, hard-synced to
it, 2.75 times higher, so voice 3 sets the pitch and the ratio the timbre.

**Known limit:** voice 1's frequency register tops out (0xFFFF, 3.85 kHz) from note 90
(F#6), and from lower notes bent or with vibrato upward (note 88 at a bend of +2
semitones, lower at wider bend ranges). Voice 1 then stays there: the pitch is still
right, but the ratio falls, and the timbre thins, toward the top.

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

Notes on every channel; program change 0..3 selects the preset. A program change loads
the preset's cutoff, resonance, pulse width and SID settings (waveform, ring, sync, the
envelope, filter routing and mode) and expression settings over any edits, even when it
selects the preset already playing; volume, octave, chip, envelope mode, effects and a bend
range set by RPN 0 are kept. Every parameter has an NRPN, and all but the bend range and the
expression targets a CC: the map, generated from the code, is [midi.md](midi.md). The
numbers M1a and M1c set are kept: 1 pulse width, 7 volume, 71 resonance, 74 cutoff, and
the effect slots on 102..116 (Distortion 102..106, Modulation 107..111, FX 112..116:
algorithm, Amount, A, B, Mix). CC 120 and 123 release every note. The chip model (CC 25,
NRPN 1/5) changes at the next 10 ms tick, to the last value received, so a stream of
changes resets the chip at most 100 times a second; the FX button changes it at once.

**Envelope: INSTANT / AUTHENTIC** (CC 26, NRPN 1/10; INSTANT at power-on). The real SID
has a quirk known as the ADSR delay bug. Each voice's envelope steps when a 15-bit rate
counter reaches the current rate's period, and only that match restarts the counter: a
gate change does not. If a note starts while the counter is already past the attack's
(shorter) period, the counter has to run all the way round first: up to 32767 chip
cycles, about 33 ms, before the attack's first step. The factory presets release slower
than they attack (SAW LEAD: release 6, attack 0), so with the chip's behaviour about 97%
of the notes you play start 33 ms late, and the rest at once. A note-off meets the same
delay when the release is faster than the decay before it: about 73% of note-offs on SAW
LEAD (decay 9, release 6), 80% on SYNC LEAD and 32% on PWM BASS hold the note at its
sustain level for up to 33 ms more, so a staccato note comes out 33 ms long at random.
That is the chip, and reSIDfp behaves the same way. C64 music works around it with a
"hard restart" (setting the rates to 0 a few frames before each note), which a player at
a keyboard cannot do.

- **INSTANT** (0, the default for the live instrument): each gate edge also restarts the
  rate counter, so a note's attack, and its release at the note-off, take their first
  step within one rate period and 3 chip cycles: 12 cycles at attack 0, 270 at release 6.
  The sound is otherwise the chip's: the same envelope, rates, exponential release curve
  and DACs. This is the departure from the chip, behind a setting, because a live
  instrument that is late on most notes, and on most note-offs, is hard to play.
- **AUTHENTIC** (1): the chip's behaviour, bit for bit as reSIDfp. Use it to hear or
  test the real timing.

A change counts from the next gate edge: a sounding note keeps the attack it had, and its
release follows the new setting. The setting
is the instrument's, not the preset's: program changes and the chip model keep it. Song
and `.sid` playback (M2, M3) will default to AUTHENTIC: their music was written for the
chip and handles the delay itself, with hard restarts, so its timing is part of how it
sounds. The measured latencies are in
[decision record 0005](../decisions/0005-midi-note-timing.md).

**MIDI out (USB only):** what you change on the panel goes out over USB on channel 1, so
a DAW can record it as automation: each knob, MASTER, FX and the effect-slot controls as
the parameter's CC (above), K4 as a program change. At most one message per parameter
every 10 ms, with its latest value. A change that came in over MIDI (TRS or USB) is never
sent back, so recording with the DAW's MIDI thru on does not loop, and TRS input is not
passed on to USB. The channel and 14-bit NRPNs for cutoff, pulse width and volume are
build options for now: [midi.md](midi.md), Output.

An algorithm CC picks the algorithm in order (above) by dividing 0..127 evenly
among the slot's algorithms: for the FX slot's five, 0..25 OFF, 26..51 DELAY, 52..76
PING-PONG, 77..102 REVERB, 103..127 LUSH. A control CC (0..127) is the control's whole
range.

## MIDI learn

Any knob's parameter can follow a CC of your choice, from a controller on TRS or USB:

1. **Press SAVE** (button 9) **and turn a knob.** A frame, MIDI LEARN, opens over the screen
   on the press and says TURN A KNOB; the knob turned names its parameter (CUTOFF
   &larr; SEND A CC) and does not move it. The knob is the parameter it turns on the panel:
   K1..K3 the cutoff, resonance and pulse width; with **EDIT held**, K1..K4 the selected
   effect slot's Amount, A, B and Mix (SCL first to pick the slot; it works while the frame
   shows). K4 alone is the preset, which has no CC (program changes select it). SAVE need
   not be held: the frame waits after it comes up, and holding it changes nothing.
2. **Send the CC** (move the controller's knob). The frame closes with LEARNED CC 21 &rarr;
   CUTOFF, the parameter jumps to the controller's value, and from then on follows it.

While the frame shows, the note keys, OCT-/OCT+, FX, SCL, ENV and EDIT work as usual.

- **Cancel:** HOME (it closes the frame and opens nothing), or 10 s without a step (each
  step restarts the time). A cancelled learn changes nothing.
- **Forget one:** SAVE, turn the knob, then SAVE again instead of sending a CC: the
  parameter's learned CC is forgotten (CUTOFF: CC 21 FORGOTTEN), and its own CC is its own
  again. The frame shows SAVE: FORGET CC 21 when there is one.
- **Clear all:** SAVE, SAVE (CLEAR ALL MAPPINGS?), SAVE: MIDI LEARN CLEARED. HOME instead of
  the third press keeps them.

What learning means for MIDI in and out ([midi.md](midi.md), MIDI learn): a learned CC
works on every channel and on both ports, like the map; the parameter's own CC keeps
working; if the learned CC was another parameter's, it now moves the learned one only (the
other keeps its NRPN); learning a parameter again moves it to the new CC, and a CC learned
for another parameter moves to it. The panel's changes to a learned parameter go out over
USB on the learned CC. Data entry and NRPN/RPN selection (6, 38, 96..101) and the channel
mode messages (120..127) cannot be learned.

The mappings are kept in RAM until power-off; saving them comes with M2's storage.

**Why SAVE.** The holds in use are EDIT (the effect-slot layer while it is held), OCT- at
power-on (the test-death builds) and HOME at power-on (the full boot); HOME's press is the
menu's. Of the buttons that do nothing yet, the ones a player hits while playing are
taken by the design for M2's performance features (REC, PLAY, SEQ and ARP for the
sequencer, LFO and GLO for the sound); SAVE is about keeping what you set, which a
mapping is, and M2 will store the mappings with the settings it saves. The press arms
learn, so SAVE need not be held; if M2 gives SAVE's plain press a job of its own, arming can
move to a knob turned while SAVE is held, a chord no other control uses, and a player who
holds SAVE while turning keeps the same gesture.
SAVE's id (9) follows the design's button order (FX SCL ENV LFO EDIT GLO HOME SAVE ...),
which matches the ids known so far (FX 2, SCL 3, ENV 4, EDIT 6, HOME 8); the hardware
checklist confirms it.

## Screen

**LIVE** is what the screen shows from power-on (the root view, `app/live_view.h`): a header
line with the chip, preset and octave; each voice in a box with its oscilloscope, its
envelope as a bar (ENV3), its note, waveform, pulse width and envelope stage (ATK, DEC, SUS,
REL); the filter's response at its mode, cutoff and resonance; VU meters for each voice and
for the left and right output after the effect slots; and the three effect slots with their
algorithm and Mix (the play mark at the one the panel edits). The scopes are flat at rest
and draw each sounding voice's waveform, still while a note holds, over two to four periods
(more for higher notes). The trace is the waveform's colour: pulse cyan, saw yellow,
triangle light green, noise light grey.

The **status screen** (STATUS in the menu) shows the numbers: the chip, preset, octave,
cutoff, resonance, pulse width, volume and the held notes, and under EFFECTS each slot's
algorithm and Mix (`>` marks the selected slot) with the selected slot's Amount, A and B.

The menu (HOME) lists CHIP, PRESET and OCTAVE with their values, then STATUS, SCENE (the
generative pieces, above), CHARSET (the glyph sheet: the font and the palette) and ABOUT. A view opens with a wipe from the top and
closes with one from the bottom. M2's STEP view will be the default then; for M1 LIVE is.

A change of octave, selected slot or algorithm shows a toast on the bottom line for
1.5 s ("OCTAVE +1", "SLOT MOD", "MOD CHORUS"), whatever made it: a button, the menu or
MIDI. A chip change shows "CHIP 8580" and sweeps raster colours down the border.
