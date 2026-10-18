<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU hardware checklist (M1 and M2)

A manual checklist for SIDOU on a real M-VAVE FM-1. Anyone with a unit can
run it. Copy this file, fill in the boxes and notes, and attach it to an
issue. The [first hardware run](hardware-results/2026-10-17-first-flash.md)
booted and played, but USB-MIDI output failed and recovery to V15 was needed.
That partial run does not mark the items below passed for the current build.
The flash timing spike has not run on a unit: program/erase timings remain
the datasheet's provisional maxima, not measured results.

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

On the day, [flash-day.md](flash-day.md) is the one-page order of work around this
checklist, and `tools/preflash` checks the package and the recovery route first (files only;
it never touches a device).

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
- [ ] **The first recovery route, from the USB fix on: OCT- at power-on.**
  With the unit off, hold **OCT- alone**, switch on, and keep holding for
  about a second: the screen stays black (the backlight goes off), and the
  unit appears on USB as the chip's ROM boot mode, `WL82 UBOOT1.00` (USB
  4C4A:8057). There `tools/fm1-unbrick-mac` (from a Mac) or
  MvaveFM1Unbricker (Linux, Windows) restores a package
  (`tools/fm1-unbrick-mac/README.md`). It is the first thing SIDOU runs at
  power-on, before anything that could hang or crash
  ([controls](reference/controls.md#at-power-on)), so it works whatever
  state the rest of the firmware or its update path is in. It needs a
  SIDOU build that has it (not v0.2.0-m2-preview), and it is checked on the
  unit right after the first install (section 1). Switching off and on
  without the key leaves the ROM boot mode with nothing written. The
  checklist's dying builds (section 10) leave it out.
- [ ] The fallback, for a unit without a SIDOU build that has the gesture: I
  can put the unit into ROM boot mode (`WL82 UBOOT1.00`, USB
  4C4A:8057) without the firmware's help: a Flipper Zero with
  [flipper-fm1-recovery](https://github.com/merthsoft/flipper-fm1-recovery), or the FM-1
  Transporter. From there MvaveFM1Unbricker writes a package from Linux or Windows, or from a
  Mac with `tools/fm1-unbrick-mac` (a Linux VM's USB passthrough did not work). This route
  recovered a unit on 2026-10-17: `docs/hardware-results/2026-10-17-first-flash.md`.
- [ ] After switching firmware on a Mac, the old USB-MIDI device is deleted in Audio MIDI
  Setup (Window > Show MIDI Studio): the Felucca family shares USB 1209:0001 and CoreMIDI
  otherwise keeps the old record.

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
- [ ] **Right after the first install of a build with the power-on UBOOT
  gesture, before anything else:** switch off, hold OCT- alone, switch on and
  keep holding. Within about a second the screen is black, and the computer
  shows `WL82 UBOOT1.00` (USB 4C4A:8057; on a Mac, System Information > USB).
  Record what you see:
  - [ ] `tools/fm1-unbrick-mac` (or MvaveFM1Unbricker) finds the unit there.
    Restore `sidou.fwsc` with it, or just switch off and on.
  - [ ] It stays in the ROM boot mode for five minutes (no watchdog reset).
  - [ ] Switched off and on without the key, SIDOU starts as before.
  - [ ] OCT- tapped briefly at power-on, OCT- held with OCT+, and OCT- held
    with a note key each boot SIDOU as usual.
  If the gesture fails, stop here: recover with the fallback in section 0
  and report it.
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
| SAVE (button 9) | clean release saves (7.1–7.2); held + knob learns (7) | | [ ] |
| SEQ (button 11) | from any view, the TRACKER (black: V1 at 2× in light red, PAT 00, a stop square, the tempo 125; SIDOU LOOP's first pattern in 12 rows at 2× with a dark grey bar on row 00, the other two voices as strips on the right, the orderlist and ORD 00 ROW 00 at the foot); SEQ again stays there, HOME goes back | | [ ] |
| SELECT (encoder 0), in TRACKER | the voice shown: V2 in cyan with its pattern, then V3 in light green, then V1 again; the other two voices are the strips (their numbers over them); counter-clockwise goes the other way; no popup, no toast | | [ ] |
| TRACKER ROWS in the menu | BAR: while SIDOU LOOP plays the bar moves down still rows, and the rows change cleanly at each new pattern; SCROLL: the bar stays on the middle row and the rows move up, smoothly at this tempo or a frame behind at most | | [ ] |
| PLAY (button 12) | SIDOU LOOP plays (a lead, a bass and drums, in time, no clicks between rows), in any view; TRACKER's header shows a play mark and the rows move (the bar, and the strips' tall bars where the other voices play notes); PLAY again stops it and the notes ring out; EDIT + PLAY goes on from there; PLAY's LED lit while it plays | | [ ] |
| REC (button 13) | REC in red under TRACKER's play mark, and off again | | [ ] |
| A note key in TRACKER, the song stopped | SIDOU LOOP's first instrument on the voice shown (after SELECT, on that voice), nothing of the song plays; let go, it releases; LIVE's keys sound the same as before afterwards | | [ ] |
| GLO (button 7) held + the leftmost note key | while SIDOU LOOP plays, its voice 1 goes silent; TRACKER's V1 turns grey and says MUT in red beside the pattern (a muted voice shown as a strip: grey and short), LIVE's top line M1 in red in the preset's place; the key sounds nothing; again: back. GLO + the fourth key: only voice 1 sounds, SOL beside V1's pattern | | [ ] |
| SELECT (encoder 0), in the menu | the selected line (reverse video) moves between CHIP, TRACKER, PRE (PRESET), OCTAVE, CARDS, STORE, KEYS, SOUNDS, ROWS (TRACKER ROWS), THEME, STATUS, SCENE, CHARSET and ABOUT; six lines show at a time and the list scrolls with it, the title bar's arrows telling which way more is | | [ ] |
| ALGORITHM (encoder 1), in the menu | changes the item under the selection (PRESET; OCTAVE -3..+3; CARDS: counter-clockwise OFF, clockwise ON; THEME: counter-clockwise HIGH, the default, clockwise C64: every screen, the menu included, changes colours at once); clockwise on CHIP opens the CHIP view behind THE CHIP's title card (below), on STATUS the status screen (the numbers), on SCENE the generative pieces (below), on CHARSET the glyph sheet, on ABOUT the credits (the chrome lettering at the top, the credits pages fading over stars, raster bars behind a white sine scroller), and HOME goes back to the menu | | [ ] |
| SELECT (encoder 0), on CHIP | the focus (the yellow tab, or the yellow title on the A/B tabs) moves tabs, CHIP, DIST, MOD, FX, EXPERT and stops at either end; onto a slot that was not the selected one, the toast says SLOT DIST (or MOD, FX); onto the slot already selected (DIST at power-on), no toast | | [ ] |
| ALGORITHM (encoder 1), on CHIP | on CHIP the PROFILE steps 6581 R2, 6581 R3, 6581 R4AR, 8580 R5, WORN 6581, then filled USER slots (7.2; MODEL follows; held notes attack again on the new chip, as at a model change); on a slot its algorithm, OFF to the last, at 3× under the tabs | | [ ] |
| K1..K4, on CHIP | on CHIP AGE, DAC, THUMP, BLEED (values at 3× with bars, in the knob's colour; the sound darkens with AGE, a released note whispers with DAC); on a slot its four controls, named for the algorithm (OVERDRIVE: DRIVE, TONE, BIAS, MIX) | | [ ] |

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
  SELECT does nothing outside the menu, CHIP, SCENE and TRACKER, ALGORITHM nothing
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

- [ ] At rest the three scopes are flat dark grey lines two pixels thick, the voice numbers
  1 2 3 in light red, cyan and light green, the three notes `---`, the VU meters empty, the
  knob row (CUTOFF, RESO, PULSE, PRESET) in yellow, cyan, light green and light red, and
  nothing flickers.
- [ ] A held key: its voice's scope turns its colour and shows the waveform (SAW LEAD a
  sawtooth, PWM BASS a pulse whose width moves, SYNC LEAD voice 1's synced saw, NOISE PERC
  noise), standing still while the note holds, two pixels thick; the note's name at 2× on
  the right and ATK, DEC, SUS, REL under it; its VU bar moves. Higher notes show more periods.
- [ ] Released, the note's name dims, and the scope goes flat and dark grey again once the
  release has faded.
- [ ] A three-note chord: all three scopes move; with noise on all three
  (CC 20 = 64) they keep moving, taking turns, with no tearing.
- [ ] K1 and K2 move the CUTOFF and RESO values and their bars (the cutoff as `980`, `1.2K`
  or `12K`); MIDI CC 23 changes the filter's mode at 2× (LP, BP, HP, NOTCH...). K3 moves
  PULSE (a percent) and K4 changes the PRESET, its three letters in the fourth column and its
  name at the top.
- [ ] The L and R meters follow the output; with REVERB on, the tail shows
  on them after the note.
- [ ] No click or crackle in the sound while LIVE shows, against the menu
  (LIVE taps the audio interrupt; the menu does not).

Large text, the menu and the value popup ([controls](reference/controls.md#value-popup);
`docs/art/README.md` "Large text"): the first hardware run found the 8×8 font almost
unreadable on this panel. Look at arm's length, at a desk and held in the hand:

- [ ] **The menu** (HOME): the title bar and six lines are readable without leaning in, the
  selected line's reverse video is clear at a glance, nothing sits under the bezel at the
  left or right edge (a cell of margin), and the top line of the title bar is whole. Scroll to
  the end and back: the lines come in a line or two a frame (a short sweep, no tearing, no
  stray cells), the arrows in the title bar say where more is, PRE shows NOISE PERC without
  touching its label, STORE reads `193K`, ROWS BAR or SCROLL and THEME HIGH or C64. Every one of
  the 15 items can be reached and read.
- [ ] **The popup**: turn K1 on LIVE: CUTOFF and its value in Hz (`1.2K`) in large letters
  over the middle of the screen with a bar in the knob's colour (yellow for K1), opening in about a tenth of a second; keep
  turning and it stays; stop and it goes about a second later, LIVE back whole (scopes, the knob
  row and the slots as before, no stray cells). Each of these says its own name and value
  readably, in the large size where it fits: K2 RESO, K3 PULSE (a percent), K4 PRESET (a name,
  no bar), EDIT + K1..K4 (`DIST DRIVE` and the like, in the smaller size), MASTER VOLUME, and on
  CHIP K1..K4 AGE, DAC, THUMP, BLEED and ALGORITHM the PROFILE or the slot's algorithm. Over
  the menu, ALGORITHM on OCTAVE shows OCTAVE and `+1`. SELECT opens nothing.
- [ ] **MIDI never opens it**: sweep a controller (CC 74) over MIDI while LIVE shows: the
  CUTOFF value and bar move, no popup.
- [ ] **A popup while notes sound**: hold a chord on LIVE and turn K1: the scopes under the box
  are hidden, the rest keeps moving, the sound has no click or dropout, and the box goes
  cleanly a second after the last turn.
- [ ] **Toasts**: OCT+ shows `OCTAVE +1` in large letters on the bottom two rows, readable, for
  about 1.5 s; SAVE shows `SAVED`.

Readability of the redesigned screens ([the theme and layouts](art/README.md#theme)): each is
drawn to be read at the panel's 23 mm. Hold the FM-1 at arm's length, at a desk and in the
hand, in the HIGH CONTRAST theme (the default) and again after THEME to C64 in the menu:

- [ ] **LIVE**: from across a desk you can tell, without leaning in, which voices sound (the
  scopes in their colours against dark grey), each voice's note, and the four knobs' values
  (the cutoff, resonance, pulse width and preset) in their colours. The 2-pixel traces are
  clean lines, not a ladder of dots. The small text (the waveform under each number, the stage
  under each note, the knob names, the slot lines) is the part you read when you look at it,
  not from across the desk: note which of it you cannot read at all.
- [ ] **TRACKER**: one voice's twelve rows at 2× are readable at a glance (the note and the
  instrument), the current row's dark grey bar is clear against black, and the other two
  voices' strips show where they play; the row numbers and the commands (small) are legible
  in the hand. SELECT switches voices without a flicker. Mute a voice: its number and strip
  are visibly grey. Note anything you cannot read.
- [ ] **CHIP**: the four tabs, the choice at 3× and the four controls at 3× are readable from
  arm's length; the values in knob order match the knobs under your fingers; three digits at 2×
  (from 100) are still easy. The A/B title and KEYS marker, and the SAVE TO USER n line (yellow,
  small), can be found when you look for them.
- [ ] **STATUS and ABOUT**: the 2× title and the key numbers are readable; the small lines are
  as before.
- [ ] **MENU and the MIDI page**: the menu's fifteen items at 2× (THEME and MIDI included)
  read from arm's length; the MIDI page (HOME, then MIDI) is small text in the theme's colours
  (the items white, the help lines grey): note whether you can read its ten items in the hand.
- [ ] **Contrast**: the grey labels and dark grey inactive things are visible but clearly
  second; in the C64 theme the same is true on blue. Note any pair of colours that vanishes on
  the panel (the dark grey on black especially, which the panel may show darker than the
  monitor).

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
  CCs): AGE 100, DAC 40, THUMP 64, BLEED 20; DIST OVERDRIVE, MOD CHORUS, FX REVERB. CHIP A at
  2× in white, the CHIP tab yellow, `6581` and the profile at 3×, the four small control labels
  in yellow, cyan, light green and light grey (K4 white in C64), their bars in the knob's
  accent colour, and their values white at 3× (AGE's `100` at 2×, which is how three digits
  fit). The K1..K4 tags, names below the choice, KEYS marker, note and help lines use the
  readable hint colour; the three slots' algorithms are at 2× below.
- [ ] SELECT through DIST, MOD and FX: each tab turns yellow in turn, the choice at 3× is the
  slot's algorithm, its controls are named for it (OVERDRIVE: DRIVE, TONE, BIAS, MIX), and the
  lines below are the chip and the other two slots. With every slot OFF (ALGORITHM
  counter-clockwise on each), a slot's control labels, values and bars are dark grey under
  their generic names (AMT, A, B, MIX), but the K1..K4 tags remain readable in the hint colour.
- [ ] ALGORITHM on CHIP between two 6581 profiles (R2 and R3, R3 and R4AR, either way) shows
  no toast and no raster, by design: only a model change (to or from 8580 R5) says CHIP 6581
  or CHIP 8580 and sweeps the border. The profile word changes either way.
- [ ] Turning K1 fast end to end on CHIP: the AGE value and bar follow without lag or torn cells,
  and the sound darkens smoothly, with no click.
- [ ] MIDI CCs for every slot and character control at once (a controller's snapshot,
  or a DAW sending them in one go): the page catches up within a few frames, no cell
  left stale.
- [ ] SELECT one stop after FX opens EXPERT. ALGORITHM steps its four pages and stops
  at either end; SELECT back returns to FX. The four physical knobs turn the labelled
  controls (K1/K2 upper pair, K3/K4 lower pair); the last page's unused positions do nothing.
- [ ] From CHIP and from EXPERT, press SEQ for TRACKER, then HOME immediately and again
  after TRACKER has finished drawing: CHIP returns whole, with no tracker rows left in it.
  Repeat with MIDI LEARN open over the returned CHIP page. Open SONG, return to TRACKER,
  and check its rows/name are restored rather than stale CHIP or SONG text.
- [ ] In both HIGH CONTRAST and C64 themes, the expert names at 2× and unit values at
  2×/3× are readable in the hand and at arm's length. Active large names keep the knob accents:
  K1 yellow, K2 cyan, K3 light green and K4 light red (light grey in C64). The small knob tags
  and the macro reminder use the neutral, readable hint colour, not the physical knob accent.
  Popup bars retain their theme colours (the knob accent in HIGH CONTRAST, light green in
  C64). Every popup names its own control; no value overlaps its neighbour or runs under the
  bezel. Model-specific inactive names and values dim, values say N/A, and tags remain readable.
- [ ] With a fresh R4AR setup, following defaults read 100% nonlinearity, 0.75% DAC leakage,
  1.16 DC filter bias and 13154 Hz external low-pass. These are physical units, not raw
  MIDI values. Compare the four pages with `chip-expert-1.png` through `chip-expert-4.png`.

SCENE (menu, SCENE; [controls](reference/controls.md#scene)):

- [ ] It opens on FIRE: black, with a brown ember line at the bottom that
  holds still in silence.
- [ ] SELECT clockwise: PLASMA, RULE 30, STARS, ANT, RED MOON RITE, then
  FIRE again, each wiping in from the top with its name as a toast;
  counter-clockwise goes back. HOME returns to the menu.
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

RED MOON RITE (SELECT counter-clockwise from FIRE; [art](art/README.md#red-moon-rite),
[controls](reference/controls.md#red-moon-rite)): the panel's look of the drawn piece, which
only the real LCD shows.

- [ ] Still, it looks like `tests/golden/lcd/scene-rmr-song.png` (two barbarians in guard
  under a blood moon, a ruin with two torches, a broadsword stamina blade a side and VS):
  the colours as in the PNG on a computer screen (no colour swapped or washed out), the
  wide pixels square-ish, no torn or shifted 8 x 8 cell.
- [ ] The dither (the moon's halo, the sky's seams between bands, the flagstones' half
  tones, the fighters' shading): does any area shimmer or crawl, or show moire, when the
  piece is still? Note where.
- [ ] PLAY (button 12): the song plays the duel. Kicks chop, snares parry with a white and
  yellow spark, the hats shift the guards, the torches flicker and flare on the lead's notes;
  near the end of the fourth bar the spinning attack knocks a sword away, it spins through
  the air and sticks in the floor, and a beat later its owner bends and pulls it free. The
  motion looks smooth (25 frames a second) with no cell lagging behind the rest of a
  fighter for more than a frame, and the sound stays clean.
- [ ] K1 (the keys' cutoff, with the song stopped) or the song's filter darkens and
  brightens the sky in three palettes, wiped down a row at a time.
- [ ] PLAY again (stop): once the notes have faded the duel settles and the torches hold
  still; the picture stays clean (no stale or flickering cell).
- [ ] Hold SAVE and turn K1 (MIDI learn's frame) over it: the frame reads as over the other
  pieces (white on blue), the duel goes on around it, and HOME brings the duel back whole.

### 6.3 Two chips, routing and UI sounds (M2b)

These are listening/panel checks for the user; emulator instruction counts do not mark
them passed. Use a moderate headphone level. Keep the USB-output release gate in section
8.1 open until it passes independently.

- [ ] With KEYS AUTO and the song stopped, the keys sound on A as before. PLAY starts the
  song on A; newly played keys sound independently above it on B, with the same keys'
  profile and character. LIVE says KEYS:B. Stop the song with a key held: it continues
  without a cut; after release and A's preparation, new keys return to A. B's release
  tails finish without a click or a frozen tail on its next use.
- [ ] Hold keys across PLAY: they re-sound on B while the song starts on A. No stuck note
  remains on either chip after releasing the keys and stopping the song.
- [ ] On CHIP, SELECT counter-clockwise to the A/B tab focus, then ALGORITHM between A and
  B. The title and KEYS marker follow the chosen chip. With no song, B says FOLLOWS KEYS;
  editing it changes the keys' setup. While a song plays, A edits the song's setup and B
  the keys'. Effects remain one shared chain, with the B tab's shared-slot indication.
- [ ] Select KEYS A in the menu and play the song: a held key temporarily takes a song
  voice; releasing it gives the voice back, with no stuck gate. The song keeps its filter
  and volume; the instrument's filter knobs do not take them over. Restore AUTO.
- [ ] While a chord holds on the keys' chip, step a built-in profile and toggle FX both
  ways. No silent block, dropout or crackle occurs during preparation. A profile/model
  change intentionally re-attacks held notes; an expert edit below must not.
- [ ] SOUNDS is OFF by default: menu moves and confirmations add no sound. Turn it ON:
  menu SELECT gives a blip, confirmation a confirm sound, a toast its chime, a completed
  explicit SAVE the drive chunk, and a refused action a buzz. They do not change the
  keys' setup or effect settings. Restore OFF and check it remains OFF after a quiet save
  and power-cycle.
- [ ] With AUTO and a song playing, hold three keys on B and move through the menu with
  SOUNDS ON: no cursor sound and no held note stolen or changed. With a UI sound playing
  and the other B voices busy, a new note takes priority immediately; its release leaves
  no stuck note. Turn SOUNDS OFF afterwards.
- [ ] Song plus keys at a loud setting: listen for clipping at the shared mix and record
  the profiles/notes/MASTER level if heard. Two loud, in-phase chips can clip; the build
  does not apply automatic attenuation or a soft knee. Also listen during expert edits,
  screen transitions and a USB replug; record any click, dropout or late note.

### 6.4 Expert edits without a note reset (M2b Task 6)

- [ ] Hold a triangle note (or a steady chord) while changing every applicable expert,
  including filter curve/range and DAC kink/leakage. Pitch/envelope/gate stay continuous:
  no new attack, silent gap or click. Table changes may glide over several ticks; check
  both the 6581 and 8580, skipping their N/A controls. The external corners change tone,
  combined strength changes combined waves, and noise feedback changes a noise combination.
- [ ] Edit 6581 curve, then move AGE: the curve returns to AGE's path. A filter-bias edit
  stays when AGE moves. DAC reclaims only kink/leakage, THUMP only output thump, BLEED only
  filter bleed. A different built-in profile/model clears all overrides; a clamped picker
  step or an empty USER choice leaves them intact. The page's macro reminder is readable.
- [ ] Send the generated chart's expert NRPNs 2/5–18 over TRS and USB: their values and the
  sound follow, with no popup opened by MIDI and no echo back. On EXPERT, SAVE held + a
  knob learns that expert without changing it; a learned CC can reach both endpoints.
  In a USB monitor, a local expert turn sends its NRPN, or its learned CC. Mixed macro and
  expert automation reaches the final values without a stale override arriving later.
- [ ] Save and recall experts with a USER profile (section 7.2), including on B and on
  the song's A tab. Release notes and wait at least 6 s after expert/settings changes,
  then power-cycle: the keys' explicit experts and learned bindings return; following
  fields still follow a subsequent related macro move.

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

Use a writable store for the persistence checks. Previous ChoralRoot/Felucca data in the
store's range reads as FOREIGN, not as a blank store. Read the destructive format check
below before deciding whether to format; nothing persists until then. SIDOU never formats
it without the menu's confirmation; preserve wanted data with the previous firmware first.

- [ ] Change CARDS, KEYS, SOUNDS and ROWS (TRACKER ROWS), the preset and a preset edit, the
  chip profile and a character control, an effect algorithm and Mix, and the envelope mode (CC 26). Release
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
- [ ] In the menu, STORE is immediately after CARDS and shows nK (the kilobytes free), excluding the
  reserve pool. ALGORITHM on a writable SIDOU store does not format it.
- [ ] **Only with unwanted foreign data and permission to destroy it:** STORE says FOREIGN.
  A plain SAVE says STORE FOREIGN: NOT SAVED; power-cycling without formatting leaves the
  region foreign. ALGORITHM clockwise once on STORE says FORMAT?; counter-clockwise
  cancels, as does moving SELECT away and back. Nothing is erased by those cancelled steps.
- [ ] **Destructive confirmation:** on FOREIGN, turn ALGORITHM clockwise twice without
  leaving STORE. The store's 73 sectors are erased; STORE shows ERASE n (sound can have
  gaps), then STORE FORMATTED and nK. Let settings save, power-cycle, and check that
  STORE is no longer FOREIGN and settings persist. Never format automatically just to
  proceed with the checklist.

Notes:

## 7.2 USER chip profiles (M2a Task 6)

Use a writable store (section 7.1). CHIP's chip or EXPERT focus, not its tabs or effect
slots, owns profile saves. A saved slot contains a base profile, AGE, DAC, THUMP and BLEED,
and all fourteen expert overrides (including fields following their defaults); the
settings keep the base setup, not the identity of the USER slot picked.

- [ ] On CHIP, select a built-in profile and set recognisable character and expert values. Hold EDIT
  and turn ALGORITHM to target an empty USER slot: the yellow title says SAVE TO USER n
  (with `-` for empty), and the popup names the target. Neither the current profile nor
  the sound changes. Step through USER 1..8 and OFF; leaving chip focus or closing CHIP
  clears the target.
- [ ] Target an empty slot again, then press and cleanly release SAVE. Wait for
  SAVED TO USER n (or SAVED TO USER n (GAPS), after the 2 s forced path). The chip's setup
  has not changed. Power-cycle after completion: the slot remains. ALGORITHM's picker
  runs the five built-ins then filled USER slots, skipping empty ones with USER n EMPTY
  at an empty end. Pick the saved slot: its base model and four controls, and its sound,
  return, including the expert overrides; the PROFILE row and popup name USER n. Repeat
  EDIT + ALGORITHM and SAVE from EXPERT, selecting the target again after reaching it:
  passing through DIST, MOD or FX clears the previous target.
- [ ] Change a character control while that USER slot is applied, then SAVE with no target:
  OVERWRITE USER n? first. HOME keeps the old slot and leaves the view, clearing the
  question/target. Repeat, then a fresh SAVE press and release confirms. Reboot and pick
  the slot: only the confirmed changes survived.
- [ ] Target a filled slot without applying it: SAVE asks before overwriting, and the
  current sound is stored only after the second SAVE. With a built-in profile and no
  target, SAVE instead saves settings (SAVED), not a USER slot.
- [ ] Repeat the save on chip B's tab showing the keys' setup. During a song, save chip A's
  setup from the A tab: the slot holds the song's setup, not the keys' or a temporary lock.
- [ ] CC 85 / NRPN 2/0 can pick a filled USER slot (profile range 0..12); an empty one changes
  nothing. Use the current [MIDI chart](reference/midi.md), since the CC scaling changed
  when the eight slots were added.
- [ ] If testing an unformatted foreign store, a profile save says STORE FOREIGN: NOT SAVED;
  it does not format or replace the foreign data. A RAM-only slot may work until reboot,
  but it is not persistent. Only the destructive confirmation in section 7.1 formats it.

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

### 8.1 Before any release: USB-MIDI out (must pass)

v0.2.0-m2-preview received USB-MIDI (notes, the upgrade request) but almost
nothing it sent reached the host. The USB fix (no `idle` while a host has the
device configured, no USB poll nested in the audio interrupt, IN packets of
16 bytes) is checked in fm1-emu against a modelled host
(`host/crates/emu-gate/tests/usb.rs`), which cannot show the chip's own
behaviour. **No release goes out unless every item here passes on a real
FM-1 with that release's package**, each from a cold boot and again after a
USB replug:

- [ ] `fm1_install.py --info` answers with SIDOU's identity (`FM-1_900`
  for development builds). Record its output:
- [ ] A reinstall over SIDOU (`fm1_install.py sidou.fwsc`, SIDOU to SIDOU)
  completes: the read requests reach the host, the loader writes, and the
  unit boots the new package (`--info` again).
- [ ] In a MIDI monitor on the SIDOU input, K1 sends CC 74 on channel 1
  while it turns, and K4 a program change per detent. Record what the
  monitor shows:
- [ ] The same after deleting the device from the OS's MIDI setup (macOS:
  Audio MIDI Setup) and replugging.

If an item fails, record it, and get back to a working firmware with the
power-on gesture (section 0) rather than the installer.

Notes:

## 8b. MIDI clock and transport (M2e Task 2)

The song is SIDOU's first song (PLAY). The MIDI page is HOME, then MIDI at the end of the
menu; CLOCK is AUTO and CLOCK OUT OFF at power-on.

- [ ] **Follow a DAW over USB:** set the DAW to send MIDI clock and transport to SIDOU at
  120 BPM and press its play: SIDOU's song starts on the DAW's downbeat and stays with it for
  a minute (record the DAW's metronome and SIDOU together: the rows on the clicks, no drift).
  Change the DAW's tempo to 90 and to 140 while it plays: the song follows within a beat or
  two. Stop the DAW: the song stops. Record what you hear at the start:
- [ ] **The same over TRS:** a hardware sequencer or a DAW's MIDI interface into the TRS
  jack: as above.
- [ ] **Song position:** in the DAW, stop, move the play head to bar 3 and press play
  (continue): the song plays from bar 3, in step with the DAW.
- [ ] **A clock that stops:** with CLOCK at AUTO, unplug the clock's cable while the song
  plays: the song carries on at the tempo it had. With CLOCK at FOLLOW, the same stops it.
- [ ] **Lead a DAW:** set CLOCK OUT to ON on the MIDI page and set the DAW to follow MIDI clock
  from SIDOU over USB. Press PLAY on SIDOU: the DAW starts and follows the song's tempo;
  PLAY again stops both. Record the tempo the DAW shows:
- [ ] **No echo:** with CLOCK OUT on and the DAW sending clock to SIDOU, the DAW's MIDI
  monitor shows no clock coming back from SIDOU.

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
the normal SIDOU, so the normal installer always reaches it. These builds
leave out the power-on UBOOT gesture (section 0), whose key is the same: their
OCT- at power-on dies instead. Never give these builds to anyone else, and
install the normal package again when you are done.

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
- [ ] Normal build: hold OCT- alone while powering on, about a second: the
  ROM boot mode, as the guard's (section 1 has the full check). Record
  whether the guard's UBOOT and this one look the same on USB:
- [ ] Normal build: power-cycle quickly (off for under a second) three times
  within ten seconds of each boot. Record whether it ends in UBOOT (it
  should not, if RAM does not survive a power cut, but a brownout may keep
  it):

Notes:

## 11. Measurements for later work

Record what you can. Counter/CRC microbenchmarks need a measuring build;
the store's listening checks use normal SIDOU, and section 12's flash spike is built.

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
- [ ] MASTER controls a song's level (M2 hardware fix): play a song (PLAY, the demo
  song or any other) with MASTER fully up, then turn MASTER down slowly to zero and
  back: the song's level follows the pot smoothly, with no zipper or click, down to
  silence at the bottom. Start a song with MASTER low and raise it. Press keys over a
  playing song (KEYS AUTO plays chip B): they follow MASTER too, and a UI sound (the
  menu's) is quiet when MASTER is. With MASTER full the song is as loud as before.
- [ ] The tail goes silent (M2 hardware fix): play a chord on a saw or pulse
  preset with the character at 0 (BLEED 0), release it and wait. Headphones
  or a quiet amp, volume up: about 50 ms after the release ends there is no
  faint tone and no hiss left over from the voices, and no click when it
  ends or when the next note starts. Then set BLEED up (CHIP view, K4) and
  repeat: the leak stays, as it should. Play two notes less than 40 ms
  apart (a fast trill): nothing drops out or changes.
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

## 13. Songs on the store (M2c Task 8)

These checks are for the user to run on hardware; automated testing uses fm1-emu only.

- [ ] Open SONG, choose NEW, and confirm discarding changes if asked. Check the three
  64-row patterns and the pulse lead, bass and noise drum instruments.
- [ ] Choose SAVE AS, enter a 16-character name with SELECT and ALGORITHM, and save.
  Wait for SONG SAVED before switching the power off.
- [ ] Power on again: the last song loads stopped. PLAY sounds the saved song.
- [ ] SAVE an unchanged song: SONG SAVED appears without a flash-write delay.
- [ ] During a save, edits are refused with SAVING; playback, mute, solo and jam continue.
- [ ] Near the saved-size limit, an edit that would exceed 65,536 bytes is refused
  with SONG FULL and leaves the song unchanged.
- [ ] With a clean, stopped song, open ABOUT: its tune plays. HOME stops it and
  restores the loaded song, stopped. With unsaved changes, ABOUT stays silent and
  its scroller says SAVE YOUR SONG FOR MUSIC.
- [ ] DELETE asks for confirmation. Cancelling keeps the file; confirming removes it.

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
