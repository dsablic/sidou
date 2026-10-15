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

- [ ] The title card appears: Cracktro Chrome's scene, the chrome SIDOU
  letters with their purple shadow over stars, raster bars and a grid floor
  (as `assets/logo/cracktro-chrome.png`; look for wrong colours, a shifted
  row or torn cells), the version at the top and "PRESS ANY KEY" blinking at
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
| EDIT (button 6) held + K1..K4 | the selected slot's Amount, A, B, Mix (the numbers on the status screen; Mix as LIVE's bar); EDIT lit while the slot is on | | [ ] |
| HOME (button 8) | from LIVE, the menu (a wipe down from the top); in the menu, back to LIVE (a wipe up from the bottom) | | [ ] |
| SAVE (button 9) | a MIDI LEARN frame at the bottom of the screen, TURN A KNOB (section 7, MIDI learn); HOME closes it | | [ ] |
| SELECT (encoder 0), in the menu | the blinking block cursor moves between CHIP, PRESET, OCTAVE, STATUS, CHARSET and ABOUT | | [ ] |
| ALGORITHM (encoder 1), in the menu | changes the item under the cursor (CHIP: clockwise 8580, counter-clockwise 6581, and a further turn the same way changes nothing; PRESET; OCTAVE -3..+3); clockwise on STATUS opens the status screen (the numbers), on CHARSET the glyph sheet, on ABOUT the credits (the chrome lettering at the top, the credits pages fading over stars, raster bars behind a white sine scroller), and HOME goes back to the menu | | [ ] |

- [ ] A held note keeps its pitch when the octave changes, and stops when
  the key is released.
- [ ] OVERDRIVE with Amount and Mix up distorts audibly; REVERB or LUSH with
  Mix up adds a stereo tail (listen on headphones); switching algorithms
  while a note plays crossfades without a click.
- [ ] In the menu, on the status screen and on the glyph sheet, the keys,
  K1..K4, OCT-/OCT+, FX and the EDIT layer play as on LIVE.
- [ ] OCT+, SCL and ENV show a toast on the bottom line ("OCTAVE +1",
  "SLOT MOD", "MOD CHORUS") that goes after about 1.5 s; FX shows
  "CHIP 8580" and sweeps colour bands down the border for about half a
  second.
- [ ] The other buttons and encoders do nothing (record anything they do);
  SELECT and ALGORITHM do nothing outside the menu.

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

**MIDI learn (M1d)** ([controls.md](reference/controls.md), MIDI learn), with a controller
that sends a CC the map leaves free (e.g. CC 14; [midi.md](reference/midi.md) lists the
map's) on the TRS jack. While the frame waits for a CC, any learnable CC binds, whatever
sends it: a DAW that sends CC 0, 32 or 64 when its transport starts, or a controller's
pedal, binds that instead. Stop the transport and keep pedals still while learning.

- [ ] Press SAVE: the MIDI LEARN frame says TURN A KNOB, and stays after SAVE comes up.
  Turn K1: CUTOFF &larr; SEND A CC, and the cutoff does not move. Move the controller's
  knob: LEARNED CC 14 &rarr; CUTOFF, the frame closes, and the controller's knob now sweeps
  the cutoff (the bar and the sound) on any channel. CC 74 still moves it too.
- [ ] The same over USB from a DAW, and the other way: learn K2 on CC 15 from USB, then
  send CC 15 from a TRS controller: both ports drive what either learned.
- [ ] EDIT held + K4 while learning, then a CC: the selected slot's Mix follows it
  (LEARNED CC n &rarr; DIST MIX; SCL first for MOD or FX).
- [ ] Learn the resonance on CC 74: CC 74 now moves the resonance, not the cutoff (the
  cutoff's NRPN 0/0 still does).
- [ ] With a DAW monitoring SIDOU's USB output, turn K1 after learning CC 14 for it: the
  monitor shows CC 14, not CC 74. Moving the controller's knob sends nothing back.
- [ ] HOME while the frame shows: LEARN CANCELLED, no menu, nothing learned. Leave the
  frame alone for 10 s: LEARN TIMED OUT.
- [ ] SAVE, K1, SAVE: CUTOFF: CC 14 FORGOTTEN, and CC 14 does nothing. SAVE, SAVE, SAVE:
  MIDI LEARN CLEARED.
- [ ] The note keys play while the frame shows, and the sound has no gap when it opens or
  closes.
- [ ] Power off and on: nothing is learned any more (RAM only until M2).

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
  CCs 103..106 for the Distortion slot (SCL moves to 108..111 and 113..116). Record
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
| 8 USB MIDI | | |
| 9 Reinstall, stock and back | | |
| 10 Guard, watchdog, resets | | |
| 11 Measurements | | |
