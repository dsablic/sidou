<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# M1a hardware checklist

A manual checklist for SIDOU M1a on a real M-VAVE FM-1. Anyone with a unit
can run it. Copy this file, fill in the boxes and notes, and attach it to an
issue. Everything here has been tested only in fm1-emu so far: CI boots the
package, plays the instrument and checks the update protocol's step 1 on the
host, but no FM-1 has run SIDOU yet.

> **Risk.** Installing custom firmware can leave the unit unable to boot. Do
> it only if you can recover it (section 0) and return to stock (section 9).
> SIDOU and its update path come with no warranty (GPL-3.0). SIDOU is not
> affiliated with M-VAVE.

**Unit and build**

| | |
|---|---|
| Tester, date | |
| Unit (serial, colour, where bought) | |
| Firmware before (`fm1_install.py --info`: `FM-1_015` is V15) | |
| SIDOU commit, package SHA-256 (`sidou-pack` prints it) | |
| Computer, OS, MIDI interface | |

Mark each item **[x] pass** or **[x] fail**, and write what you saw.

## 0. Before you start: recovery

The update loader writes only the app area, never the flash head, so the
unit keeps M-VAVE's SPL and settings. Once step 1 of an install commits, the
unit runs the update loader at every power-on until an install completes. If
the unit still ends up unable to boot, these community recovery paths exist.
How each is used, and in which state of the unit it works, is part of what
this checklist records.

- [ ] I have M-VAVE's official `FM-1.fwsc` (V15, SHA-256
  `db1642b2b6fa5c2cccb11ffd13878068bb28601678d3644049f99dc40e7edb8a`) for
  the way back.
- [ ] I have read the instructions of FM-1-transporter
  (<https://github.com/kurogedelic/FM-1-transporter>) and of
  MvaveFM1Unbricker with jl-uboot-tool, and have them ready.
- [ ] I have Felucca's installer (`tools/fm1_install.py` from
  <https://github.com/hugelton/Felucca>, needs `mido` and
  `python-rtmidi`), and `fm1_install.py --info` finds the unit.

Notes:

## 1. Install

The package is `build/fm1/sidou.fwsc` (BUILDING.md) or the `sidou-fwsc`
CI artifact. Its update loader is Felucca's, byte for byte, so during the
install the unit appears as Felucca's loader.

- [ ] `fm1_install.py sidou.fwsc` starts: step 1 (SIDOU or stock reads the
  package and stages the loader) reports no error.
- [ ] The unit restarts into the update loader. It enumerates as Felucca's
  loader, whatever the OS shows (expected: "Felucca Update", USB
  1209:0002, identity `ota-FM-1_900`). Record what you see:
- [ ] Step 2 (the loader writes the app area) completes; the unit restarts.
- [ ] `fm1_install.py --info` reports `FM-1_900`. (Felucca's development
  builds answer the same; distinct digits come later.)
- [ ] Optional, only if you know what it writes: the aroum/fm1-custom-fw
  flasher with the same `.fwsc`. Record whether it writes the flash head
  (0..0x4000); SIDOU's own path never does.

Notes:

## 2. Interrupted and refused installs

Do this only with section 0 ready. Step 1 is the part SIDOU runs (it reads
the package and stages the loader); step 2 is the loader's (it writes the
app area).

- [ ] Pull the USB cable during step 1 (while the installer reports its
  first read requests, before the unit restarts). Within about 15 s (three
  unanswered reads of 5 s) SIDOU gives up: it erases what it staged, shows
  "FAILED" with a code, and reboots into SIDOU. Plug the cable back:
  `fm1_install.py sidou.fwsc` then installs normally.
- [ ] Power-cycle the unit during step 1: it comes back as SIDOU (its first
  boot clears a staging area left behind), and an install afterwards works.
- [ ] A refused package: build one with `sidou-pack --test-wrong-size` (its
  flash.bin is 4 KiB short) and install it with `fm1_install.py`. SIDOU
  refuses it before erasing anything: the screen shows "UPDATE" and
  "FAILED -15" for about 1.5 s, then the unit reboots into SIDOU as normal.
- [ ] Pull the USB cable during step 2 (the write). Plug it back: the unit
  comes up as the update loader again, and running `fm1_install.py
  sidou.fwsc` again finishes the install.
- [ ] Power-cycle the unit during step 2. It comes up as the update loader
  again, and the install finishes as above.

Notes:

## 3. Boot

Check the full sequence first, then let the instrument sit quiet for at least 6 s so its
version can be saved. The title-card-only checks below apply to subsequent boots of the
same version with a writable store.

- [ ] With no saved version, or after installing a different firmware version, the full
  sequence plays without holding HOME (the sequence described below). After a quiet save,
  power-cycle: only the title card plays. Reinstalling the same version keeps the short
  boot; installing a different version gives the full boot once again.
- [ ] The title card appears: Metal Chrome's scene, the spiked SIDOU letters
  with their red to white chrome and purple shadow on a dithered purple glow,
  between raster bars, with stars, a glint on the U and the tagline "FORGED IN
  SILICON FOR THE FM-1" (as `assets/logo/metal-chrome.png`; look for wrong
  colours, a shifted row, torn cells or a dither that shimmers), the version at the top and "PRESS ANY KEY" blinking at
  the bottom. Time from power-on to it:
- [ ] About 1.2 s later LIVE wipes in from the top: C64 blue, a light blue
  header line "LIVE ... 6581 SAW LEAD ... OCT+0", three voice boxes "V1 SAW"..
  "V3 SAW" with a flat yellow line each, FILTER LP and VU boxes, and the
  DIST, MOD and FX lines at the bottom.
- [ ] A note key on the title card: it plays at once, LIVE wipes in, and
  voice 1's scope shows the saw.
- [ ] OCT+ on the title card: LIVE wipes in, and the header stays OCT+0
  (the press only skipped). The same with FX: the header stays 6581.
