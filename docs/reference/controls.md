<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Controls

How the live instrument is played from the FM-1's panel and over MIDI. Button names are
fm1-emu's and Felucca's ids (`platform/fm1/VENDORED.md`); the printed labels may differ,
see [hardware-test.md](../hardware-test.md). The CHIP view (below) edits the chip's profile,
its character and the effect slots; the EDIT layer stays as a shortcut to the slots in every
view.

## At power-on

The full boot sequence (about 10 s) plays once when the saved firmware version is missing or
differs from the installed version. Once the current version has been saved, later boots
show only the title card (about 1.2 s). Both play over LIVE while the instrument already
plays. **Any button or encoder** skips the boot and does nothing else; **a note** (from the
keys or MIDI) skips it and plays. **HOME held while powering on** always plays the full
sequence; letting HOME go does not skip it, and anything after that does. Without HOME held,
a HOME press skips it like any button. Allow the settings save to finish before powering
off; a foreign store cannot remember the version ([Settings and STORE](#settings-and-store)).

## Routing

Each button press and encoder turn goes, in this order (`app/views.h`):

1. **a capture:** a view waiting for one control (M1d's MIDI learn, the boot animation, a
   title card) sees everything first, HOME included, so HOME can cancel it; it ends when its
   view closes;
2. **global:** HOME opens the menu from LIVE and goes back one view anywhere else; SAVE
   saves on a clean release, or held with a knob opens MIDI learn over any view; SAVE held
   with HOME consumes HOME ([SAVE](#save)); SEQ, PLAY and REC are the song views' ([below](#song-views)),
   in every view;
3. **the view on top:** the menu and CHIP take SELECT and ALGORITHM, SCENE takes SELECT;
   LIVE, the status screen, the glyph sheet and About take nothing; MIDI learn's frame passes
   them to the CHIP view under it, and to no other;
4. **the panel:** everything else plays the instrument as below, in every view. K1..K4 turn
   the page of the view that shows: CHIP's focused section on the CHIP view (on top or under
   MIDI learn's frame), LIVE's knobs in every other view.

The note keys and MASTER always play, whichever view shows, but for two layers before the
panel: **GLO held** turns the leftmost keys into the song's mutes and solos, and in a **song
view** the keys audition the song's instrument ([Song views](#song-views)). The effect-slot shortcut (EDIT,
SCL, ENV) works in every view, the CHIP view included ([CHIP](#chip) says why it stays).

## Which chip the keys play

SIDOU has two SID chips, A and B. The panel and MIDI play **the keys' chip**:

- **no song playing:** chip A, exactly as with one chip;
- **a song playing on chip A:** chip B, on top of the song (KEYS **AUTO**, the default), or
  chip A with KEYS **A** (menu): a key then takes a voice from the song while it sounds, the
  song keeps the filter and the volume (K1, K2 and MASTER's volume do nothing on it) and its
  chip setup.

The keys' setup (the chip model, the profile, the character controls and the envelope mode)
travels with the keys: chip B always holds it, prepared in the background after every change,
so a key sounds the same on either chip. FX, the CHIP view's profile and character, and the
chip CCs change the keys' setup. When a song starts the keys move to chip B (held keys sound
again there); when it stops they move back to chip A once it has the keys' setup again and no
key is held. LIVE says **KEYS:B** in its top line while the keys play chip B; the status
screen shows both chips and marks the keys'. Until M2c plays songs, nothing starts one, so the
keys always play chip A. KEYS persists with the settings.

## UI sounds

**SOUNDS** in the menu (OFF by default, restored with the settings) turns
on short sounds on chip B's spare voice: a blip as the menu's cursor moves, a confirm as
ALGORITHM changes a value or opens a view, a chime with a toast, a buzz when an action is
refused (the cursor already at the menu's end, an action turned counter-clockwise), and a
drive "chunk" when SAVE saves. They never take a voice the keys use: with the keys
on chip A they play on chip B; with the keys on chip B (during a song) only on a voice whose
note has finished its release, and a note that needs that voice takes it back at once, ending
the sound. Off, they write nothing and chip B stays out of use.

## Panel

| Control (id) | Does |
|---|---|
| Note keys (14..40) | play F3..G5 at the current octave, at velocity 127: they sound as velocity off does, whatever the preset's velocity target |
| OCT- / OCT+ (buttons 0, 1) | octave -1 / +1 (-3..+3); their LEDs show the direction |
| FX (button 2) | chip 6581 / 8580; lit for the 8580 |
| K1 (encoder 2) | cutoff (on the CHIP view: its focused section's first value, [below](#chip)) |
| K2 (encoder 3) | resonance (CHIP: the second) |
| K3 (encoder 4) | pulse width (CHIP: the third) |
| K4 (encoder 5) | preset (CHIP: the fourth) |
| MASTER (pot) | volume |
| **EDIT (button 6) held** + K1 / K2 / K3 / K4 | the selected effect slot's **Amount / A / B / Mix** (32 detents end to end), in every view; K1-K4 are the knobs above again once EDIT is let go. EDIT is lit while the selected slot has an algorithm |
| **SCL (button 3)** | selects the next effect slot: Distortion, Modulation, FX |
| **ENV (button 4)** | the selected slot's next algorithm (back to OFF after the last) |
| **HOME (button 8)** | the menu, from LIVE; back one view from any other (the menu, CHIP, the song views, the status screen, SCENE, the glyph sheet, About); from the menu back to LIVE behind THE OSCILLATOR's title card (below) |
| **SAVE (button 9)** | press and release without another button or encoder: save settings; hold and turn K1..K4: MIDI learn for that knob ([SAVE](#save)) |
| **SEQ (button 11)** | opens the song views, and steps through them ([below](#song-views)) |
| **PLAY (button 12)** | plays the song (SIDOU LOOP until songs load) from the song views' position, and stops it (each voice's release rings on); with EDIT held, goes on from where it stopped; lit while it plays. While it plays the keys play chip B (KEYS AUTO), or take voices from the song on chip A (KEYS A). A song the device cannot play is refused with a toast (`SNG: MULTISPEED 8 (MAX 4)`) |
| **REC (button 13)** | record mode on and off |
| **GLO (button 7) held** + note key 1..3 / 4..6 | mutes voice 1..3 / solos voice 1..3 of the song (again: back), in every view; the keys sound nothing while GLO is held |
| **SELECT (encoder 0)**, in the menu | moves the blinking cursor up and down the items |
| **ALGORITHM (encoder 1)**, in the menu | changes the item under the cursor: PRESET, OCTAVE, CARDS (the title cards: clockwise ON, counter-clockwise OFF), STORE ([format confirmation](#settings-and-store)), KEYS (clockwise A, counter-clockwise AUTO: [which chip the keys play](#which-chip-the-keys-play)), SOUNDS (the UI sounds, below: clockwise ON, counter-clockwise OFF); turned clockwise on CHIP, opens the CHIP view (behind THE CHIP's title card), on TRACKER the song views, on STATUS the status screen, on SCENE the generative pieces (behind THE SCENE's title card), on CHARSET the glyph sheet, on ABOUT the credits |
| **SELECT (encoder 0)**, on CHIP | moves the focus: CHIP (the profile and the character), DIST, MOD, FX; it stops at either end |
| **ALGORITHM (encoder 1)**, on CHIP | steps the focused section's choice: the profile on CHIP, the algorithm on a slot; it stops at either end |
| **SELECT (encoder 0)**, in SCENE | the next piece clockwise, the one before counter-clockwise (FIRE, PLASMA, RULE 30, STARS, ANT, and round): it wipes in, and a toast names it |

The other buttons (LFO, ARP) and PRESETS (encoder 6) do nothing yet; SELECT does nothing outside the menu, CHIP and SCENE, ALGORITHM nothing outside
the menu and CHIP.

## Song views

The song views make and play a song (M2d): **STEP**, **TRACKER**, **ORDER**, **INSTR** and
**TABLE**, all on the same GoatTracker 2 song (`app/song_view.h`). Until M2d Task 2 there is
one, a TRACKER placeholder: the header, the voice heads and the rows' numbers. The song is
SIDOU LOOP until songs load from the store (M2c Task 8).

| Control | Does |
|---|---|
| **SEQ** | from any view, opens the first song view over it (HOME goes back to that view); in a song view, the next one: STEP, TRACKER, ORDER, INSTR, TABLE and round (the views not made yet are skipped). The menu's TRACKER opens them too |
| **PLAY** | in every view: plays the song from the orderlist entry the song views show (the cursor's), or stops it (releases ring on). While it starts (the keys moving off chip A, [which chip the keys play](#which-chip-the-keys-play)) it shows as playing; PLAY again stops it |
| **EDIT held + PLAY** | goes on from where the song stopped (from the cursor if it never played) |
| **REC** | record mode on and off: REC in red on the song views' top line |
| **GLO held + note key 1, 2, 3** | mutes voice 1, 2, 3 of the song, or lets it sound again |
| **GLO held + note key 4, 5, 6** | solos voice 1, 2, 3, or unsolos it: while any voice is soloed the soloed ones sound and the rest are silent; with none, the muted ones are silent |
| **Note keys**, in a song view | audition the song's current instrument on the cursor's voice through the player, on chip A with the song's setup, at the panel's octave, one note at a time (the last key wins). With the song stopped the audition takes chip A for the song (the keys elsewhere move to chip B, as for PLAY; the first note sounds a few tens of milliseconds late) and gives it back two seconds after the last note is let go; no row of the song plays |
| **SAVE** | a clean release saves settings; held + K1..K4 opens MIDI learn. Saving the song itself comes with M2c Task 8 |

GLO's layer works in every view and comes before the panel: a key pressed while GLO is held
sounds nothing until it is let go, even if GLO is let go first; a key already sounding when GLO
goes down sounds on. The mutes and solos show on every song view's voice heads (MUT in light
red, SOL in white, a silent voice's name in grey) and on LIVE's top line, in red in the
preset's place: **M13 S2** is voices 1 and 3 muted and voice 2 soloed.

The song views' top line: the view's name, a play mark or a stop square, the tempo in BPM
(the frame rate times the play routine's calls a frame, over the calls a row and the rows a
beat, 4; funktempo's two speeds averaged: tempo 6 on PAL is 125.3 BPM, shown 125), the
position (**ORD 03 ROW 1A**, in hex: the player's while it plays, the cursor's while it is
stopped) and REC. The second line: V1, V2, V3 in their waveform's colour (pulse cyan, saw
yellow, triangle light green, noise light grey), the waveform's name and a one-cell envelope
meter.

## Title cards

A major view entered from the menu opens behind a short title card in the logo's metal
lettering (`app/title_card.h`): **THE CHIP** before CHIP, **THE SCENE** before SCENE, and
**THE OSCILLATOR** before LIVE when HOME goes back to it from the menu (not at power-on, after
the boot, nor when MIDI learn's frame closes). The glow fades in, the title
wipes in, the card holds for a second and the view cuts in, about 2.6 s in all.

- **Any button or encoder** ends the card at once and does nothing else (FX does not toggle
  the chip, OCT+ does not step the octave, SELECT does not pick a piece or move CHIP's focus).
- **A note**, from the keys or MIDI, plays as ever and ends the card. The keys, MIDI and the
  audio are never held up by it.
- **CARDS** in the menu turns the title cards off (ALGORITHM counter-clockwise on it) and on
  again (clockwise). They are on by default; the setting persists across power-off.

## CHIP

CHIP (the menu's first item) is a chip's sound on one page (`app/chip_view.h`): its profile,
its character and the three effect slots, in PETSCII frames, for chip **A** or chip **B** (the
tabs on row 2). What a tab shows and edits follows [which chip the keys
play](#which-chip-the-keys-play):

- the keys' setup (live's), on both tabs, with no song: chip B follows it, so its tab says
  **FOLLOWS KEYS**;
- during a song, the A tab is the song's chip: it shows the song's setup and edits it (the
  profile and K1..K4; MIDI learn binds nothing there, since MIDI reaches chip A on its own
  only from M2e), and the B tab shows the keys' setup and edits it.

The effect slots are one chain on the mix of both chips: their frames show on both tabs, and
the B tab's note under the tabs says **SLOTS SHARED**. **KEYS** marks the tab of the chip the keys play.

| Rows | Section | Shows |
|---|---|---|
| 2 | CHIP and the tabs | **A** and **B**, the shown one in reverse video (white while the tabs have the focus), **KEYS** beside the keys' chip |
| 3 | the tab's note | on the B tab FOLLOWS KEYS  SLOTS SHARED (no song) or SLOTS SHARED WITH CHIP A (the keys on B); on a song's A tab SONG SETUP  SLOTS SHARED; nothing on chip A with no song |
| 4-7 | CHIP | the profile as asked for (6581 R2, 6581 R3, 6581 R4AR, 8580 R5, WORN 6581) and the model the chip plays |
| 8-13 | CHARACTER | AGE, DAC, THUMP and BLEED, each a bar and its value 0..127 |
| 14-17, 18-21, 22-25 | DIST, MOD, FX | the slot and its algorithm in the frame's title; its Amount, A, B and Mix, each named for what it does in that algorithm (OVERDRIVE: DRIVE, TONE, BIAS, MIX); a control the algorithm does not use, and every control of a slot that is off, greyed under its plain name (AMT, A, B, MIX) |
| 26 | | the help line: SELECT MOVE ALGORITHM STEP |

The focus is on the tabs, on the chip (both its frames) or on a slot: a play mark in the
margin and white frames show it.

| Control | On the tabs | On the chip | On a slot |
|---|---|---|---|
| SELECT | moves the focus: the tabs, CHIP, DIST, MOD, FX (it stops at either end) | the same | the same |
| ALGORITHM | the B tab clockwise, the A tab counter-clockwise | the next profile clockwise, the one before counter-clockwise (it stops at either end) | the slot's next algorithm clockwise, the one before counter-clockwise (it stops at OFF and at the last) |
| K1 / K2 / K3 / K4 | as on the chip | age / DAC sloppiness / output thump / filter bleed (32 detents end to end) | the slot's Amount / A / B / Mix (32 detents end to end) |

The slot in focus is the selected slot, the one SCL selects and the EDIT shortcut turns:
SELECT onto a slot selects it (and the toast says so, SLOT MOD), and SCL on a slot moves the
focus with it. A profile changes from the next 10 ms tick, as its CC does (a chip reset, once
the new chip is prepared, 70 to 110 ms later, the first 8580 after power-on about 150 ms:
held notes attack again on the new chip); the
page shows it at once, the model follows it, and the FX button still toggles the model. The
character's controls glide, as from MIDI ([Chip character](#chip-character)). Everything else
plays as on LIVE: the note keys, MASTER, OCT-/OCT+, FX, SAVE (save settings, or held + a knob
for MIDI learn, which picks the parameter K1..K4 turn on the page: AGE for K1 on the chip),
and HOME back to the menu. A change
from MIDI shows on the page at the next frame.

**The EDIT layer stays, as a shortcut.** EDIT + K1..K4, SCL and ENV were the slots' stopgap
until this view. They stay in every view, because they are a player's: they turn an effect
while the scopes and the keys stay in sight, one hand on EDIT, without the three presses and
the title card that reach CHIP; they cost nothing and conflict with nothing (EDIT is a hold
nothing else uses, and on the CHIP view they act on the same selected slot, so the page shows
what they turn). The CHIP view is where the slots are seen and named in full. M2 may give SCL
and ENV the jobs their names suggest (a scale, an envelope); then the shortcut keeps EDIT +
K1..K4 and the view keeps the rest.

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
by default; saved slot settings are restored at power-on.

## Chip character

On the panel they are the CHIP view's ([above](#chip)): ALGORITHM steps the profile and K1..K4
turn the four controls. The chip's profile and its four character controls (`core/sid/sid_character.h`; sources, definitions
and measurements: [decision record 0006](../decisions/0006-chip-profiles.md)):

| Control | CC | NRPN | Values |
|---|---|---|---|
| Profile | 85 | 2/0 | 0 6581 R2, 1 6581 R3, 2 6581 R4AR (power-on with the 6581), 3 8580 R5, 4 worn 6581 (artistic) |
| Age/heat | 86 | 2/1 | 0..127: the filter drifts darker, the 6581's non-linearity rises |
| DAC sloppiness | 87 | 2/2 | 0..127: leakier, more kinked voice DACs (a released voice still whispers) |
| Output thump | 89 | 2/3 | 0..127: a volume change steps the output, as the 6581's operating point does |
| Filter bleed | 90 | 2/4 | 0..127: the unfiltered voices also reach the filter, up to half |

A profile is a chip and a model: changing it resets the chip (held notes attack again on the
new chip, as at a model change), and the model follows it. The model (CC 25, the FX button) selects
the 8580 R5, or goes back to the last 6581 profile chosen. The four controls change the
playing chip without a reset, and glide there rather than jump, so they don't click: from
the next 10 ms tick, age and thump take 0.32 s from 0 to 127, the bleed 1.27 s, the DACs
1.72 s. A
profile applies from the next tick: the new chip is prepared in the background while the old
one plays on, and swaps in once it is whole, 70 to 110 ms later (the first 8580 after
power-on about 150 ms; the CHIP view and the toast show the choice at once). Changes faster
than that, a profile every 70 ms or less, restart the preparation each time: the chip swaps
once they stop. All of them keep the last value received and
survive preset and model changes. Every control at 0 is the profile as reSIDfp has it (the
worn 6581 aside, which goes beyond reSIDfp on purpose).

**Thump and the MASTER pot:** MASTER sets the chip's volume register, in 15 steps. With
output thump above 0 each step thumps, as a real 6581 does at every volume write: up to
about 280 LSB a step at thump 127 on a 6581, so turning MASTER clicks. Keep thump at 0 (the
default) for a clean MASTER.

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
NRPN 1/5) changes from the next 10 ms tick, to the last value received: its chip is
prepared in the background, a slice a tick, and swaps in when it is whole, so a stream of
changes never holds the instrument up; the FX button asks for a change the same way.

**Envelope: INSTANT / AUTHENTIC** (CC 26, NRPN 1/10; INSTANT by default, restored with the
settings). The real SID
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

## Settings and STORE

Settings load at power-on: CARDS, KEYS, SOUNDS, the preset and its edits, the keys' chip
model, profile and character, the effect slots, envelope mode, MIDI-learn bindings and the
last firmware version booted. Missing or unreadable settings use the defaults. Octave,
held notes, controllers and the view on screen are not restored; volume follows MASTER.

Changes are saved in the background **5 s after the last change to a persisted value**,
without a toast. A busy store waits; background saves never force audible gaps. With the
current flash timing limits, programs as well as erases wait for quiet. Stop playing and
let release tails fade before powering off, or use SAVE and wait for its completion toast.

The menu's **STORE** item, immediately after CARDS, shows free space outside the store's
reserve pool (for example, **STORE 193K FREE**) or **FOREIGN**. FOREIGN means the region
contains data SIDOU does not own: it is left untouched, and settings cannot persist.

On FOREIGN, turn **ALGORITHM clockwise once**: **FORMAT? TURN**. Turn clockwise again
without leaving the item to confirm. Counter-clockwise or moving the menu cursor cancels
the confirmation. **Formatting destroys the other firmware's data in the store**, erases
all 73 sectors and can interrupt the sound. STORE shows **ERASING n**, then the toast
**STORE FORMATTED**. Current settings are then saved when the store can take them. Formatting
is never automatic; ALGORITHM does not format a SIDOU store.

## SAVE

A plain SAVE acts on **release**, not on the initial press. Turning any encoder or pressing
another button while SAVE is down prevents that release from saving.

| Gesture | Result |
|---|---|
| SAVE pressed and released, no encoder turned or other button pressed while down | saves settings now, skipping the 5 s debounce; **SAVED** when committed |
| SAVE held + K1..K4, with or without EDIT | opens MIDI LEARN on that knob's parameter on the turn, not the press; the parameter does not move and SAVE's release saves nothing |
| Another knob turned while the learn frame waits, SAVE held or not | picks that knob's parameter instead |
| CC received while the frame waits | learns it, closes the frame and applies its value; **LEARNED CC 21 → CUTOFF** |
| SAVE pressed and released again in the learn frame | forgets the picked parameter's binding and closes: **CUTOFF: CC 21 FORGOTTEN**; with none, **CUTOFF: NO CC** and the frame stays |
| SAVE held + HOME in the learn frame | asks **CLEAR ALL MAPPINGS?**; release SAVE, then press and release it again to confirm: **MIDI LEARN CLEARED**; HOME or 10 s without a step keeps the mappings |
| HOME without SAVE held, or 10 s without a step in the learn frame | cancels and closes the frame; HOME opens nothing else |
| SAVE held + HOME outside the learn frame | consumes HOME without opening the menu; SAVE's release saves nothing |
| SAVE held + any other button | that button acts as usual; SAVE's release saves nothing |
| SAVE pressed during the boot or a title card | skips it only; its release saves nothing |

An explicit save waiting for 2 s may force short held flash operations, with gaps in the
audio; completion then says **SAVED (GAPS)**. Background saves never do this. With SOUNDS on,
a completed explicit save plays the drive chunk. A foreign store refuses it with
**STORE FOREIGN: NOT SAVED**. SAVE currently saves settings on CHIP and in the song views
too; USER-slot and song-file save precedence belongs to M2a Task 6 and M2c Task 8.

## MIDI learn

Any knob's parameter can follow a CC of your choice, from a controller on TRS or USB:

1. **Hold SAVE** (button 9) **and turn a knob.** A frame, MIDI LEARN, opens over the screen
   on the turn and names its parameter (CUTOFF
   &larr; SEND A CC) and does not move it. The knob is the parameter it turns on the panel, in
   the view under the frame: K1..K3 the cutoff, resonance and pulse width; on the CHIP view,
   K1..K4 its focused section's (AGE, DAC, THUMP, BLEED on the chip; a slot's Amount, A, B and
   Mix on a slot; SELECT moves the focus while the frame shows); with **EDIT held**, K1..K4
   the selected effect slot's Amount, A, B and Mix in any view (SCL first to pick the slot; it
   works while the frame shows). K4 alone on LIVE's page is the preset, which has no CC
   (program changes select it); trying it shows K4: THE PRESET, NO CC and opens no frame.
   Release SAVE once the frame opens: the frame keeps waiting and that release saves nothing.
2. **Send the CC** (move the controller's knob). The frame closes with LEARNED CC 21 &rarr;
   CUTOFF, the parameter jumps to the controller's value, and from then on follows it.

While the frame shows, the note keys, OCT-/OCT+, FX, SCL, ENV and EDIT work as usual, and on
the CHIP view SELECT and ALGORITHM (K1..K4 then pick on the page the focus gives). Over the menu
and SCENE, SELECT and ALGORITHM do nothing while it shows.

- **Cancel:** HOME (it closes the frame and opens nothing), or 10 s without a step (each
  step restarts the time). A cancelled learn changes nothing.
- **Forget one:** hold SAVE and turn the knob, release SAVE, then press and release SAVE
  again instead of sending a CC: the parameter's learned CC is forgotten (CUTOFF: CC 21
  FORGOTTEN), and its own CC is its own
  again. The frame shows SAVE: FORGET CC 21 when there is one. With no binding, it says
  CUTOFF: NO CC and stays open.
- **Clear all:** with the frame open, hold SAVE and press HOME (CLEAR ALL MAPPINGS?),
  release SAVE, then press and release SAVE again: MIDI LEARN CLEARED. HOME without SAVE,
  or 10 s without a step, keeps them.

What learning means for MIDI in and out ([midi.md](midi.md), MIDI learn): a learned CC
works on every channel and on both ports, like the map; the parameter's own CC keeps
working; if the learned CC was another parameter's, it now moves the learned one only (the
other keeps its NRPN); learning a parameter again moves it to the new CC, and a CC learned
for another parameter moves to it. The panel's changes to a learned parameter go out over
USB on the learned CC. Data entry and NRPN/RPN selection (6, 38, 96..101) and the channel
mode messages (120..127) cannot be learned.

The mappings persist with the settings, including forget and clear-all changes: background
save after 5 s without another persisted change, or a plain SAVE after the frame closes.

**Why SAVE.** SAVE is about keeping what you set, which a mapping is. Its plain press and
release now saves settings; MIDI learn has moved to a knob turned while SAVE is held.
Opening the frame on the turn distinguishes learning from saving without a long-press
delay, and works with EDIT's effect-slot layer. The mappings are saved with the settings.
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

The menu (HOME) lists CHIP (the CHIP view, above), TRACKER (the song views, above), PRESET,
OCTAVE, CARDS (the title cards, above), STORE, KEYS and SOUNDS with their values, then STATUS,
SCENE (the generative pieces, above), CHARSET (the glyph sheet: the font and the palette) and
ABOUT. A view opens with a wipe from the top and
closes with one from the bottom; CHIP, SCENE, and LIVE back from the menu, open behind a title
card instead, and cut in under it in bands. M2's STEP view will be the default then; for M1 LIVE is.

A change of octave, selected slot or algorithm shows a toast on the bottom line for
1.5 s ("OCTAVE +1", "SLOT MOD", "MOD CHORUS"), whatever made it: a button, the menu or
MIDI. A chip change shows "CHIP 8580" and sweeps raster colours down the border.
