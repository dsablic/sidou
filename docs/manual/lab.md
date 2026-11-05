<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Shape a sound in LAB, and keep it

LAB is where you shape one sound with every parameter in sight, and keep it with one press.
It is made for experimenting: turn something, hear it, compare, undo, roll the dice, keep what
you like. LAB came in 1.0.

## 1. Open it

- On LIVE, press **ENV**, **LFO** or **FX**. LAB opens at the page of that button: ENVELOPE,
  MOD or FX. (Before LAB, ENV on LIVE toggled the chip model and FX stepped the next
  algorithm; they are LAB's now. The chip model is ALGORITHM on LIVE's TONE page.)
- Or HOME, then LAB in the menu (after SONG). It opens at OSC.

**SELECT** steps the ten pages (OSC, ENVELOPE, FILTER, FILTER ENV, MOD, LFO, FX, MACRO, LAYERS, TABLE), and ENV, LFO and FX jump to
theirs (LFO again on MOD goes to the LFO page, and back). **HOME** goes back to LIVE with the sound as you left it. Outside LIVE and LAB, ENV, LFO
and FX do nothing.

## 2. Turn the cards

Each page has four cards in the knobs' places: K1 to K4, each with its name, value and a bar. The
drawing above them shows what they do (the pulse's width, the envelope's shape, the filter's
curve, the modulation, the effect slot). **ALGORITHM** is the page's choice: the waveform on OSC,
the envelope mode on ENVELOPE (that one is for every sound, and says ALL SOUNDS), the filter mode
on FILTER, the arpeggio on MOD, the slot's algorithm on FX. A card turned lights for a second.
The keys play all the while, so you hear each turn.