- [ ] Tap HOME as soon as the title card shows: LIVE wipes in (not the full
  boot, and not the menu).
- [ ] Hold HOME while powering on, then let it go: the full boot plays
  ("READY.", `LOAD"SIDOU",8,1` typed, "SEARCHING FOR SIDOU", "LOADING" with
  colour stripes running in the border, "READY.", "RUN", then the title
  card), about 10 s, and a note key played during it sounds.
- [ ] HOME held at power-on, 5 times: the full boot every time (letting HOME
  go does not skip it).
- [ ] No sound, click or hum before a key is played.
- [ ] Power on and off ten times: it boots every time.

The first boot after any install is also the first time SIDOU's flash code
runs on the unit: at boot it reads the flash chip's id and, if an update
left its staging area (`0xE0000..0xE5000`) in use, erases it
(`ota_boot_cleanup`). Note anything unusual on that first boot (a longer
start, a dark screen for a moment).

Notes:

## 4. Note keys and their LEDs

Key 1 is the lowest. At OCTAVE 0 each key plays the note below (the
preset's saw, through the filter). Check the pitch of a few with a tuner
(A4 = 440 Hz). Its LED is lit while the key is held. The id is Felucca's
input id, for mapping notes.

| Key | Note | Hz | Id | Sounds right | LED while held | Notes |
|---|---|---|---|---|---|---|
| 1 | F3 | 174.61 | 14 | [ ] | [ ] | |
| 2 | F#3 | 185.00 | 15 | [ ] | [ ] | |
| 3 | G3 | 196.00 | 16 | [ ] | [ ] | |
| 4 | G#3 | 207.65 | 17 | [ ] | [ ] | |
| 5 | A3 | 220.00 | 18 | [ ] | [ ] | |
| 6 | A#3 | 233.08 | 19 | [ ] | [ ] | |
| 7 | B3 | 246.94 | 20 | [ ] | [ ] | |
| 8 | C4 | 261.63 | 21 | [ ] | [ ] | |
| 9 | C#4 | 277.18 | 22 | [ ] | [ ] | |
| 10 | D4 | 293.66 | 23 | [ ] | [ ] | |
| 11 | D#4 | 311.13 | 24 | [ ] | [ ] | |
| 12 | E4 | 329.63 | 25 | [ ] | [ ] | |
| 13 | F4 | 349.23 | 26 | [ ] | [ ] | |
| 14 | F#4 | 369.99 | 27 | [ ] | [ ] | |
| 15 | G4 | 392.00 | 28 | [ ] | [ ] | |
| 16 | G#4 | 415.30 | 29 | [ ] | [ ] | |
| 17 | A4 | 440.00 | 30 | [ ] | [ ] | |
| 18 | A#4 | 466.16 | 31 | [ ] | [ ] | |
| 19 | B4 | 493.88 | 32 | [ ] | [ ] | |
| 20 | C5 | 523.25 | 33 | [ ] | [ ] | |
| 21 | C#5 | 554.37 | 34 | [ ] | [ ] | |
| 22 | D5 | 587.33 | 35 | [ ] | [ ] | |
| 23 | D#5 | 622.25 | 36 | [ ] | [ ] | |
| 24 | E5 | 659.26 | 37 | [ ] | [ ] | |
| 25 | F5 | 698.46 | 38 | [ ] | [ ] | |
| 26 | F#5 | 739.99 | 39 | [ ] | [ ] | |
| 27 | G5 | 783.99 | 40 | [ ] | [ ] | |

- [ ] Three keys at once sound together; a fourth steals the oldest note.
- [ ] A held key shows its note name on the NOTES line.

Notes:

## 5. Buttons, encoders and MASTER

The printed labels on the FM-1 may not match these roles: SIDOU's mapping is
Felucca's input ids, and the labels are guesses. For each control, write
which printed control did it.

| Role (id) | Expected | Printed control that does it | Pass |
|---|---|---|---|
| OCT- (button 0) | OCTAVE -1 (to -3), keys an octave lower; its LED lit while the octave is below 0 | | [ ] |
| OCT+ (button 1) | OCTAVE +1 (to +3), keys an octave higher; its LED lit while the octave is above 0 | | [ ] |
| FX (button 2) | CHIP toggles 6581/8580 with no gap in the sound; FX LED lit for the 8580 | | [ ] |
| K1 (encoder 2) | CUTOFF; turning down takes the highs out | | [ ] |
| K2 (encoder 3) | RESO 0..15 | | [ ] |
| K3 (encoder 4) | PULSE (audible on PWM BASS) | | [ ] |
| K4 (encoder 5) | PRESET: SAW LEAD, PWM BASS, SYNC LEAD, NOISE PERC | | [ ] |
| MASTER (pot) | VOLUME 0..15, quieter turned down | | [ ] |
| SCL (button 3) | selects the next effect slot (the play mark on LIVE's DIST, MOD and FX lines moves) | | [ ] |
| ENV (button 4) | the selected slot's next algorithm (the name on LIVE's slot line changes) | | [ ] |
| EDIT (button 6) held + K1..K4 | the selected slot's Amount, A, B, Mix (the numbers on the status screen and the CHIP view; Mix as LIVE's bar), in every view; EDIT lit while the slot is on | | [ ] |
| HOME (button 8) | from LIVE, the menu (a wipe down from the top); in the menu, back to LIVE behind THE OSCILLATOR's title card (below; with CARDS OFF, a wipe up from the bottom) | | [ ] |
| SAVE (button 9) | press alone opens nothing; a clean release saves settings and shows SAVED when committed (or SAVED (GAPS)); held + K1 opens MIDI LEARN on CUTOFF, HOME closes it (section 7) | | [ ] |
| SEQ (button 11) | from any view, the TRACKER (black, a light blue top line TRACKER, a stop square, the tempo 125, ORD 00 ROW 00; V1 V2 V3 on the second line; row numbers 00..14 with a blue bar on 00); SEQ again stays there, HOME goes back | | [ ] |
| PLAY (button 12) | SIDOU LOOP plays (a lead, a bass and drums, in time, no clicks between rows), in any view; TRACKER's top line shows a play mark and the rows move; PLAY again stops it and the notes ring out; EDIT + PLAY goes on from there; PLAY's LED lit while it plays | | [ ] |
| REC (button 13) | REC in red at the right of TRACKER's top line, and off again | | [ ] |
| A note key in TRACKER, the song stopped | SIDOU LOOP's first instrument on voice 1 (V1's meter moves), nothing of the song plays; let go, it releases; LIVE's keys sound the same as before afterwards | | [ ] |
| GLO (button 7) held + the leftmost note key | while SIDOU LOOP plays, its voice 1 goes silent; TRACKER's V1 head says MUT in light red, LIVE's top line M1 in the preset's place; the key sounds nothing; again: back. GLO + the fourth key: only voice 1 sounds, SOL on V1 | | [ ] |
| SELECT (encoder 0), in the menu | the blinking block cursor moves between CHIP, TRACKER, PRESET, OCTAVE, CARDS, STORE, KEYS, SOUNDS, STATUS, SCENE, CHARSET and ABOUT | | [ ] |
| ALGORITHM (encoder 1), in the menu | changes the item under the cursor (PRESET; OCTAVE -3..+3; CARDS: counter-clockwise OFF, clockwise ON); clockwise on CHIP opens the CHIP view behind THE CHIP's title card (below), on STATUS the status screen (the numbers), on SCENE the generative pieces (below), on CHARSET the glyph sheet, on ABOUT the credits (the chrome lettering at the top, the credits pages fading over stars, raster bars behind a white sine scroller), and HOME goes back to the menu | | [ ] |
| SELECT (encoder 0), on CHIP | the focus (a play mark and white frames) moves CHIP, DIST, MOD, FX and stops at either end; onto a slot that was not the selected one, the toast says SLOT DIST (or MOD, FX); onto the slot already selected (DIST at power-on), no toast | | [ ] |
| ALGORITHM (encoder 1), on CHIP | on CHIP the PROFILE steps 6581 R2, 6581 R3, 6581 R4AR, 8580 R5, WORN 6581 (MODEL follows; held notes attack again on the new chip, as at a model change); on a slot its algorithm, OFF to the last, named in the slot's frame | | [ ] |
| K1..K4, on CHIP | on CHIP AGE, DAC, THUMP, BLEED (bars and values; the sound darkens with AGE, a released note whispers with DAC); on a slot its four controls, named for the algorithm (OVERDRIVE: DRIVE, TONE, BIAS, MIX) | | [ ] |

