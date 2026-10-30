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

**SELECT** steps the six pages (OSC, ENVELOPE, FILTER, MOD, FX, MACRO), and ENV, LFO and FX jump to
theirs. **HOME** goes back to LIVE with the sound as you left it. Outside LIVE and LAB, ENV, LFO
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

The sixth page, **MACRO**, links what one knob does to another, for this sound:

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
the song plays on: the toast says which (`SENT AS I06 SAW LEAD`). It is a plain GoatTracker 2
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
could only approximate (`I01 ~DRUM`). The pull is one undo step: EDIT + LFO gives your sound
back. The table of what goes how is in [controls.md](../reference/controls.md#send-a-lab-sound-to-the-song).