The full table of cards is in [controls.md](../reference/controls.md#lab).

## 3. Compare, undo, vary, roll

| Do | Gets you |
|---|---|
| tap **EDIT** | A/B: the sound as kept or loaded (A), then back to your edit (B) |
| hold **EDIT**, play keys | peek: A for as long as EDIT is held (the header's `A` outlined), your edit back when you let go |
| **EDIT + LFO** | undo, up to 16 steps back (`UNDO (3)`); either finger may land first |
| **EDIT + ARP** | vary the page a little, never silent, never the same twice; either finger may land first |
| **EDIT + SCL** | roll the whole voice, as DICE does, also never silent and undoable; either finger may land first (SCL alone selects the next effect slot when it is let go) |
| **EDIT + PRESETS** | back to the sound as kept or loaded |

A result is held to what a C4 would give on the chip: if none of the tries is audible the sound
stays and the toast says `NOTHING AUDIBLE`.

To bypass an effect, turn its algorithm to **OFF** on the FX page. (This is what ENV's cycle
through OFF did on LIVE.)

## 4. Keep it, and name it

**SAVE** keeps the sound as one of sixteen **USER sounds**. A sound that is already a USER sound
is kept where it is; any other goes to the lowest free slot as `LAB 01`, `LAB 02`, ... and the
toast says where (`KEPT AS U03 LAB 03`). With all sixteen full, SAVE toasts `USER FULL:
EDIT+SAVE` and writes nothing: free a slot, or overwrite one with the name page.

To name it, hold **EDIT** and press **SAVE**. The note keys type A to Z and a space, SELECT moves
the cursor, ALGORITHM changes the character, K1 chooses the slot, and SAVE writes it (a used slot
asks once more). HOME leaves without writing.

A USER sound is a preset: PRESETS goes to it after DICE (`U01` to `U16`, empty slots skipped),
it is shown with its name, and it survives switching the FM-1 off. A MIDI program change knows
only the built-in presets.

## 5. Learn a knob

Hold **SAVE** and turn a card: the next MIDI controller you move is learned for that card's
parameter, on every page. (OSC's ratio on a sync sound has no CC and says so.)

## Knobs that turn by ear

The cards whose sound is heard as a ratio turn by ear, not by number: each detent of PWM HZ and
VIB HZ is the same step in pitch (PWM HZ from 0.05 to 12.75 Hz in 32 detents, shown in
hundredths below 1 Hz: `.05`, `0.12`), VIB's DEPTH and DELAY grow by the same ratio each detent
after 0, PWM's depth on a square law (fine near off), and each effect control on the FX page by
its own curve (a delay's feedback and every Mix by even steps of dB, the rates and tones even in
pitch). A sound saved before keeps its values exactly; the first detent takes a value the curve
does not hold to the next one the way you turn. A MIDI controller learned for a card moves along
the same curve. [The controls reference](../reference/controls.md#knob-curves) lists every curve.

## Sweep the filter

FILTER's K4 is the **filter LFO**: how far the cutoff swings each way, in K1's detents (`0` is
off, `32` the most). It swings at the PWM's rate, so OSC's PWM HZ sets its speed too; the card's
name line shows the rate (`LFO1.3HZ`). On a sound with no rate yet the first detent starts it at
1 Hz, so you hear it at once. The drawing dots the band the cutoff sweeps through. CC 31 sets it
from MIDI.

## The arpeggio's speed

MOD's K4 is the arpeggio's **RATE**, as ARP + K1 on LIVE: a note every few frames (`3FR`), or
in time with a MIDI clock (`1/32`, `1/16`, `1/8`). It is dark while ARP is off, and you can set
it all the same, ready for when you turn ARP on with ALGORITHM. (1.0 had the velocity target
here; it is NRPN 6/1 now.)

## Link the knobs: MACRO

The seventh page, **MACRO**, links what one knob does to another, for this sound:

- **K1, RES LNK** (`-8` to `+8`): the resonance follows the cutoff. Positive, it rises as you close
  the filter (the classic sweep); negative, as you open it (a squelch).
- **K2, PWM LNK** (`-8` to `+8`): the PWM's depth follows the pulse width: positive, the further the
  width from a square, the more it moves.

The two graphs show each link and where the sound is on it now. The links act wherever the cutoff
or the width moves: LIVE's knobs, LAB's cards, CC 74, the mod wheel, the filter LFO. Your RESO and
PWM settings stay as you set them; the cards show what the chip has, and light when K1 moves them.
K3 and K4 have no link yet.

## Send it to the song, and pull one back

**REC** sends the sound to the song as a GoatTracker instrument, into the lowest free one, while
the song plays on: the toast says which (`SENT AS I06 SAW LEAD`), and the instrument takes the
sound's name. SEQ then finds it on INSTR, through STEP or not. It is a plain GoatTracker 2
instrument with its wave, pulse, filter and speed rows, so the song still opens in GoatTracker.
**EDIT + REC** writes it over the instrument INSTR shows instead, asking first if that one is in
use (`REPLACE I05? EDIT+REC`, then EDIT + REC again).

The waveform, ring, sync, the envelope and the pulse width arrive exactly (GoatTracker's hard
restart added, and an attack of 0 becomes 1). The PWM, the filter LFO and the vibrato become
GoatTracker loops and speeds, close but stepped; the filter takes over the song's filter on the
voice TRACKER shows; with ARP on, TABLE opens with its ARP helper on a default chord for you to
finish. One-voice sync, the fine tune, the velocity, wheel and pressure targets and the effect
slots stay behind. For six seconds the hint line says what was approximated or left behind
(`APPROX: PWM FILTER`, `NOT SENT: FX TUNE`).

On **INSTR**, **ENV**, **LFO** or **FX** pull the instrument under the cursor into LAB, at that
page. A sound you sent and nobody edited comes back exactly (`PULLED I06`), its effect slots and
tune too; any other instrument is read as near as LAB can play it, and the toast names what it
could only approximate (`I01 ~DRUM`). The sound takes the instrument's name (its first 12
characters), so sending it back keeps the name. HOME goes back to INSTR, and from there to where
you opened LAB. The pull is one undo step: EDIT + LFO gives your sound back. The table of what
goes how is in [controls.md](../reference/controls.md#send-a-lab-sound-to-the-song).

A voice of a `.sid` tune comes into LAB the same way: REC in NOW PLAYING, then the voice's key
([lift a sound](sid-tunes.md#5-lift-a-sound)). LAB opens with it as **LIFTED V2**; SAVE keeps
it as a USER sound and HOME goes back to the tune.

## Sweep the filter on every note: FILTER ENV

**FILTER ENV**, after FILTER, moves the cutoff with an envelope of its own each time a note
starts. ALGORITHM is its **DEPTH**, `-63` to `+63` (`ENV OFF` at `0`): how far the cutoff goes at
the envelope's peak, up for a positive depth and down for a negative one. K1 to K4 are its
**ATTACK**, **DECAY**, **SUS** and **RELEASE**, with the SID envelope's own timing, so the drawing
reads as ENVELOPE's does; a dot rides the curve while the note plays.

## Play three oscillators on one key: LAYERS

**LAYERS**, the last page, makes a note play up to three oscillators at once, one on each of the
chip's voices, as a SidStation's single mode does. ALGORITHM sets how many: **ONE OSC** (the
sound plays three-note chords, as before), **TWO OSC** or **THREE OSC** (one note at a time, on
two or three voices). A sound with SYNC LEAD's one-voice sync keeps one (`ONE OSC: SYNC`).

- **K1 EDIT** picks the layer OSC and ENVELOPE edit; their header then says `OSC L2` or
  `ENVELOPE L3`, and their cards and drawing are that layer's.
- **K2 TRANS** transposes the layer, `-24` to `+24` semitones; **K3 DETUNE** detunes it, `-99`
  to `+99` cents; **K4 FILT** routes it into the filter or not.

The page shows the three layers side by side, each with its waveform, transpose and detune, the
one you edit reversed and an unused one dark. A new layer starts as a copy of the first, so it is
heard at once. A layered sound is kept, compared, undone, varied and rolled as one sound; a roll
reshapes the first layer, as DICE does.


## Play a step table: TABLE

**TABLE**, the last page, gives the sound a step table, as a SidStation or a C64 tune does: each
step sets the waveform and the pitch for a few frames, so one key plays a noise attack into a
pulse, an octave arpeggio or a drum that drops in pitch. The table starts on each note-on and runs
on through the release.

- **ALGORITHM** is how it ends: `TABLE OFF`, `PLAY ONCE` (the last step holds) or `LOOP FROM 1`
  to `LOOP FROM n` (after the last step it goes back to that one). Turned on from off, step 1 is
  the sound's own waveform at the key's pitch, so nothing changes until you edit it. Turned off,
  the steps wait for the next time.
- **K1 STEP** picks the step the other cards edit, `1` to the last, then `NEW`: a WAVE or NOTE
  turn there adds a step, a copy of the one before (up to 16).
- **K2 WAVE** is the step's waveform: `TRI`, `SAW`, `PUL`, `NOI`, the mixes (`T+S`, `T+P`, `S+P`,
  `TSP`), ring and sync (`TRR`, `SAS`, `PUS`), or `OFF` (silent). Turned counter-clockwise past
  `OFF` it says `END`: the table ends before that step.
- **K3 NOTE** is the step's pitch from the key, `-24` to `+24` semitones; an imported step on a
  fixed note shows its name (`C-4`).
- **K4 FRAMES** is how long each step lasts, `1` to `16` frames of 50 Hz.

The drawing shows each step's pitch as a bar from the middle, the edited step in K1's colour, a dot
on the step the last note plays and `^LOOP` under the step the loop goes back to. On a
one-oscillator sound every note of a chord runs the table from its own note-on; on a layered sound
it plays on one layer, the one LAYERS' EDIT picked when you turned it on (the header says
`TABLE L2`). EDIT + SCL's roll rests the table of the layer it reshapes (turn ALGORITHM to bring it
back); EDIT + ARP on TABLE varies the steps, and on a table that is off starts a new one-step table.

REC sends the table as the instrument's GoatTracker wave program (with ARP on, or on a layer other
than the first, it stays behind: `NOT SENT: TABLE`), and ENV on INSTR reads a wave program back as
a table.

## Two free LFOs: the LFO page

**LFO**, after MOD (press **LFO** on MOD to get there, and again to go back), has two LFOs of
their own, free-running (they do not restart at a note-on):

- **ALGORITHM** is the edited LFO's shape: `LFO OFF`, `TRIANGLE`, `SAW UP`, `SAW DOWN`, `SQUARE`,
  `S&H` (a new random level each cycle). Turned on from off with no depth yet, it starts at about
  2 Hz and a quarter of the depth, so you hear it at once.
- **K1 EDIT** picks LFO `1` or `2` (the header says `LFO 2`).
- **K2 DEST** is where it goes: `PCH` (the pitch of every layer), `PW` (every layer's pulse width),
  `CUT` (the cutoff) or `VOL` (the chip's volume: a tremolo).
- **K3 RATE** runs from 0.05 to 12.75 Hz in 32 detents, even in pitch, as OSC's PWM HZ.
- **K4 DEPTH**, `-63` to `+63` (the sign turns it upside down): at 63 the pitch swings an octave
  each way, the width and the cutoff half their range, the volume all the way down.

The page draws both, one cycle each: the edited one bright, the other dim, with its destination
under it and a dot where it is now. The SID has no volume per voice, so `VOL` moves every layer at
once. A roll that finds nothing audible tries again with the LFOs resting.