- [ ] A held note keeps its pitch when the octave changes, and stops when
  the key is released.
- [ ] OVERDRIVE with Amount and Mix up distorts audibly; REVERB or LUSH with
  Mix up adds a stereo tail (listen on headphones); switching algorithms
  while a note plays crossfades without a click.
- [ ] In the menu, on the status screen, in SCENE and on the glyph sheet, the
  keys, K1..K4, OCT-/OCT+, FX and the EDIT layer play as on LIVE. On the CHIP
  view the keys, OCT-/OCT+, FX and the EDIT layer do too (EDIT + K1 turns the
  selected slot's Amount, whatever the focus), and K1..K4 are the page's.
- [ ] On the CHIP view, a character or slot CC from a controller (CC 86 for AGE,
  CC 102..116 for the slots) moves its bar or value on the page as it plays.
- [ ] OCT+, SCL and ENV show a toast on the bottom line ("OCTAVE +1",
  "SLOT MOD", "MOD CHORUS") that goes after about 1.5 s; FX shows
  "CHIP 8580" and sweeps colour bands down the border for about half a
  second.
- [ ] **The song views' button ids (M2d).** GLO 7, SEQ 11, PLAY 12 and REC 13 (and LFO 5,
  ARP 10, unused yet) are Felucca's ids and were never confirmed on a SIDOU unit (M1 used
  only FX, SCL, ENV, EDIT, HOME and SAVE). The four rows above check them by what they do;
  write the printed control for each. If one does nothing and another printed button does
  its job, only `app/panel.h` changes: note which id is which. M2d is not done on hardware
  until this passes.
- [ ] The other buttons and encoders do nothing (record anything they do);
  SELECT does nothing outside the menu, CHIP and SCENE, ALGORITHM nothing
  outside the menu and CHIP.

Notes:

## 6. LCD

The panel runs at 12 MHz SPI (Felucca's setting).

- [ ] Text is sharp, colours right, no stray pixels or shifted cells.
- [ ] Only what changes is redrawn: no flicker while idle, none while
  turning K1 fast or playing fast notes.
- [ ] Nothing goes wrong on screen while the audio plays (no clicks either).
- [ ] The wipes and the border raster run smoothly, with no tearing or
  stray cells left behind, and no click in the sound while they run.

LIVE (the screen at power-on):

- [ ] At rest the three scopes are flat lines, the envelope bars and the
  VU meters empty, and nothing flickers.
- [ ] A held key: its voice's scope shows the waveform (SAW LEAD a sawtooth,
  PWM BASS a pulse whose width moves, SYNC LEAD voice 1's synced saw, NOISE
  PERC noise), standing still while the note holds; its envelope bar rises
  and falls with ATK, DEC, SUS and REL beside it; its VU meter moves. Higher
  notes show more periods.
- [ ] Released, the scope goes flat again once the release has faded.
- [ ] A three-note chord: all three scopes move; with noise on all three
  (CC 20 = 64) they keep moving, taking turns, with no tearing.
