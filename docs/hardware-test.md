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

## 2. An interrupted install resumes

Do this only with section 0 ready.

- [ ] Pull the USB cable during step 2 (the write). Plug it back: the unit
  comes up as the update loader again, and running `fm1_install.py
  sidou.fwsc` again finishes the install.
- [ ] Power-cycle the unit during step 2. It comes up as the update loader
  again, and the install finishes as above.

Notes:

## 3. Boot

- [ ] The status screen appears: C64 blue with a light blue border,
  "SIDOU V0.0.0", CHIP 6581, PRESET SAW LEAD, OCTAVE 0, four bars, "READY.".
  Time from power-on to the screen:
- [ ] No sound, click or hum before a key is played.
- [ ] Power on and off ten times: it boots every time.

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

- [ ] A held note keeps its pitch when the octave changes, and stops when
  the key is released.
- [ ] The other buttons and encoders do nothing (record anything they do).

Notes:

## 6. LCD

The panel runs at 12 MHz SPI (Felucca's setting).

- [ ] Text is sharp, colours right, no stray pixels or shifted strips.
- [ ] Only what changes is redrawn: no flicker while idle, none while
  turning K1 fast or playing fast notes.
- [ ] Nothing goes wrong on screen while the audio plays (no clicks either).

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
- [ ] A dense part from a sequencer (running status, clock on) plays without
  hanging notes. Latency feels:

Notes:

## 8. MIDI over USB

- [ ] The unit enumerates as a USB-MIDI device "SIDOU" by "SIDOU
  contributors" (pid.codes test VID/PID 1209:0001). Record what the OS shows:
- [ ] The same note, CC and program tests as section 7 pass over USB.
- [ ] TRS and USB at the same time both play.
- [ ] Unplug the USB cable while playing (on battery): the sound goes on
  without dropouts, and plugging it back in enumerates again within a few
  seconds.
- [ ] Put the computer to sleep and wake it while the unit plays: no
  dropouts, MIDI works again after the wake.
- [ ] On battery with no cable for five minutes: the keys and screen stay
  responsive and the sound clean (USB is retried once a second).
- [ ] If you can measure it: supply current with no cable:

Notes:

## 9. Back to stock V15, and SIDOU again

- [ ] From SIDOU, `fm1_install.py FM-1.fwsc` (the unmodified official V15)
  installs: SIDOU's step 1 accepts it (its flash head is the unit's), the
  official loader writes it, and `--info` reports `FM-1_015`.
- [ ] The stock firmware plays its patches as before.
- [ ] Installing `sidou.fwsc` again from stock works (section 1).

Notes:

## 10. Boot guard, watchdog and resets

These need checklist builds that die on purpose, one second after boot,
before the boot counts as stable (10 s). Build them as BUILDING.md says, with
`-DSIDOU_TEST_DEATH=hang` (interrupts off: the watchdog resets the chip
after about 8 s) or `-DSIDOU_TEST_DEATH=reboot` (a P33 reset, as the fault
handler does), and package each as usual. Never give them to anyone else.
Section 0 must be ready: the expected end is UBOOT.

- [ ] Hang build: boot 1 dies, the watchdog resets, boot 2 dies, and boot 3
  enters UBOOT instead of SIDOU. Record what the screen shows and how the
  unit appears on USB in UBOOT. This shows the guard state in `.noinit`
  survives a watchdog reset.
- [ ] In UBOOT, the unit stays there (no watchdog reset) for at least five
  minutes.
- [ ] From UBOOT, install the normal `sidou.fwsc` with a recovery tool from
  section 0, and record which one worked and how:
- [ ] Reboot build: the same, with the guard surviving a P33 reset.
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
- [ ] USB poll cost with a host attached (three SIE round trips per poll at
  2 kHz).
- [ ] Divide and IRQ-entry latencies.
- [ ] Core 1 is not used by SIDOU; its RAM self-test (Felucca's) is for a
  later milestone.

Notes:

## Result

| Section | Pass / fail | Notes |
|---|---|---|
| 0 Recovery ready | | |
| 1 Install | | |
| 2 Interrupted install | | |
| 3 Boot | | |
| 4 Keys and LEDs | | |
| 5 Buttons, encoders, MASTER | | |
| 6 LCD | | |
| 7 TRS MIDI | | |
| 8 USB MIDI | | |
| 9 Back to stock and again | | |
| 10 Guard, watchdog, resets | | |
| 11 Measurements | | |
