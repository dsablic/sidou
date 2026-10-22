<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Flash day: the checklist

One page for putting a SIDOU build on a real FM-1, in order. It is the routine after every
large merge. The full checklist is [hardware-test.md](hardware-test.md). Stop at the first
step that fails and go to "If a step fails". Do not skip ahead.

Each step says who does it:

- **[script]**: a script can do it over USB-MIDI, or from files, with nobody at the unit.
- **[hands]**: a person at the unit: keys, power, cable, a screen to read, sound to hear.

The installer is Felucca 1.6.1's `tools/fm1_install.py` (needs `mido` and `python-rtmidi`).
It finds a unit running SIDOU without `--port`: the port is "SIDOU FM-1".

## Before the install

- [ ] **[script]** Build the package from the commit under test in a **fresh build
  directory** (BUILDING.md; a stale CMake cache can build a different firmware), or take the
  release's.
- [ ] **[script]** `tools/preflash build/fm1/sidou.fwsc --installer <fm1_install.py>
  --unbricker <MvaveFM1Unbricker folder> [--v15 FM-1.fwsc]` prints only PASS (a SKIP for
  something you will not use is fine). It reads files only, never a device: the package's
  SHA-256, identity, sizes and CRCs, the update loader (Felucca 1.6.1's, which refuses a
  package of the wrong size before erasing anything), the recovery route and the V15
  `FM-1.fwsc` (SHA-256 `db1642b2...edb8a`). It ends with the exact commands to run.
- [ ] **[hands]** Only the FM-1 plugged in as a MIDI device, with a USB cable that carries
  data. On a Mac, after a firmware switch, delete the old USB-MIDI device in Audio MIDI Setup.
- [ ] **[script]** `fm1_install.py --info`: note the firmware on the unit now. The installer
  has no backup: save the songs you want first (SysEx GET,
  [section 8c](hardware-test.md#8c-sysex-files-instruments-and-songs-m2e-task-5)).

## The install: over V15 (or SIDOU) with the normal installer

- [ ] **[script]** `fm1_install.py sidou.fwsc --yes`. Step 1 (the running firmware stages the
  loader) reports no error; the unit restarts as "Felucca Update" (USB 1209:0002,
  `ota-FM-1_900`); step 2 reaches 100 %; the unit restarts.
- [ ] **[script]** `fm1_install.py --info` answers `FM-1_900` (or the release's `FM-1_9XY`) on
  port "SIDOU FM-1".
- [ ] **[hands]** The unit boots SIDOU: the boot screen, then LIVE.

The unit runs M-VAVE's FM1 Open (identity `FM-1_8000013`, port "FM1 Community")? Go to V15
first with `tools/fm1_install_v15_over_open.py <Felucca tools dir> FM-1.fwsc --port "FM1
Community"`, then install as above.

## Then, in order

1. **The OCT- gesture** ([section 1](hardware-test.md#1-install)). **[hands]** Switch off,
   hold OCT- alone, switch on, keep holding about a second: black screen. **[script]** USB
   shows `WL82 UBOOT1.00` (4C4A:8057; only list it, never open it). **[hands]** It stays
   there five minutes; a power cycle without the key boots SIDOU; a brief OCT- tap, OCT- with
   OCT+, and OCT- with a note key boot normally. **This must pass before anything else**: it
   is what makes every later failure recoverable.
2. **The USB update gate** ([section 8.1](hardware-test.md#81-before-any-release-usb-midi-out-must-pass)),
   each from a cold boot and again after a replug **[hands]**:
   - **[script]** `--info` answers; the handshake answers 10 times out of 10; SIDOU's
     inquiry and the universal inquiry answer;
   - **[script]** a reinstall over SIDOU (`fm1_install.py sidou.fwsc --yes`) completes and
     `--info` answers again;
   - **[hands]** turn K1: **[script]** a MIDI monitor on "SIDOU FM-1" sees CC 74 on
     channel 1 (K4 released in LIVE: CC 72);
   - **[hands]** the same after deleting the device in Audio MIDI Setup and replugging.
   A failure here blocks any release.
3. **Readability [hands]**: LIVE, TRACKER, CHIP, menus and popups at arm's length, in both
   themes ([section 6](hardware-test.md#6-lcd)). Note what cannot be read.
4. **The audio checks [hands]**: the note keys and their LEDs, MASTER (a playing song follows
   it), the knob ranges, the quiet floor after a release, no clicks while the screen redraws
   ([sections 4 to 6](hardware-test.md#4-note-keys-and-their-leds)).
5. **The ergonomics recipes [hands]**: `docs/recipes/*.json` on the device, step by step as
   the ergonomics harness records them. Note every place where the device and the harness
   disagree.

## The way back to V15

- **While SIDOU answers over USB [script]:** `fm1_install.py FM-1.fwsc --yes` (the
  unmodified official V15); `--info` then answers `FM-1_015`
  ([section 9](hardware-test.md#9-reinstall-back-to-stock-v15-and-sidou-again)).
- **When it does not [hands, then script]:** the OCT- gesture into ROM boot, then from
  `tools/fm1-unbrick-mac`: `sudo <venv>/bin/python fm1_unbrick_mac.py info` (reads only),
  then `... restore FM-1.fwsc` (two verified backups, write, read back). If a write or verify
  fails, do not power off: run `restore` again
  ([README](../tools/fm1-unbrick-mac/README.md)). Without the gesture: a Flipper Zero
  (flipper-fm1-recovery) or the FM-1 Transporter into ROM boot
  ([section 0](hardware-test.md#0-before-you-start-recovery)).

## If a step fails

- **Step 1 errors, or the install stops before the unit restarts:** nothing is erased yet.
  Replug, `--info`, retry once. "FAILED -15" on the screen (-14 from the loader) is a package
  of the wrong size: run `tools/preflash` on it.
- **The unit sits in "Felucca Update"** (power loss or a pulled cable during step 2): plug it
  back and run the same install again; it resumes.
- **The OCT- gesture fails:** stop, record what you saw, and go back to V15 over USB.
- **The USB gate, readability, audio or a recipe fails:** SIDOU is running and recoverable.
  Record it, then stay on the build to look closer or go back to V15.
- **Nothing responds at all:** the Flipper or Transporter route into ROM boot; SIDOU's path
  never writes the chip's flash head.

## What to record

In a copy of hardware-test.md and a new page under `docs/hardware-results/`:

- the commit, the package's SHA-256 (from `tools/preflash`) and the firmware on the unit
  before (`--info`);
- each step above: passed or failed, with what the screen, the installer and the monitor
  showed (photos of any failure screen);
- the readability and audio notes, and every recipe step where device and harness disagree;
- anything done to recover, and how the unit was left.