- [ ] K1 and K2 move the FILTER curve (the yellow column is the cutoff),
  and the FC and RES numbers under it; MIDI CC 23 changes its mode (LP, BP,
  HP, NOTCH...).
- [ ] The L and R meters follow the output; with REVERB on, the tail shows
  on them after the note.
- [ ] No click or crackle in the sound while LIVE shows, against the menu
  (LIVE taps the audio interrupt; the menu does not).

Title cards ([controls](reference/controls.md#title-cards)):

- [ ] Menu, SCENE: THE SCENE's card first (as `assets/logo/title-scene.png`):
  a dark blue to purple dithered glow fading in over about a second in four
  steps, THE and SCENE in the metal lettering wiping in from the left, a
  second held, then SCENE cutting in from the top in bands. Look for wrong
  colours, torn cells, a shimmer in the dither or a step of the glow that
  flashes. Time from the turn to SCENE (about 2.6 s):
- [ ] Menu, CHIP: THE CHIP's card (as `assets/logo/title-chip.png`), then the
  CHIP view in bands.
- [ ] Menu, HOME: THE OSCILLATOR's card, then LIVE. Not at power-on, not
  after the boot, and not when MIDI learn's frame closes.
- [ ] OCT+, FX or SELECT on a card: the view comes in at once, and the
  octave, the chip and SCENE's piece stay as they were.
- [ ] A note key on a card, and a note over MIDI: it sounds at once (no
  delay against playing on LIVE) and the view comes in.
- [ ] A chord held and played through a card: no click, crackle or late note.
- [ ] Menu, CARDS turned counter-clockwise: OFF; SCENE and HOME back to LIVE
  now wipe in with no card. Clockwise: ON, and the cards are back. Set OFF,
  wait at least 6 s with no further changes and the keys released, then power-cycle:
  CARDS stays OFF. Restore ON if needed for the remaining title-card checks.

CHIP (menu, CHIP; [controls](reference/controls.md#chip)):

- [ ] The page as `tests/golden/lcd/chip.png`, its state set over MIDI (the emulator test's
  CCs): AGE 100, DAC 40, THUMP 64, BLEED 20; DIST OVERDRIVE, MOD CHORUS, FX REVERB, every
  slot on with its controls named and lit. The A tab in reverse, B M2 greyed, the CHIP and
  CHARACTER frames white with the play mark, the slot frames light blue.
- [ ] With every slot OFF (ALGORITHM counter-clockwise on each), a slot's controls are
  greyed under their generic names (AMT, A, B, MIX).
- [ ] ALGORITHM on CHIP between two 6581 profiles (R2 and R3, R3 and R4AR, either way) shows
  no toast and no raster, by design: only a model change (to or from 8580 R5) says CHIP 6581
  or CHIP 8580 and sweeps the border. The PROFILE line changes either way.
- [ ] Turning K1 fast end to end on CHIP: the AGE bar follows without lag or torn cells,
  and the sound darkens smoothly, with no click.
- [ ] MIDI CCs for every slot and character control at once (a controller's snapshot,
  or a DAW sending them in one go): the page catches up within a few frames, no cell
  left stale.

SCENE (menu, SCENE; [controls](reference/controls.md#scene)):

- [ ] It opens on FIRE: black, with a brown ember line at the bottom that
  holds still in silence.
- [ ] SELECT clockwise: PLASMA, RULE 30, STARS, ANT, then FIRE again, each
  wiping in from the top with its name as a toast; counter-clockwise goes
  back. HOME returns to the menu.
- [ ] FIRE: a key flares and heats one flame (the first note the left
  third); a three-note chord all three, in tongues of red and light red;
  higher notes burn higher; K1 down dulls them to red and light red, up
  burns yellow and white near the bed. Released, they die back to the
  embers.
- [ ] PLASMA: blue and purple dithered clouds drifting slowly in silence,
  faster with notes held and with K1 up; each note-on drops a bright ring
  that grows from a place set by the note (higher notes higher up).
- [ ] RULE 30: a grey triangle at the start; each note-on writes a new
  generation in the voice's colour (SAW LEAD's yellow) on the row below the
  last, a held note keeps it going, and it stops in silence. K1 turned well
  down darkens the colour.
- [ ] STARS: stars fly out of the centre, slowly in silence, faster with
  notes; a note-on lights a third of them in its colour for a moment, and
  high notes steer the centre right, low ones left.
- [ ] ANT: three white balls (grey while their voice is silent) walking
  Langton's ant trails while their voices sound, one per voice.
- [ ] No flicker or tearing beyond the pieces' own motion, the sound stays
  clean (SCENE taps the audio interrupt as LIVE does), and fast playing
  (a run of notes, a chord stab) keeps the sound and the keys on time.

Notes:

## 7. MIDI over the TRS jack

Which TRS-to-DIN adapter (type A or B) the FM-1 needs is not known yet:
record the one that works.

- [ ] Adapter type that works:
- [ ] Note on/off on any channel (1..16) plays and releases.
- [ ] CC 74 cutoff, CC 71 resonance, CC 1 pulse width, CC 7 volume move the
  sound and the bars.
- [ ] Program change 0..3 selects the preset; 4 and above change nothing.
- [ ] CC 123 (all notes off) and CC 120 release held notes.
- [ ] CC 112 = 90 and CC 116 = 127 turn the FX slot's REVERB on
  ([controls.md](reference/controls.md)); CC 116 = 0 back to dry.
- [ ] A dense part from a sequencer (running status, clock on) plays without
  hanging notes. Latency feels:

**MIDI learn (SAVE held + knob)** ([controls.md](reference/controls.md), MIDI learn), with a controller
that sends a CC the map leaves free (e.g. CC 14; [midi.md](reference/midi.md) lists the
map's) on the TRS jack. While the frame waits for a CC, any learnable CC binds, whatever
sends it: a DAW that sends CC 0, 32 or 64 when its transport starts, or a controller's
pedal, binds that instead. Stop the transport and keep pedals still while learning.

- [ ] Press and hold SAVE: no learn frame yet. Turn K1: the MIDI LEARN frame opens on
  CUTOFF &larr; SEND A CC, and the cutoff does not move. Release SAVE: no save toast,
  and the frame stays. Move the controller's
  knob: LEARNED CC 14 &rarr; CUTOFF, the frame closes, and the controller's knob now sweeps
  the cutoff (the bar and the sound) on any channel. CC 74 still moves it too.
- [ ] The same over USB from a DAW, and the other way: learn K2 on CC 15 from USB, then
  send CC 15 from a TRS controller: both ports drive what either learned.
- [ ] EDIT held + K4 while learning, then a CC: the selected slot's Mix follows it
  (LEARNED CC n &rarr; DIST MIX; SCL first for MOD or FX).
- [ ] On the CHIP view with the focus on CHIP, hold SAVE and turn K1: AGE &larr; SEND A CC;
  release SAVE, then send the CC: it moves AGE's bar and the sound. On a slot, hold SAVE
  and turn K4: that slot's MIX.
- [ ] On the CHIP view with the focus on CHIP, hold SAVE and turn K1, release SAVE, then
  SELECT one step clockwise while the
  frame shows: the focus moves to DIST under the frame, and K4 then learns DIST MIX (not
  BLEED). ALGORITHM under the frame steps DIST's algorithm. In the menu, SELECT and
  ALGORITHM do nothing while the frame shows.
- [ ] Learn the resonance on CC 74: CC 74 now moves the resonance, not the cutoff (the
  cutoff's NRPN 0/0 still does).
- [ ] With a DAW monitoring SIDOU's USB output, turn K1 after learning CC 14 for it: the
  monitor shows CC 14, not CC 74. Moving the controller's knob sends nothing back.
- [ ] HOME while the frame shows: LEARN CANCELLED, no menu, nothing learned. Leave the
  frame alone for 10 s: LEARN TIMED OUT.
- [ ] Hold SAVE and turn K1, release SAVE, then press and release SAVE: CUTOFF: CC 14
  FORGOTTEN, and CC 14 does nothing. Repeat with no binding: CUTOFF: NO CC, frame stays.
- [ ] With the learn frame open, hold SAVE and press HOME: CLEAR ALL MAPPINGS?. Release
  SAVE: nothing cleared yet. Press and release SAVE again: MIDI LEARN CLEARED. Repeat with
  HOME without SAVE held instead of confirming, then with a 10 s wait: mappings are kept.
- [ ] Outside the frame, hold SAVE and press HOME: no menu and no frame; release SAVE:
  no save. Hold SAVE and press OCT+: octave changes, but release SAVE saves nothing.
- [ ] While the frame waits, turn K2 without holding SAVE: RESO &larr; SEND A CC; resonance
  does not move. On LIVE, SAVE held + K4 says K4: THE PRESET, NO CC and opens no frame.
- [ ] The note keys play while the frame shows, and the sound has no gap when it opens or
  closes.
- [ ] Learn CC 14 for K1, release all notes and wait at least 6 s without changing a persisted
  value; power off and on: CC 14 still moves cutoff. Forget it, wait for another quiet save,
  power-cycle: the binding stays forgotten.

Notes:

## 7.1 Settings, SAVE and STORE (M2a Task 5)

Use a writable store for the persistence checks. If STORE says FOREIGN, read the destructive
format check below before deciding whether to format; nothing persists until then.

- [ ] Change CARDS, KEYS and SOUNDS, the preset and a preset edit, the chip profile and a
  character control, an effect algorithm and Mix, and the envelope mode (CC 26). Release
  notes and wait at least 6 s after the last change, without further persisted changes:
  no background-save toast. Power-cycle: all those values and the learn binding return.
  Octave returns to 0, no notes are held, LIVE opens and volume follows MASTER.
- [ ] Make several changes less than 5 s apart, then stop: the settings save follows the
  last change, not the first. After a quiet save and power-cycle, the final values return.
- [ ] Change CARDS, then press and hold SAVE: no frame and no save yet. Release without
  turning an encoder or pressing another button: SAVED appears when the write commits,
  without the 5 s debounce. Power-cycle after the toast: the new value returns.
- [ ] Hold a chord and release SAVE to request a save. If it cannot write while playing,
  after about 2 s short held operations may interrupt the sound; completion says
  SAVED (GAPS). With SOUNDS on a completed explicit save plays the drive chunk. A background
  save alone waits for quiet and never forces gaps.
- [ ] In the menu, STORE is immediately after CARDS and shows nK FREE, excluding the
  reserve pool. ALGORITHM on a writable SIDOU store does not format it.
- [ ] **Only with unwanted foreign data and permission to destroy it:** STORE says FOREIGN.
  A plain SAVE says STORE FOREIGN: NOT SAVED; power-cycling without formatting leaves the
  region foreign. ALGORITHM clockwise once on STORE says FORMAT? TURN; counter-clockwise
  cancels, as does moving SELECT away and back. Nothing is erased by those cancelled steps.
- [ ] **Destructive confirmation:** on FOREIGN, turn ALGORITHM clockwise twice without
  leaving STORE. The store's 73 sectors are erased; STORE shows ERASING n (sound can have
  gaps), then STORE FORMATTED and nK FREE. Let settings save, power-cycle, and check that
  STORE is no longer FOREIGN and settings persist. Never format automatically just to
  proceed with the checklist.

Notes:

## 8. MIDI over USB

- [ ] The unit enumerates as a USB-MIDI device "SIDOU" by "SIDOU
  contributors" (pid.codes test VID/PID 1209:0001). Record what the OS shows:
- [ ] The same note, CC and program tests as section 7 pass over USB.
- [ ] TRS and USB at the same time both play.
- [ ] **MIDI out (M1d):** in a DAW (or a MIDI monitor) on the SIDOU input, turn K1:
  CC 74 on channel 1 follows the knob, at most a message every 10 ms, and stops when
  the knob stops. K2 sends CC 71, K3 CC 1, MASTER CC 7, FX CC 25 (127 for the 8580,
  0 for the 6581), K4 a program change 0..3 and nothing else, and EDIT held with K1..K4
  CCs 103..106 for the Distortion slot (SCL moves to 108..111 and 113..116). On the CHIP
  view K1..K4 on CHIP send CC 86, 87, 89 and 90, ALGORITHM on CHIP CC 85, on a slot its
  algorithm's CC (102, 107, 112). Record
  what the monitor shows for one K1 turn:
- [ ] Record K1 as automation in the DAW, then play it back into SIDOU over USB: the
  cutoff moves as recorded, and the monitor shows no CCs coming back from SIDOU while
  it plays (no echo).
- [ ] With the DAW's MIDI thru (input monitoring) on, send CC 74 to SIDOU over USB and
  over TRS: nothing comes back from SIDOU, so no feedback loop, no runaway messages.
- [ ] Notes from the keys are not sent (M1: parameters only).
- [ ] **MIDI out across a host sleep:** with the monitor open, put the computer to
  sleep, turn K1 and K2 while it sleeps, then wake it: SIDOU still shows up, no burst of
  the values from the sleep arrives, and the next K1 turn shows CC 74 again within a
  second. Record what the monitor shows right after the wake:
- [ ] **MIDI out across a replug:** turn K1 continuously, unplug the USB cable mid-turn
  (on battery), keep turning a few seconds, plug it back in and keep turning: the sound
  never stops, the unit enumerates again, and CC 74 follows the knob again without a
  stale burst. Repeat with the cable pulled and replugged quickly (under a second).
- [ ] *(Optional, a build configured with `-DSIDOU_MIDI_OUT_NRPN=ON`.)* K1 sends NRPN
  0/0 as four CCs (99 0, 98 0, 6 and 38) at full 11-bit resolution, K3 NRPN 0/2; K2
  still sends CC 71. Record K1 at both ends (0 and 2047):
- [ ] With the DAW closed (the port open by no application), turn the knobs for a
  minute: the sound and the panel stay responsive; opening the DAW afterwards gets no
  burst of old values beyond at most 64 messages.
- [ ] Unplug the USB cable while playing (on battery): the sound goes on
  without dropouts, and plugging it back in enumerates again within a few
  seconds.
- [ ] Put the computer to sleep and wake it while the unit plays: no
  dropouts, MIDI works again after the wake.
- [ ] On battery with no cable for five minutes: the keys and screen stay
  responsive and the sound clean (USB is retried once a second).
- [ ] If you can measure it: supply current with no cable:

Notes:

## 9. Reinstall, back to stock V15, and SIDOU again

- [ ] From SIDOU, install `sidou.fwsc` again (SIDOU to SIDOU): it installs,
  and `--info` reports `FM-1_900`.
- [ ] From SIDOU, `fm1_install.py FM-1.fwsc` (the unmodified official V15)
  installs: SIDOU's step 1 accepts it (its flash head is the unit's), the
  official loader writes it, and `--info` reports `FM-1_015`.
- [ ] The stock firmware plays its patches as before.
- [ ] Installing `sidou.fwsc` again from stock works (section 1).

Notes:

## 10. Boot guard, watchdog and resets

**Run this section only after a recovery from UBOOT has been shown to work
on this unit** (with a tool from section 0), since its expected end is
UBOOT.

It needs a checklist build that dies on purpose, one second after boot,
before the boot counts as stable (10 s). Build it as BUILDING.md says, with
`-DSIDOU_TEST_DEATH=hang` (interrupts off: the watchdog resets the chip
after about 8 s) or `-DSIDOU_TEST_DEATH=reboot` (a P33 reset, as the fault
handler does), and package it as usual. It dies **only when OCT- (button 0)
is held at power-on**, and only on that boot: started without the key it is
the normal SIDOU, so the normal installer always reaches it. Never give
these builds to anyone else, and install the normal package again when you
are done.

- [ ] Without OCT-: the checklist build boots and plays as normal, and stays
  up past 10 s.
- [ ] Hang build: hold OCT- while powering on and keep it held through the
  resets. Boot 1 dies, the watchdog resets, boot 2 dies, and boot 3 enters
  UBOOT instead of SIDOU. Record what the screen shows and how the unit
  appears on USB in UBOOT. This shows the guard state in `.noinit` survives
  a watchdog reset.
- [ ] In UBOOT, the unit stays there (no watchdog reset) for at least five
  minutes.
- [ ] From UBOOT, install the normal `sidou.fwsc` with a recovery tool from
  section 0, and record which one worked and how:
- [ ] Reboot build: the same, with the guard surviving a P33 reset.
- [ ] One death, then a normal boot (OCT- released) that runs past 10 s,
  then one more death: no UBOOT (the stable boot cleared the count).
- [ ] Normal build: power-cycle quickly (off for under a second) three times
  within ten seconds of each boot. Record whether it ends in UBOOT (it
  should not, if RAM does not survive a power cut, but a brownout may keep
  it):

Notes:

## 11. Measurements for later work

Optional; each needs a measuring build that does not exist yet. Record what
you can.

- [ ] A photo of the chip's part number (docs/hardware.md expects AC7911B8).
- [ ] The real CPU clock (360 MHz expected after the SPL; the budget assumes
  240).
- [ ] Cycles per audio block against fm1-emu's instruction count, and the
  gain from running the audio path from SRAM (docs/hardware.md, open
  checks).
- [ ] Cycles per audio block with combined waveforms on both chips against
  single waveforms: the combined waveforms' pulldown rows are the audio
  interrupt's only flash data reads (docs/hardware.md, open checks).
- [ ] USB poll cost with a host attached (three SIE round trips per poll at
  2 kHz).
- [ ] Divide and IRQ-entry latencies.
- [ ] The voice DAC tables the device builds at boot match the host's. The
  core computes them in float (`sid_dac_build`), and the device's float
  division may round differently from the host and from fm1-emu's soft FPU,
  which the emulator tests compare against. A measuring build computes the
  CRC-32 of `sid_dac_default(0)` (8580) and `sid_dac_default(1)` (6581), each
  over the struct's bytes as it lies in memory (`wave`, then `env`), and
  shows them on the status screen (or, later, answers them over SysEx).
  Expected, from `cargo test --release -p sidou-tests --test unit --
  the_default_tables_crc --nocapture` in `host/`: 8580 `0xfde1a3ba`, 6581
  `0x748fe138`. Record the device's values:
- [ ] The 6581's filter on the device (M1b Task 6). Each 6581 sample does one
  FPU divide in the audio interrupt (the band-pass's damping, `sid_nl_tick`),
  which fm1-emu counts as one instruction. A measuring build renders the
  golden `workload_6581_1chip` (`bench/workload.c`, one chip, the 6581) on
  the device and compares its FNV-1a 64 hash with `tests/golden/hashes.txt`:
  a match shows the divide (and the rest of the float path) rounds as IEEE
  single precision does on the host. It also measures the cycles of the
  6581's render loop against the 8580's on the same workload, which shows the
  divide's real cost (in fm1-emu the workload's triad costs 400.50
  instructions a sample on the 6581 and 347.37 on the 8580). Record the hash
  and the cycles:
- [ ] Core 1 is not used by SIDOU; its RAM self-test (Felucca's) is for a
  later milestone.
- [ ] The store's write windows (M2a Task 4, once Task 3b's numbers let
  programs go in windows; docs/hardware.md, "How the store writes"): hold a
  chord and save settings (Task 5's SAVE) several times; listen for a click,
  a dropout or a missed block. Then play a TRS MIDI controller at full rate
  (a fast arpeggio) while saving: no note is lost or stuck.
- [ ] A hold's gap: release a chord and wait for its tail to fade; a pool
  refill (after a few saves) may follow 500 ms after the input stops. Listen
  for each held erase (a fade out and back, up to 35 ms): note whether it is
  heard, and whether a DAW's MIDI clock into the TRS jack keeps the refill
  from running (it must not).
- [ ] The first boot after an install on a unit whose store region is blank:
  the store formats its first 22 sectors at boot, up to 0.5 s more than a
  later boot. Note the boot's length against the next one's.

Notes:

## 12. Flash timing spike (M2a Task 3b)

The store (M2a) will write the flash while SIDOU plays, and the size of its
writes depends on how long this unit's flash part takes to program and
erase. The emulator can't tell; this section measures it with a test
firmware, the flash spike. While the spike is installed the unit is not
SIDOU: it makes no sound and shows only the spike's screen. You put SIDOU
(or stock) back at the end with the normal installer.

What the spike writes, and what it never touches:
- It erases and programs only the store's last two sectors,
  `0xDE000..0xDFFFF`, and in each only bytes below `0xF00`. It never
  writes the flash head, the app area or the update staging area
  (`0xE0000..0xE4FFF`).
- At boot it only reads: both sectors, and the whole store range
  `0x97000..0xDFFFF`. If either sector holds anything that is neither blank
  nor the spike's own, or the pattern it left in `0xDE000` has changed, it
  shows **REFUSED** and writes nothing.
- It starts measuring only when you press SAVE. Until then, and once it is
  done, it answers the normal installer just as SIDOU does.
- At the end it erases `0xDF000` back to blank and leaves a test pattern in
  `0xDE000`, for the stock round trip (12.4). The pattern is harmless:
  SIDOU's store treats that sector as unused and erases it when it needs it.

Like SIDOU, the spike's boot erases a staging area that an interrupted
install left in use (section 3). That is the update protocol's own cleanup,
not the spike.

### 12.0 Before you start

- [ ] Section 0 is done: M-VAVE's `FM-1.fwsc` (V15), FM-1-transporter,
  MvaveFM1Unbricker with jl-uboot-tool, and Felucca's `fm1_install.py`
  are at hand, and `fm1_install.py --info` finds the unit.
- [ ] Sections 1 and 9 have passed on this unit at least once: SIDOU
  installs, and the way back to V15 works.
- [ ] For the TRS check (optional): a MIDI sender on the TRS jack that never
  pauses, wired as in section 7. One way is a computer sending a large
  SysEx file in a loop through a USB-MIDI interface, or a sequencer
  streaming dense CCs. Full rate is 3,125 bytes a second; the spike shows
  how many arrived. Without a sender, the TRS rows read 0 and that check is
  skipped.
- [ ] A phone or camera, to photograph the result screen.

Notes:

### 12.1 Build the spike's package

From the repository root (BUILDING.md, "The flash timing spike"):

```sh
tools/fm1-docker cmake --preset fm1 -B build/fm1-spike -DSIDOU_FLASH_SPIKE=ON
tools/fm1-docker cmake --build build/fm1-spike
```

Then from `host/`:

```sh
cargo run --release -p sidou-pack -- \
    --app ../build/fm1-spike/platform/fm1/sidou_app.bin \
    --loader ../build/fm1-spike/platform/fm1/loader/loader.bin \
    -o ../build/fm1-spike/sidou-spike.fwsc
```

Also build the normal `build/fm1/sidou.fwsc` (section 1), for the way back.

- [ ] SIDOU commit, and the SHA-256 that `sidou-pack` prints for
  `sidou-spike.fwsc`:

### 12.2 Install it and read the start screen

Install it as in section 1, from SIDOU or from stock:
`fm1_install.py build/fm1-spike/sidou-spike.fwsc`. The unit restarts into
the spike.

- [ ] The screen reads "SIDOU FLASH SPIKE M2A TASK 3B". It shows the part
  (JEDEC, SR, CR, the SFDP lines and CLOCK), "START DE: ... DF: ...",
  "PATTERN AT START", "STORE SECTORS USED n/73", and "SAVE: START". There is
  no sound. Record:
  - JEDEC (expected `856014`), SR, CR:
  - SFDP and ERASE (the part's own table: the signature `50444653`, and the
    erase types, where `81` with size `08` would be the 256-byte page erase):
  - CLOCK, in adds/µs (192 at 240 MHz, 288 at 360 MHz):
  - START: both BLANK on a first run, `DE:SPIKE'S` after one:
  - STORE SECTORS USED (how much of the store range holds anything: Felucca
    or stock data, or nothing):
- [ ] If the screen says **REFUSED**: photograph it and go to 12.5. Nothing
  was written.

### 12.3 Run the measurements

Start the TRS sender, then press **SAVE**. The screen shows "RUNNING, STEP
n" for steps 0 to 6 and fills in the results; it takes under a minute.
**Do not power the unit off while it runs.** A cut there can leave the
spike's own sectors half-erased: the next boot shows REFUSED. Nothing else
is at risk, and SIDOU installs as usual.

- [ ] The screen ends with "DONE. DF000 ERASED, DE000 KEEPS THE PATTERN".
  Photograph it, and copy the numbers into
  [hardware.md](hardware.md)'s table ("The flash part's timings").
- [ ] If it ends with "FAILED AT STEP n RC m": photograph it and go to
  12.5.

How to read the screen:
- **TIMES US**: min, mean and max microseconds. `ERASE 4K` is a 4 KiB
  sector erase (20 runs). `PROG 16`, `PROG 64` and `PROG 256` are page
  programs of that many bytes (20, 20 and 280 runs). `PAGE ER` is the
  256-byte page erase, `SUSPEND` the erase-suspend latency. For comparison,
  the datasheet's maximums are 20 ms for an erase, 3 ms for a page program
  and 30 µs for a suspend.
- **PAGE ERASE 81**: `THE PAGE` (it erased that page and nothing else),
  `WIDER` (more than the page: the CR's DP bit makes pages 512 bytes),
  `IGNORED` (nothing changed), or `ODD` (anything else).
- **SUS1 n/5**: how many of the 5 suspended erases set the suspend bit.
  `READ` is whether `0xDE000` read correctly while suspended; `END` is
  whether each erase finished after the resume.
- **VERIFY**: every timed program read back. **PARTIAL**: 64 programs of
  4 bytes into one page (several programs per page). **REZERO**: zeros
  programmed over zeros, 3 times. **BITWALK**: one byte programmed 8
  times, clearing one more bit each time. **TWICE**: pages programmed twice
  with different data; anything but OK means the part does not just AND
  what it is given, which suggests ECC. **GUARD**: requests the spike's
  own bounds turned down (must be 0).
- **TRS**: bytes received and lost while the interrupts run (1 s), while
  they are masked for 1 ms (100 times), and for 35 ms (10 times). LOST
  must be 0 on the 35 ms row: the store's longest hold is 35 ms.

Notes:

### 12.4 The stock round trip (the store's range under V15)

This checks whether the stock firmware, or its SPL, writes into the store's
range, as the package's VM region allows (docs/hardware.md, "The flash
part's timings" and the M2a plan's flash map). Start right after a DONE,
with the pattern in `0xDE000`.

1. Install M-VAVE's V15: `fm1_install.py FM-1.fwsc` (as in section 9).
2. Use the stock firmware for a few minutes: play, and change and save a
   setting or a patch if it offers one. Power it off and on once.
3. Install the spike again: `fm1_install.py
   build/fm1-spike/sidou-spike.fwsc` (as in section 1).

- [ ] On the start screen, record "PATTERN AT START" (INTACT: the stock
  firmware left the sector alone; DAMAGED or ABSENT: something wrote or
  erased it), "START DE: ... DF: ...", and "STORE SECTORS USED", against the
  values before the round trip:
- [ ] If it shows REFUSED, something wrote the spike's sectors (PATTERN AT
  START DAMAGED, or a sector FOREIGN). That is the result: record it. The
  spike writes nothing then, so the evidence stays.
- [ ] Optional: press SAVE to measure once more and compare.

Notes:

### 12.5 Back to SIDOU or to stock

The spike answers the installer whenever it is not RUNNING: on the start
screen, after DONE, REFUSED or FAILED. The screen shows SIDOU's UPDATE box
during the install.

- [ ] Back to SIDOU: `fm1_install.py build/fm1/sidou.fwsc`. `--info`
  reports `FM-1_900`, and SIDOU boots and plays (section 3).
- [ ] Or back to stock: `fm1_install.py FM-1.fwsc`; `--info` reports
  `FM-1_015`.
- [ ] If the spike's screen never appears, or the unit doesn't answer the
  installer: three boots in a row that die within 10 s end in UBOOT
  (section 10), from which a tool from section 0 installs `sidou.fwsc` or
  V15. Record what happened and which tool worked.

Notes:

## Result

| Section | Pass / fail | Notes |
|---|---|---|
| 0 Recovery ready | | |
| 1 Install | | |
| 2 Interrupted and refused installs | | |
| 3 Boot | | |
| 4 Keys and LEDs | | |
| 5 Buttons, encoders, MASTER | | |
| 6 LCD | | |
| 7 TRS MIDI | | |
| 7.1 Settings, SAVE and STORE | | |
| 8 USB MIDI | | |
| 9 Reinstall, stock and back | | |
| 10 Guard, watchdog, resets | | |
| 11 Measurements | | |
| 12 Flash timing spike | | |
